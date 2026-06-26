use std::collections::HashMap;
use std::sync::Mutex;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::{Error, Result};
use crate::storage::DocType;
use crate::websearch;

const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const MAX_TOKENS: u32 = 64_000;

const KEYRING_SERVICE: &str = "com.adamwickwire.markdown";
const DEV_KEYS_FILE: &str = "dev-keys.json";

// Event names shared with the frontend assistant store. Every payload carries
// the stream id the frontend generated for the request, so listeners can drop
// events from a superseded stream (e.g. after switching documents mid-stream).
const EVT_TOKEN: &str = "assistant:token";
const EVT_DONE: &str = "assistant:done";
const EVT_ERROR: &str = "assistant:error";
const EVT_USAGE: &str = "assistant:usage";
/// Carries the raw assistant content-block array (incl. a compaction block) so
/// the frontend can persist and replay it verbatim. Emitted before EVT_DONE,
/// only when a compaction block was produced.
const EVT_CONTENT: &str = "assistant:content";
/// Transient activity line (e.g. "Searching the web…") shown while a tool call
/// runs between streamed text. The frontend clears it on the next token/done.
const EVT_STATUS: &str = "assistant:status";

/// Beta header enabling server-side context compaction.
const COMPACT_BETA: &str = "compact-2026-01-12";

/// Hard cap on web-search rounds within a single user turn — a backstop against
/// a model that keeps calling the tool instead of answering.
const MAX_SEARCH_ROUNDS: usize = 4;

/// The `web_search` tool the assistant may call when web search is enabled. The
/// description steers the model to search for current/factual info and cite it;
/// the schema is a single `query` string. Shared by both provider wire formats
/// (Anthropic tool / OpenAI function) via `web_search_tool_*` below.
const WEB_SEARCH_TOOL_NAME: &str = "web_search";
const WEB_SEARCH_TOOL_DESC: &str = "Search the web for current, factual, or \
    up-to-date information. Returns titled results with source URLs and snippets. \
    Use it whenever the user asks about recent events, or facts you are unsure of \
    or that benefit from live sources — then cite the sources you used.";

/// Input schema shared by both tool encodings.
fn web_search_input_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": { "query": { "type": "string", "description": "The search query" } },
        "required": ["query"],
    })
}

/// Anthropic tool definition (`tools[]` entry).
fn web_search_tool_anthropic() -> serde_json::Value {
    json!({
        "name": WEB_SEARCH_TOOL_NAME,
        "description": WEB_SEARCH_TOOL_DESC,
        "input_schema": web_search_input_schema(),
    })
}

/// OpenAI-style function definition (`tools[]` entry for OpenRouter).
fn web_search_tool_openai() -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": WEB_SEARCH_TOOL_NAME,
            "description": WEB_SEARCH_TOOL_DESC,
            "parameters": web_search_input_schema(),
        },
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Anthropic,
    Openrouter,
}

impl Provider {
    fn key_name(self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic-api-key",
            Provider::Openrouter => "openrouter-api-key",
        }
    }

    /// Defaults verified 2026-06-09 (claude-api skill / OpenRouter catalog).
    fn default_model(self) -> &'static str {
        match self {
            Provider::Anthropic => "claude-opus-4-8",
            Provider::Openrouter => "anthropic/claude-opus-4.8",
        }
    }

    /// Cheaper/faster model for short, scoped jobs like inline edits.
    /// Verified 2026-06-10 (claude-api skill / OpenRouter catalog).
    fn fast_model(self) -> &'static str {
        match self {
            Provider::Anthropic => "claude-haiku-4-5",
            Provider::Openrouter => "anthropic/claude-haiku-4.5",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "user" | "assistant"
    pub content: String,
    /// Raw assistant content-block array for a turn carrying a compaction
    /// summary; when present it (not `content`) is sent to Anthropic so the
    /// summary round-trips verbatim. None for user turns and plain replies.
    /// `rawContent` on the wire (the frontend sends camelCase).
    #[serde(rename = "rawContent", default, skip_serializing_if = "Option::is_none")]
    pub raw_content: Option<serde_json::Value>,
}

/// A document the user @-mentioned in chat, attached as background context for a
/// single message. Sent from the frontend (name + content); rendered into the
/// system prompt as a labeled block separate from the editable document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocReference {
    pub name: String,
    pub content: String,
}

/// Tracks the in-flight streaming task (and its stream id) so
/// stop/new-message can cancel it and report which stream ended.
#[derive(Default)]
pub struct AiState {
    current: Mutex<Option<(String, tauri::async_runtime::JoinHandle<()>)>>,
}

// ---------------------------------------------------------------------------
// Key storage
//
// Release builds use the OS keychain. Debug builds use a plain JSON file in
// the app data dir: every dev rebuild changes the binary signature, and the
// keychain would re-prompt for the login password each time.
// ---------------------------------------------------------------------------

fn dev_keys_path(app: &AppHandle) -> Result<std::path::PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| Error::InvalidInput(format!("no app data dir: {e}")))?;
    Ok(dir.join(DEV_KEYS_FILE))
}

fn dev_keys_read(app: &AppHandle) -> Result<HashMap<String, String>> {
    let path = dev_keys_path(app)?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let raw = std::fs::read_to_string(&path)?;
    serde_json::from_str(&raw).map_err(|e| {
        Error::InvalidInput(format!(
            "stored API keys are unreadable ({e}) — delete {} and re-enter them",
            path.display()
        ))
    })
}

fn dev_keys_write(app: &AppHandle, keys: &HashMap<String, String>) -> Result<()> {
    std::fs::write(dev_keys_path(app)?, serde_json::to_string_pretty(keys).unwrap())?;
    Ok(())
}

/// Keychain/dev-keys name for the Tavily web-search key. Stored the same way as
/// the provider keys but not tied to the `Provider` enum (it isn't an AI
/// provider — it's a tool the assistant can call).
const TAVILY_KEY_NAME: &str = "tavily-api-key";

fn keyring_entry(name: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, name)
        .map_err(|e| Error::InvalidInput(format!("keychain unavailable: {e}")))
}

/// Store a secret under `name` (release: keychain; debug: dev-keys file). Shared
/// by the provider keys and the Tavily key so both honor the same invariant.
fn store_key(app: &AppHandle, name: &str, key: &str) -> Result<()> {
    let key = key.trim();
    if key.is_empty() {
        return Err(Error::InvalidInput("API key must not be empty".into()));
    }
    if cfg!(debug_assertions) {
        let mut keys = dev_keys_read(app)?;
        keys.insert(name.to_string(), key.to_string());
        dev_keys_write(app, &keys)
    } else {
        keyring_entry(name)?
            .set_password(key)
            .map_err(|e| Error::InvalidInput(format!("failed to store API key: {e}")))
    }
}

fn read_key(app: &AppHandle, name: &str) -> Result<Option<String>> {
    if cfg!(debug_assertions) {
        Ok(dev_keys_read(app)?.get(name).cloned())
    } else {
        match keyring_entry(name)?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(Error::InvalidInput(format!("failed to read API key: {e}"))),
        }
    }
}

fn remove_key(app: &AppHandle, name: &str) -> Result<()> {
    if cfg!(debug_assertions) {
        let mut keys = dev_keys_read(app)?;
        keys.remove(name);
        dev_keys_write(app, &keys)
    } else {
        match keyring_entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(Error::InvalidInput(format!("failed to delete API key: {e}"))),
        }
    }
}

pub fn set_api_key(app: &AppHandle, provider: Provider, key: &str) -> Result<()> {
    store_key(app, provider.key_name(), key)
}

pub fn get_api_key(app: &AppHandle, provider: Provider) -> Result<Option<String>> {
    read_key(app, provider.key_name())
}

pub fn delete_api_key(app: &AppHandle, provider: Provider) -> Result<()> {
    remove_key(app, provider.key_name())
}

pub fn set_tavily_key(app: &AppHandle, key: &str) -> Result<()> {
    store_key(app, TAVILY_KEY_NAME, key)
}

pub fn get_tavily_key(app: &AppHandle) -> Result<Option<String>> {
    read_key(app, TAVILY_KEY_NAME)
}

pub fn delete_tavily_key(app: &AppHandle) -> Result<()> {
    remove_key(app, TAVILY_KEY_NAME)
}

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

/// The user's global "Voice & tone" guidance, appended to every system prompt.
/// Empty/whitespace → no section, so prompts are unchanged when unset. Placed
/// AFTER each prompt's mechanical rules and self-limited to style, so it can
/// never override the output contracts (e.g. inline edit's "replacement only").
fn voice_section(voice: Option<&str>) -> String {
    match voice.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => format!(
            "\n\nVoice & tone — the user's own description of how their writing should \
             sound (wording, rhythm, tone). Apply it to what you write, but it never \
             overrides the formatting rules above:\n---\n{v}\n---"
        ),
        None => String::new(),
    }
}

/// @-mentioned documents, rendered as a labeled block AFTER the editable
/// document so the model treats them as background, not the doc to revise.
/// Empty → no section (byte-identical prompt to before the feature).
fn references_section(references: &[DocReference]) -> String {
    if references.is_empty() {
        return String::new();
    }
    let mut s = String::from(
        "\n\nReferenced documents the user attached for context — use them as background; \
         do NOT rewrite them or treat them as the document being edited:",
    );
    for r in references {
        s.push_str(&format!(
            "\n\n--- Referenced: \"{}\" ---\n{}\n--- end ---",
            r.name, r.content
        ));
    }
    s
}

/// Guidance added to the chat prompt when web search is enabled. Tells the model
/// it may call the `web_search` tool and must cite what it uses with inline
/// markdown links. Empty when disabled, so the prompt is byte-identical to
/// before the feature. Placed before `voice` (style stays last).
fn web_search_section(enabled: bool) -> String {
    if !enabled {
        return String::new();
    }
    "\n\nWeb search — you have a `web_search` tool. Use it whenever the user asks about \
     recent events, or facts you are unsure of or that change over time, rather than \
     guessing. When you rely on a result, cite that specific claim with an inline \
     markdown link to its source URL (e.g. [source](https://example.com)). Don't dump \
     a list of every result; weave the sourced facts into your answer naturally."
        .to_string()
}

/// Chat system prompt. The model is a writing partner with the live document as
/// context; revisions of the whole doc come back in a ```markdown block so the
/// UI can apply them in one click. `references` are @-mentioned docs added as
/// background; `web_search` adds tool-use guidance; `voice` stays last so style
/// never overrides the format rules.
fn system_prompt(
    document_content: &str,
    references: &[DocReference],
    web_search: bool,
    voice: Option<&str>,
) -> String {
    format!(
        "You are the AI writing partner in Plume, a local-first desktop app for content \
         creators who write in markdown and publish across platforms (blogs, \
         newsletters, LinkedIn, X). The user is editing the document below — help them \
         review, improve, or generate content. Be concise and direct. When you propose a \
         revised version of the whole document, give the complete markdown in a \
         ```markdown code block so it can be applied in one click.\n\n\
         Current document content:\n---\n{document_content}\n---{references}{web}{voice}",
        references = references_section(references),
        web = web_search_section(web_search),
        voice = voice_section(voice)
    )
}

/// System prompt for an inline edit: the model is editing a selected fragment
/// of the document and must return ONLY the replacement text — no preamble, no
/// code fences — so the result can be spliced straight back into the editor.
fn inline_system_prompt(document_content: &str, selected_text: &str, voice: Option<&str>) -> String {
    format!(
        "You are performing an inline edit inside Plume, a desktop writing app. Apply the \
         user's instruction to the selected text and return ONLY the replacement text — \
         no preamble, explanation, quotes, or code fences — ready to paste in place. \
         Match the surrounding markdown style.\n\n\
         Full document (for context):\n---\n{document_content}\n---\n\n\
         Selected text to edit:\n---\n{selected_text}\n---{voice}",
        voice = voice_section(voice)
    )
}

/// System prompt for expanding a captured idea fragment into a full draft of
/// the given target document type. Returns ONLY the markdown body of the draft
/// (no preamble, no code fences) so it can be written straight into a new doc.
fn expand_system_prompt(idea: &str, target_label: &str, voice: Option<&str>) -> String {
    format!(
        "You are the writing partner in Plume, a desktop app for content creators. The \
         user captured a rough idea and wants it expanded into a structured first draft \
         for a {target_label}. Develop the idea into a complete, well-organized markdown \
         draft: a clear angle, logical sections, and concrete substance — not filler. \
         Match the conventions of a {target_label}. Return ONLY the markdown body of the \
         draft: no preamble, no explanation, no surrounding code fences.\n\n\
         The captured idea:\n---\n{idea}\n---{voice}",
        voice = voice_section(voice)
    )
}

// Platform-native adaptation guidance for content multiplication, selected per
// target. Each string bakes in the platform's hard rules (and mirrors the
// export-side constraints — X's ~280 char posts, LinkedIn's no-markdown / "see
// more" fold) so the generated draft already conforms to what its exporter
// expects on paste.
const GUIDANCE_BLOG_POST: &str = "Adapt it into a long-form blog post: a strong \
    title, an opening hook, logical H2/H3 sections, and a closing takeaway. Full \
    markdown is supported — use it.";
const GUIDANCE_NEWSLETTER: &str = "Adapt it into an email newsletter issue: a \
    subject-line-worthy opening, a warm, direct-address voice, short scannable \
    sections, and one clear call to action.";
const GUIDANCE_LINKEDIN_POST: &str = "Adapt it into a single LinkedIn post. Use \
    NO markdown syntax (no #, *, _, backticks) — LinkedIn renders none of it. \
    Front-load the most compelling idea into the first one or two lines \
    (everything after is hidden behind 'see more'). Short paragraphs, line breaks \
    between thoughts, plain text only.";
const GUIDANCE_X_THREAD: &str = "Adapt it into an X thread. Use NO markdown. Each \
    post must stand alone and stay under ~270 characters. Open with a hook post, \
    develop one idea per post, and do not number the posts (they are numbered on \
    export). Plain text only.";

/// Per-target adaptation guidance keyed on `DocType`. Targets without specific
/// guidance fall back to generic adaptation.
fn target_guidance(target: DocType) -> &'static str {
    match target {
        DocType::BlogPost => GUIDANCE_BLOG_POST,
        DocType::Newsletter => GUIDANCE_NEWSLETTER,
        DocType::LinkedinPost => GUIDANCE_LINKEDIN_POST,
        DocType::XThread => GUIDANCE_X_THREAD,
        _ => "",
    }
}

/// System prompt for content multiplication: re-shape a FINISHED source document
/// into a platform-native `target_label` (same ideas, the platform's format and
/// audience — not a copy). Returns ONLY the markdown body so it can be written
/// straight into a new doc. Platform rules come before the source; the style-only
/// voice block comes last so it can't override the format contract.
fn multiply_system_prompt(
    source: &str,
    target: DocType,
    target_label: &str,
    voice: Option<&str>,
) -> String {
    format!(
        "You are the writing partner in Plume, a desktop app for content creators \
         who write once and publish across platforms. The user has a FINISHED source \
         document and wants a platform-native {target_label} derived from it: the same \
         ideas, re-shaped for the platform's format and audience — not a copy of the \
         original. {guidance} Return ONLY the markdown body of the {target_label}: no \
         preamble, no explanation, no surrounding code fences.\n\n\
         The source document:\n---\n{source}\n---{voice}",
        guidance = target_guidance(target),
        voice = voice_section(voice)
    )
}

/// Expand a captured idea into a draft of `target_label`, streaming the draft
/// markdown over the same `assistant:*` events (filtered by stream id, so the
/// chat panel ignores it). Uses the provider's default (strong) model — this is
/// a generative job. Shares the single AiState slot (mutually exclusive with
/// chat/inline edit).
#[allow(clippy::too_many_arguments)]
pub fn start_expand_stream(
    app: AppHandle,
    state: &AiState,
    stream_id: String,
    provider: Provider,
    model: Option<String>,
    idea: String,
    target_label: String,
    voice: Option<String>,
) -> Result<()> {
    let model = model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| provider.default_model().to_string());
    let system = expand_system_prompt(&idea, &target_label, voice.as_deref());
    let messages = vec![ChatMessage {
        role: "user".into(),
        content: format!("Expand my idea into a {target_label} draft."),
        raw_content: None,
    }];
    // one-shot generation — no caching, no compaction (no history)
    run_stream(app, state, stream_id, provider, model, system, messages, false, false, false)
}

/// Adapt a finished document into a platform-native draft of `target`, streaming
/// the draft markdown over the same `assistant:*` events (filtered by stream id).
/// Uses the provider's default (strong) model — a generative job. Shares the
/// single AiState slot, so the frontend must run targets sequentially.
#[allow(clippy::too_many_arguments)]
pub fn start_content_multiply_stream(
    app: AppHandle,
    state: &AiState,
    stream_id: String,
    provider: Provider,
    model: Option<String>,
    source_content: String,
    target: DocType,
    target_label: String,
    voice: Option<String>,
) -> Result<()> {
    let model = model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| provider.default_model().to_string());
    let system = multiply_system_prompt(&source_content, target, &target_label, voice.as_deref());
    let messages = vec![ChatMessage {
        role: "user".into(),
        content: format!("Adapt my document into a {target_label}."),
        raw_content: None,
    }];
    // one-shot generation — no caching, no compaction (no history)
    run_stream(app, state, stream_id, provider, model, system, messages, false, false, false)
}

#[allow(clippy::too_many_arguments)]
pub fn start_stream(
    app: AppHandle,
    state: &AiState,
    stream_id: String,
    provider: Provider,
    model: Option<String>,
    messages: Vec<ChatMessage>,
    document_content: String,
    references: Vec<DocReference>,
    web_search: bool,
    voice: Option<String>,
) -> Result<()> {
    let model = model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| provider.default_model().to_string());
    let system = system_prompt(&document_content, &references, web_search, voice.as_deref());
    // chat: the system prefix (instructions + document) is re-sent every turn —
    // cache it so unchanged-document follow-ups read back at ~0.1× input price.
    // Enable server-side compaction so a long master chat stays bounded without
    // hard-dropping context (Anthropic + supported model only).
    let compact = provider == Provider::Anthropic && supports_compaction(&model);
    run_stream(app, state, stream_id, provider, model, system, messages, true, compact, web_search)
}

/// Start an inline edit: apply `instruction` to `selected_text`, streaming the
/// replacement back over the same `assistant:*` events (the frontend filters by
/// stream id, so the chat panel ignores it). Defaults to the provider's faster
/// model. Shares the single AiState slot — starting this aborts any chat stream
/// and vice versa.
#[allow(clippy::too_many_arguments)]
pub fn start_inline_stream(
    app: AppHandle,
    state: &AiState,
    stream_id: String,
    provider: Provider,
    model: Option<String>,
    instruction: String,
    selected_text: String,
    document_content: String,
    voice: Option<String>,
) -> Result<()> {
    let model = model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| provider.fast_model().to_string());
    let system = inline_system_prompt(&document_content, &selected_text, voice.as_deref());
    let messages =
        vec![ChatMessage { role: "user".into(), content: instruction, raw_content: None }];
    // one-shot edit — don't pay the cache-write premium on a single-use prompt
    run_stream(app, state, stream_id, provider, model, system, messages, false, false, false)
}

/// Shared driver: load the key, abort any in-flight stream, then spawn the
/// provider request as the new in-flight task. `system` is the fully-built
/// system prompt (chat or inline); `model` is already resolved. `cache_system`
/// requests prompt caching of the system block (chat only — see
/// `anthropic_request_body`); `compact` enables server-side context compaction
/// (Anthropic chat only).
#[allow(clippy::too_many_arguments)]
fn run_stream(
    app: AppHandle,
    state: &AiState,
    stream_id: String,
    provider: Provider,
    model: String,
    system: String,
    messages: Vec<ChatMessage>,
    cache_system: bool,
    compact: bool,
    web_search: bool,
) -> Result<()> {
    let Some(api_key) = get_api_key(&app, provider)? else {
        return Err(Error::InvalidInput("no API key configured".into()));
    };
    // When web search is on, the Tavily key is required up front — fail fast with
    // a clear message rather than letting the model call a tool we can't run.
    let tavily_key = if web_search {
        Some(get_tavily_key(&app)?.ok_or_else(|| {
            Error::InvalidInput("no Tavily API key configured — add one in Settings".into())
        })?)
    } else {
        None
    };

    let mut current = state.current.lock().expect("ai mutex poisoned");
    if let Some((old_id, handle)) = current.take() {
        handle.abort();
        // `aborted` tells listeners this is NOT a successful completion — the
        // text they have is truncated and must not be treated as a result.
        let _ = app.emit(EVT_DONE, json!({ "id": old_id, "aborted": true }));
    }
    let task_app = app.clone();
    let task_id = stream_id.clone();
    let task = tauri::async_runtime::spawn(async move {
        let result = match provider {
            Provider::Anthropic => {
                stream_anthropic(
                    &task_app,
                    &task_id,
                    &api_key,
                    &model,
                    messages,
                    &system,
                    cache_system,
                    compact,
                    tavily_key,
                )
                .await
            }
            Provider::Openrouter => {
                stream_openrouter(
                    &task_app,
                    &task_id,
                    &api_key,
                    &model,
                    messages,
                    &system,
                    tavily_key,
                )
                .await
            }
        };
        if let Err(e) = result {
            let _ = task_app.emit(EVT_ERROR, json!({ "id": task_id, "message": e.to_string() }));
        }
        let _ = task_app.emit(EVT_DONE, json!({ "id": task_id }));
    });
    *current = Some((stream_id, task));
    Ok(())
}

pub fn stop_stream(app: &AppHandle, state: &AiState) {
    if let Some((id, handle)) = state.current.lock().expect("ai mutex poisoned").take() {
        handle.abort();
        let _ = app.emit(EVT_DONE, json!({ "id": id, "aborted": true }));
    }
}

/// Drain complete lines (terminated by '\n') from a byte buffer, passing each
/// `data: ` payload to `on_data`. The buffer holds raw bytes — decoding only
/// complete lines means a multi-byte UTF-8 character split across two network
/// chunks is decoded whole instead of becoming U+FFFD replacement characters
/// (which v2 would persist into documents via expand/multiply).
fn drain_sse_lines(
    buffer: &mut Vec<u8>,
    on_data: &mut impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
        let line: Vec<u8> = buffer.drain(..=pos).collect();
        let line = String::from_utf8_lossy(&line);
        if let Some(data) = line.trim_end().strip_prefix("data: ") {
            on_data(data)?;
        }
    }
    Ok(())
}

/// Read SSE `data:` payloads from a response, calling `on_event` per payload.
async fn for_each_sse_data(
    response: reqwest::Response,
    mut on_event: impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    let mut stream = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Error::InvalidInput(format!("stream error: {e}")))?;
        buffer.extend_from_slice(&chunk);
        drain_sse_lines(&mut buffer, &mut on_event)?;
    }
    Ok(())
}

async fn error_for_status(response: reqwest::Response, provider: &str) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let detail = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(String::from))
        .unwrap_or(text);
    // Map the common, actionable cases to plain guidance; fall back to the raw
    // provider message (with status) for everything else.
    let message = match status.as_u16() {
        401 | 403 => format!("Your {provider} API key was rejected — check it in Settings."),
        429 => format!("{provider} rate limit reached — wait a moment and try again."),
        500 | 502 | 503 | 529 => {
            format!("{provider} is unavailable right now — try again shortly.")
        }
        _ => format!("{provider} API error {status}: {detail}"),
    };
    Err(Error::InvalidInput(message))
}

/// Token usage. Providers report counts incrementally and across web-search
/// rounds; `update` keeps the latest non-zero value for each field. Reporting
/// the final round's counts gives a sensible "context size after the turn"
/// (the input then reflects the full context, tool results included).
#[derive(Default, Clone, Copy)]
struct Usage {
    input: u64,
    output: u64,
}

impl Usage {
    fn update(&mut self, input: u64, output: u64) {
        if input > 0 {
            self.input = input;
        }
        if output > 0 {
            self.output = output;
        }
    }
}

/// Shared HTTP client. A connect timeout and an idle *read* timeout mean a hung
/// connection (socket open but no bytes arriving) fails instead of spinning a
/// spinner forever. The read timeout is per-read, not a total cap, so a long
/// generation still streams as long as tokens keep arriving.
///
/// A process-wide `reqwest::Client` with sane timeouts. Reused across every AI
/// stream round and the Tavily web-search path so connection pools/TLS sessions
/// are amortized instead of rebuilt per request.
pub(crate) fn http_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .read_timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("reqwest client builds with valid timeouts")
    })
}

/// Send a request and run `on_event` for each decoded SSE `data:` payload (JSON
/// already parsed; `[DONE]` and unparseable lines skipped). The low-level loop
/// shared by both providers' per-round streaming.
async fn stream_events(
    provider_label: &str,
    request: reqwest::RequestBuilder,
    mut on_event: impl FnMut(&serde_json::Value) -> Result<()>,
) -> Result<()> {
    let response = request
        .send()
        .await
        .map_err(|e| Error::InvalidInput(format!("request failed: {e}")))?;
    let response = error_for_status(response, provider_label).await?;
    for_each_sse_data(response, |data| {
        if data == "[DONE]" {
            return Ok(());
        }
        match serde_json::from_str::<serde_json::Value>(data) {
            Ok(event) => on_event(&event),
            Err(_) => Ok(()),
        }
    })
    .await
}

/// Run a Tavily search for one tool call and return the text block to feed back
/// as the tool result. Search failures become a result string (not a hard
/// error) so the model can recover or tell the user, rather than killing the
/// whole turn.
async fn run_web_search(tavily_key: &str, query: &str) -> String {
    match websearch::search(
        tavily_key,
        query,
        websearch::DEFAULT_MAX_RESULTS,
        websearch::DEFAULT_DEPTH,
    )
    .await
    {
        Ok(results) => websearch::format_results(query, &results),
        Err(e) => format!("Web search for \"{query}\" failed: {e}"),
    }
}

/// Whether an Anthropic model accepts `"thinking": {"type": "adaptive"}`.
/// Adaptive thinking exists on Opus 4.6+, Sonnet 4.6, and the Fable/Mythos 5
/// family; Haiku 4.5 (our `fast_model`) and older models 400 on it, so the
/// thinking field is omitted entirely for them (omitting is valid everywhere).
/// Verified 2026-06-11 against the claude-api skill.
fn supports_adaptive_thinking(model: &str) -> bool {
    ["claude-fable-5", "claude-mythos-5", "claude-opus-4-6", "claude-opus-4-7", "claude-opus-4-8", "claude-sonnet-4-6"]
        .iter()
        .any(|m| model.starts_with(m))
}

/// Whether a model supports server-side context compaction (beta
/// `compact-2026-01-12`). Kept separate from `supports_adaptive_thinking` even
/// though the lists currently coincide — they are distinct capabilities.
/// Verified 2026-06-11 against the claude-api skill.
fn supports_compaction(model: &str) -> bool {
    ["claude-fable-5", "claude-mythos-5", "claude-opus-4-6", "claude-opus-4-7", "claude-opus-4-8", "claude-sonnet-4-6"]
        .iter()
        .any(|m| model.starts_with(m))
}

/// Render one chat message to the Anthropic wire shape. A turn that carries a
/// compaction summary replays its stored content-block array verbatim;
/// everything else is a plain `{role, content}` string.
fn anthropic_message_json(m: &ChatMessage) -> serde_json::Value {
    match &m.raw_content {
        Some(blocks) => json!({ "role": m.role, "content": blocks.clone() }),
        None => json!({ "role": m.role, "content": m.content }),
    }
}

/// Mark the most-recent compaction block (newest→oldest) for caching so the
/// summary reads back cheaply. ONLY the last one: it's the anchor the API keeps
/// (everything before it is dropped server-side), and caching every compaction
/// block in a long thread would blow past the 4 cache_control breakpoints/request
/// limit (system already uses one).
fn cache_last_compaction(messages: &mut [serde_json::Value]) {
    for msg in messages.iter_mut().rev() {
        if let Some(arr) = msg["content"].as_array_mut() {
            if let Some(block) = arr
                .iter_mut()
                .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("compaction"))
            {
                block["cache_control"] = json!({ "type": "ephemeral" });
                return;
            }
        }
    }
}

/// Build the Anthropic request body. `cache_system` wraps the system prompt in
/// a cached content block (`cache_control: ephemeral`) — worth it for the chat
/// path, where the same system prefix (instructions + document) is re-sent every
/// turn and reads back at ~0.1× when the document is unchanged. One-shot paths
/// (inline edit / expand / multiply) pass `false`: a single-use prompt would
/// only pay the ~1.25× write premium with nothing to read it back. `compact`
/// turns on server-side context compaction (chat only). `web_search` adds the
/// `web_search` tool and, because tool turns are replayed without the signed
/// thinking blocks that adaptive thinking would require, omits the `thinking`
/// field for the turn (a deliberate v1 simplification — search turns don't use
/// extended thinking). `messages` already includes any loop-local tool_use /
/// tool_result turns (carried as `raw_content`).
fn anthropic_request_body(
    model: &str,
    messages: &[ChatMessage],
    system: &str,
    cache_system: bool,
    compact: bool,
    web_search: bool,
) -> serde_json::Value {
    let system_field = if cache_system {
        json!([{ "type": "text", "text": system, "cache_control": { "type": "ephemeral" } }])
    } else {
        json!(system)
    };
    let mut messages_json: Vec<serde_json::Value> =
        messages.iter().map(anthropic_message_json).collect();
    cache_last_compaction(&mut messages_json);
    let mut body = json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "system": system_field,
        "messages": messages_json,
        "stream": true,
    });
    // adaptive thinking and web search are mutually exclusive per turn — see doc
    if web_search {
        body["tools"] = json!([web_search_tool_anthropic()]);
    } else if supports_adaptive_thinking(model) {
        body["thinking"] = json!({"type": "adaptive"});
    }
    if compact {
        // default trigger (150K input tokens); the API summarizes older history
        // into a compaction block and continues from the summary
        body["context_management"] = json!({ "edits": [{ "type": "compact_20260112" }] });
    }
    body
}

/// One tool call captured from a streamed Anthropic turn. `input_json` is the
/// `input_json_delta` fragments concatenated; parsed to JSON once the stream ends.
struct ToolUse {
    id: String,
    name: String,
    input_json: String,
}

/// What one streamed Anthropic request produced. Drives the tool loop: if
/// `stop_reason == "tool_use"` with `tool_uses`, run them and request again.
#[derive(Default)]
struct AnthropicRound {
    text: String,
    tool_uses: Vec<ToolUse>,
    compaction: Option<String>,
    stop_reason: Option<String>,
    usage: Usage,
}

/// Anthropic chat with an optional `web_search` tool loop. With `tavily_key`
/// set, the model may call `web_search`; we run Tavily and feed the results
/// back, looping until it answers (or `MAX_SEARCH_ROUNDS` searches, after which
/// the next request offers no tool so the model must respond). Without it, this
/// is a single request — identical to the pre-feature behavior. Text streams as
/// `EVT_TOKEN` throughout; only the final visible text (and an optional
/// compaction block) is persisted — tool turns are loop-local.
#[allow(clippy::too_many_arguments)]
async fn stream_anthropic(
    app: &AppHandle,
    stream_id: &str,
    api_key: &str,
    model: &str,
    messages: Vec<ChatMessage>,
    system: &str,
    cache_system: bool,
    compact: bool,
    tavily_key: Option<String>,
) -> Result<()> {
    let web_search = tavily_key.is_some();
    let mut convo = messages; // loop appends loop-local tool_use / tool_result turns
    let mut usage = Usage::default();
    let mut final_text = String::new();
    let mut compaction: Option<String> = None;
    let mut searches = 0;

    loop {
        let allow_tools = web_search && searches < MAX_SEARCH_ROUNDS;
        let body =
            anthropic_request_body(model, &convo, system, cache_system, compact, allow_tools);
        let mut request = http_client()
            .post(ANTHROPIC_URL)
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json");
        if compact {
            request = request.header("anthropic-beta", COMPACT_BETA);
        }
        let round = stream_anthropic_round(app, stream_id, request.json(&body)).await?;
        usage.update(round.usage.input, round.usage.output);
        final_text.push_str(&round.text);
        if round.compaction.is_some() {
            compaction = round.compaction;
        }

        let searching = round.stop_reason.as_deref() == Some("tool_use")
            && !round.tool_uses.is_empty();
        let Some(key) = tavily_key.as_deref().filter(|_| allow_tools && searching) else {
            break;
        };

        // Replay the assistant turn (text + tool_use blocks) verbatim so the
        // tool_use ids line up with the results we send back.
        let mut assistant_blocks: Vec<serde_json::Value> = Vec::new();
        if !round.text.is_empty() {
            assistant_blocks.push(json!({ "type": "text", "text": round.text }));
        }
        let mut result_blocks: Vec<serde_json::Value> = Vec::new();
        for tu in &round.tool_uses {
            let input: serde_json::Value =
                serde_json::from_str(&tu.input_json).unwrap_or_else(|_| json!({}));
            let query = input["query"].as_str().unwrap_or_default().to_string();
            assistant_blocks.push(json!({
                "type": "tool_use", "id": tu.id, "name": tu.name, "input": input,
            }));
            let _ = app.emit(
                EVT_STATUS,
                json!({ "id": stream_id, "message": format!("Searching the web: {query}") }),
            );
            let result = run_web_search(key, &query).await;
            result_blocks.push(json!({
                "type": "tool_result", "tool_use_id": tu.id, "content": result,
            }));
        }
        convo.push(ChatMessage {
            role: "assistant".into(),
            content: round.text,
            raw_content: Some(json!(assistant_blocks)),
        });
        convo.push(ChatMessage {
            role: "user".into(),
            content: String::new(),
            raw_content: Some(json!(result_blocks)),
        });
        searches += 1;
    }

    if usage.input > 0 || usage.output > 0 {
        let _ = app.emit(
            EVT_USAGE,
            json!({ "id": stream_id, "inputTokens": usage.input, "outputTokens": usage.output }),
        );
    }
    // Only on success: a compaction block round-trips the full content array.
    if let Some(summary) = compaction {
        let content = json!([
            { "type": "compaction", "content": summary },
            { "type": "text", "text": final_text },
        ]);
        let _ = app.emit(EVT_CONTENT, json!({ "id": stream_id, "content": content }));
    }
    Ok(())
}

/// Stream one Anthropic request, accumulating text (emitted live as `EVT_TOKEN`),
/// any `tool_use` blocks (id/name/input_json by block index), a compaction
/// summary, the stop reason, and usage.
async fn stream_anthropic_round(
    app: &AppHandle,
    stream_id: &str,
    request: reqwest::RequestBuilder,
) -> Result<AnthropicRound> {
    use std::collections::HashMap;
    let mut round = AnthropicRound::default();
    let mut tools: HashMap<u64, ToolUse> = HashMap::new();

    stream_events("Anthropic", request, |event| {
        match event["type"].as_str() {
            Some("content_block_start") if event["content_block"]["type"] == "tool_use" => {
                let block = &event["content_block"];
                tools.insert(
                    event["index"].as_u64().unwrap_or(0),
                    ToolUse {
                        id: block["id"].as_str().unwrap_or_default().to_string(),
                        name: block["name"].as_str().unwrap_or_default().to_string(),
                        input_json: String::new(),
                    },
                );
            }
            Some("content_block_delta") => match event["delta"]["type"].as_str() {
                Some("text_delta") => {
                    let text = event["delta"]["text"].as_str().unwrap_or_default();
                    if !text.is_empty() {
                        round.text.push_str(text);
                        let _ = app.emit(EVT_TOKEN, json!({ "id": stream_id, "text": text }));
                    }
                }
                Some("input_json_delta") => {
                    let idx = event["index"].as_u64().unwrap_or(0);
                    if let (Some(tu), Some(frag)) =
                        (tools.get_mut(&idx), event["delta"]["partial_json"].as_str())
                    {
                        tu.input_json.push_str(frag);
                    }
                }
                // the compaction summary arrives as one complete delta; capture
                // it but never surface it as visible reply text
                Some("compaction_delta") => {
                    if let Some(summary) = event["delta"]["content"].as_str() {
                        round.compaction = Some(summary.to_string());
                    }
                }
                _ => {}
            },
            // input_tokens in message_start; output accumulates in message_delta,
            // which also carries the stop_reason
            Some("message_start") => {
                let (i, o) = anthropic_usage(&event["message"]["usage"]);
                round.usage.update(i, o);
            }
            Some("message_delta") => {
                let (i, o) = anthropic_usage(&event["usage"]);
                round.usage.update(i, o);
                if let Some(sr) = event["delta"]["stop_reason"].as_str() {
                    round.stop_reason = Some(sr.to_string());
                }
            }
            Some("error") => {
                let msg = event["error"]["message"].as_str().unwrap_or("stream error");
                return Err(Error::InvalidInput(msg.to_string()));
            }
            _ => {}
        }
        Ok(())
    })
    .await?;

    let mut indexed: Vec<(u64, ToolUse)> = tools.into_iter().collect();
    indexed.sort_by_key(|(i, _)| *i);
    round.tool_uses = indexed.into_iter().map(|(_, tu)| tu).collect();
    Ok(round)
}

/// Pull `(input, output)` from an Anthropic `usage` object. The reported input
/// is the prompt total including any cached tokens.
fn anthropic_usage(u: &serde_json::Value) -> (u64, u64) {
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    (
        n("input_tokens") + n("cache_read_input_tokens") + n("cache_creation_input_tokens"),
        n("output_tokens"),
    )
}

/// One OpenAI-style tool call captured from an OpenRouter stream. `arguments`
/// is the streamed JSON-argument fragments concatenated.
struct OpenRouterToolCall {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
struct OpenRouterRound {
    text: String,
    tool_calls: Vec<OpenRouterToolCall>,
    usage: Usage,
}

/// OpenRouter chat with the same optional `web_search` tool loop, using the
/// OpenAI function-calling wire format (`tool_calls` deltas, `role:"tool"`
/// results). Without `tavily_key` it is a single request — unchanged behavior.
async fn stream_openrouter(
    app: &AppHandle,
    stream_id: &str,
    api_key: &str,
    model: &str,
    messages: Vec<ChatMessage>,
    system: &str,
    tavily_key: Option<String>,
) -> Result<()> {
    let web_search = tavily_key.is_some();
    let mut convo: Vec<serde_json::Value> = vec![json!({ "role": "system", "content": system })];
    convo.extend(messages.iter().map(|m| json!({ "role": m.role, "content": m.content })));

    let client = http_client();
    let mut usage = Usage::default();
    let mut searches = 0;

    loop {
        let allow_tools = web_search && searches < MAX_SEARCH_ROUNDS;
        let mut body = json!({
            "model": model,
            "messages": convo,
            "stream": true,
            // ask for a final usage chunk (OpenAI-compatible streaming option)
            "stream_options": { "include_usage": true },
        });
        if allow_tools {
            body["tools"] = json!([web_search_tool_openai()]);
            body["tool_choice"] = json!("auto");
        }
        let request = client
            .post(OPENROUTER_URL)
            .header("authorization", format!("Bearer {api_key}"))
            .header("content-type", "application/json")
            .header("http-referer", "https://github.com/WalrusQuant/plume")
            .header("x-title", "Plume")
            .json(&body);

        let round = stream_openrouter_round(app, stream_id, request).await?;
        usage.update(round.usage.input, round.usage.output);

        let Some(key) = tavily_key.as_deref().filter(|_| allow_tools && !round.tool_calls.is_empty())
        else {
            break;
        };

        // assistant turn carrying the tool_calls (content may be null)
        let tool_calls_json: Vec<serde_json::Value> = round
            .tool_calls
            .iter()
            .map(|tc| {
                json!({
                    "id": tc.id,
                    "type": "function",
                    "function": { "name": tc.name, "arguments": tc.arguments },
                })
            })
            .collect();
        convo.push(json!({
            "role": "assistant",
            "content": if round.text.is_empty() { serde_json::Value::Null } else { json!(round.text) },
            "tool_calls": tool_calls_json,
        }));
        for tc in &round.tool_calls {
            let args: serde_json::Value =
                serde_json::from_str(&tc.arguments).unwrap_or_else(|_| json!({}));
            let query = args["query"].as_str().unwrap_or_default().to_string();
            let _ = app.emit(
                EVT_STATUS,
                json!({ "id": stream_id, "message": format!("Searching the web: {query}") }),
            );
            let result = run_web_search(key, &query).await;
            convo.push(json!({ "role": "tool", "tool_call_id": tc.id, "content": result }));
        }
        searches += 1;
    }

    if usage.input > 0 || usage.output > 0 {
        let _ = app.emit(
            EVT_USAGE,
            json!({ "id": stream_id, "inputTokens": usage.input, "outputTokens": usage.output }),
        );
    }
    Ok(())
}

/// Stream one OpenRouter request, accumulating content (emitted live), any
/// streamed `tool_calls` (by index), and usage (final chunk).
async fn stream_openrouter_round(
    app: &AppHandle,
    stream_id: &str,
    request: reqwest::RequestBuilder,
) -> Result<OpenRouterRound> {
    use std::collections::HashMap;
    let mut text = String::new();
    let mut usage = Usage::default();
    let mut calls: HashMap<u64, OpenRouterToolCall> = HashMap::new();

    stream_events("OpenRouter", request, |event| {
        if let Some(msg) = event["error"]["message"].as_str() {
            return Err(Error::InvalidInput(msg.to_string()));
        }
        // the final chunk carries usage (choices empty)
        if event["usage"].is_object() {
            usage.update(
                event["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
                event["usage"]["completion_tokens"].as_u64().unwrap_or(0),
            );
        }
        let delta = &event["choices"][0]["delta"];
        if let Some(t) = delta["content"].as_str() {
            if !t.is_empty() {
                text.push_str(t);
                let _ = app.emit(EVT_TOKEN, json!({ "id": stream_id, "text": t }));
            }
        }
        if let Some(tcs) = delta["tool_calls"].as_array() {
            for tc in tcs {
                let entry = calls.entry(tc["index"].as_u64().unwrap_or(0)).or_insert_with(|| {
                    OpenRouterToolCall {
                        id: String::new(),
                        name: String::new(),
                        arguments: String::new(),
                    }
                });
                if let Some(id) = tc["id"].as_str().filter(|s| !s.is_empty()) {
                    entry.id = id.to_string();
                }
                if let Some(name) = tc["function"]["name"].as_str().filter(|s| !s.is_empty()) {
                    entry.name = name.to_string();
                }
                if let Some(args) = tc["function"]["arguments"].as_str() {
                    entry.arguments.push_str(args);
                }
            }
        }
        Ok(())
    })
    .await?;

    let mut indexed: Vec<(u64, OpenRouterToolCall)> = calls.into_iter().collect();
    indexed.sort_by_key(|(i, _)| *i);
    Ok(OpenRouterRound {
        text,
        tool_calls: indexed.into_iter().map(|(_, c)| c).collect(),
        usage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptive_thinking_gated_by_model() {
        // strong-tier models accept adaptive thinking…
        assert!(supports_adaptive_thinking("claude-opus-4-8"));
        assert!(supports_adaptive_thinking("claude-sonnet-4-6"));
        assert!(supports_adaptive_thinking("claude-fable-5"));
        // …but Haiku (the fast_model fallback) and older models 400 on it
        assert!(!supports_adaptive_thinking(Provider::Anthropic.fast_model()));
        assert!(!supports_adaptive_thinking("claude-haiku-4-5-20251001"));
        assert!(!supports_adaptive_thinking("claude-sonnet-4-5"));
    }

    #[test]
    fn anthropic_caches_system_only_when_requested() {
        let msgs =
            vec![ChatMessage { role: "user".into(), content: "hi".into(), raw_content: None }];

        // chat path: the system prompt is wrapped in a cached content block
        let cached = anthropic_request_body("claude-opus-4-8", &msgs, "SYS", true, false, false);
        assert!(cached["system"].is_array(), "cached system must be a content-block array");
        assert_eq!(cached["system"][0]["text"], "SYS");
        assert_eq!(cached["system"][0]["cache_control"]["type"], "ephemeral");

        // one-shot paths: plain string system, no cache-write premium
        let plain = anthropic_request_body("claude-opus-4-8", &msgs, "SYS", false, false, false);
        assert!(plain["system"].is_string());
        assert_eq!(plain["system"], "SYS");
        assert!(plain["system"][0]["cache_control"].is_null());

        // caching is orthogonal to the existing model-gated thinking field
        assert_eq!(cached["thinking"]["type"], "adaptive");
        let haiku = anthropic_request_body("claude-haiku-4-5", &msgs, "SYS", false, false, false);
        assert!(haiku.get("thinking").is_none());
    }

    #[test]
    fn supports_compaction_gates_by_model() {
        assert!(supports_compaction("claude-opus-4-8"));
        assert!(supports_compaction("claude-sonnet-4-6"));
        assert!(supports_compaction("claude-fable-5"));
        assert!(!supports_compaction("claude-haiku-4-5"));
        assert!(!supports_compaction("claude-sonnet-4-5"));
    }

    #[test]
    fn anthropic_compaction_param_and_block_replay() {
        // context_management only present when compaction is requested
        let plain_msgs =
            vec![ChatMessage { role: "user".into(), content: "hi".into(), raw_content: None }];
        let off = anthropic_request_body("claude-opus-4-8", &plain_msgs, "SYS", true, false, false);
        assert!(off.get("context_management").is_none());
        let on = anthropic_request_body("claude-opus-4-8", &plain_msgs, "SYS", true, true, false);
        assert_eq!(on["context_management"]["edits"][0]["type"], "compact_20260112");
        // a plain message stays a string
        assert_eq!(on["messages"][0]["content"], "hi");

        // a turn with raw_content replays the block array, and the compaction
        // block gets a cache_control breakpoint injected
        let blocks = json!([
            { "type": "compaction", "content": "summary" },
            { "type": "text", "text": "reply" },
        ]);
        let with_blocks = vec![ChatMessage {
            role: "assistant".into(),
            content: "reply".into(),
            raw_content: Some(blocks),
        }];
        let body = anthropic_request_body("claude-opus-4-8", &with_blocks, "SYS", true, true, false);
        let content = &body["messages"][0]["content"];
        assert!(content.is_array());
        assert_eq!(content[0]["type"], "compaction");
        assert_eq!(content[0]["content"], "summary");
        assert_eq!(content[0]["cache_control"]["type"], "ephemeral");
        assert_eq!(content[1]["text"], "reply");
    }

    #[test]
    fn only_the_last_compaction_block_is_cached() {
        // a long thread with two compaction turns must not stack cache_control
        // breakpoints (system + N compactions would blow past the 4/request cap)
        let turn = |summary: &str| ChatMessage {
            role: "assistant".into(),
            content: "r".into(),
            raw_content: Some(json!([
                { "type": "compaction", "content": summary },
                { "type": "text", "text": "r" },
            ])),
        };
        let msgs = vec![
            turn("older"),
            ChatMessage { role: "user".into(), content: "more".into(), raw_content: None },
            turn("newer"),
        ];
        let body = anthropic_request_body("claude-opus-4-8", &msgs, "SYS", true, true, false);
        // older compaction: NOT cached
        assert!(body["messages"][0]["content"][0]["cache_control"].is_null());
        // newest compaction: cached
        assert_eq!(body["messages"][2]["content"][0]["cache_control"]["type"], "ephemeral");
        // exactly one compaction breakpoint + the system breakpoint = 2 (≤ 4)
        let body_str = serde_json::to_string(&body).unwrap();
        assert_eq!(body_str.matches("cache_control").count(), 2);
    }

    #[test]
    fn sse_multibyte_char_split_across_chunks_decodes_whole() {
        // "é" is 0xC3 0xA9 — split it across two network chunks
        let payload = "data: é\n".as_bytes();
        let mut buffer: Vec<u8> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        buffer.extend_from_slice(&payload[..7]); // ends mid-character
        drain_sse_lines(&mut buffer, &mut |d| {
            seen.push(d.to_string());
            Ok(())
        })
        .unwrap();
        assert!(seen.is_empty(), "incomplete line must stay buffered");
        buffer.extend_from_slice(&payload[7..]);
        drain_sse_lines(&mut buffer, &mut |d| {
            seen.push(d.to_string());
            Ok(())
        })
        .unwrap();
        assert_eq!(seen, vec!["é"]);
    }

    #[test]
    fn sse_multiple_lines_in_one_chunk() {
        let mut buffer = b"data: one\n\ndata: two\nleftover".to_vec();
        let mut seen: Vec<String> = Vec::new();
        let mut on_data = |d: &str| {
            seen.push(d.to_string());
            Ok(())
        };
        drain_sse_lines(&mut buffer, &mut on_data).unwrap();
        assert_eq!(seen, vec!["one", "two"]);
        assert_eq!(buffer, b"leftover");
    }

    #[test]
    fn fast_model_is_haiku_tier() {
        assert_eq!(Provider::Anthropic.fast_model(), "claude-haiku-4-5");
        assert_eq!(Provider::Openrouter.fast_model(), "anthropic/claude-haiku-4.5");
    }

    #[test]
    fn fast_model_differs_from_default() {
        for provider in [Provider::Anthropic, Provider::Openrouter] {
            assert_ne!(provider.fast_model(), provider.default_model());
        }
    }

    #[test]
    fn inline_prompt_includes_selection_and_replace_only_rule() {
        let prompt = inline_system_prompt("# Doc\n\nbody text", "body text", None);
        assert!(prompt.contains("body text"));
        assert!(prompt.contains("# Doc"));
        // the replace-only instruction is what keeps the output spliceable
        assert!(prompt.contains("ONLY the replacement text"));
        assert!(prompt.contains("code fences"));
    }

    #[test]
    fn expand_prompt_includes_idea_and_target() {
        let prompt = expand_system_prompt("a post about local-first apps", "Newsletter", None);
        assert!(prompt.contains("a post about local-first apps"));
        assert!(prompt.contains("Newsletter"));
        // body-only so it can be written straight into a new doc
        assert!(prompt.contains("ONLY the markdown body"));
    }

    #[test]
    fn multiply_prompt_includes_source_and_target_guidance() {
        let source = "# Local-first apps\n\nWhy own your data.";
        // LinkedIn: no-markdown rule is the platform marker
        let li = multiply_system_prompt(source, DocType::LinkedinPost, "LinkedIn post", None);
        assert!(li.contains("Why own your data"));
        assert!(li.contains("LinkedIn post"));
        assert!(li.contains("ONLY the markdown body"));
        assert!(li.contains("NO markdown"));
        // X thread: the ~270 char rule is the platform marker
        let x = multiply_system_prompt(source, DocType::XThread, "X thread", None);
        assert!(x.contains("270"));
        // a target without specific guidance still produces a valid prompt
        let generic = multiply_system_prompt(source, DocType::Generic, "document", None);
        assert!(generic.contains("ONLY the markdown body"));
    }

    #[test]
    fn multiply_prompt_voice_after_guidance() {
        // voice is style-only and must come after the platform format rules
        let prompt = multiply_system_prompt(
            "src",
            DocType::LinkedinPost,
            "LinkedIn post",
            Some("flowery and verbose"),
        );
        let guidance = prompt.find("NO markdown").unwrap();
        let voice = prompt.find("Voice & tone").unwrap();
        assert!(guidance < voice);
    }

    #[test]
    fn voice_injected_into_all_surfaces_when_set() {
        let v = Some("terse, lowercase, dry wit");
        assert!(system_prompt("doc", &[], false, v).contains("terse, lowercase, dry wit"));
        assert!(inline_system_prompt("doc", "sel", v).contains("terse, lowercase, dry wit"));
        assert!(expand_system_prompt("idea", "Blog Post", v).contains("terse, lowercase, dry wit"));
        assert!(
            multiply_system_prompt("doc", DocType::BlogPost, "Blog Post", v)
                .contains("terse, lowercase, dry wit")
        );
    }

    #[test]
    fn voice_absent_when_unset_or_blank() {
        // None and whitespace both yield no voice section
        assert!(!system_prompt("doc", &[], false, None).contains("Voice & tone"));
        assert!(!system_prompt("doc", &[], false, Some("   ")).contains("Voice & tone"));
    }

    #[test]
    fn mechanical_rule_precedes_voice_in_inline() {
        // with a voice set, the replace-only contract must still hold and come
        // before the voice block (voice is style-only, never overrides format)
        let prompt = inline_system_prompt("doc", "sel", Some("flowery and verbose"));
        let rule = prompt.find("ONLY the replacement text").unwrap();
        let voice = prompt.find("Voice & tone").unwrap();
        assert!(rule < voice);
    }

    #[test]
    fn references_absent_when_empty_unchanged_prompt() {
        // no @-mentions → byte-identical to the pre-feature prompt
        assert!(!system_prompt("doc", &[], false, None).contains("Referenced"));
    }

    #[test]
    fn references_injected_after_document_before_voice() {
        let refs = vec![DocReference {
            name: "Pricing notes".into(),
            content: "we charge $9/mo".into(),
        }];
        let prompt = system_prompt("the live doc", &refs, false, Some("terse and dry"));
        // reference content is present and labeled
        assert!(prompt.contains("Referenced: \"Pricing notes\""));
        assert!(prompt.contains("we charge $9/mo"));
        // ordering: editable doc → references → voice (style stays last)
        let doc = prompt.find("the live doc").unwrap();
        let refs_at = prompt.find("Referenced:").unwrap();
        let voice = prompt.find("Voice & tone").unwrap();
        assert!(doc < refs_at && refs_at < voice);
    }

    #[test]
    fn web_search_section_only_when_enabled_and_asks_for_citations() {
        // disabled → byte-identical prompt (no section)
        assert!(web_search_section(false).is_empty());
        assert!(!system_prompt("doc", &[], false, None).contains("web_search"));
        // enabled → mentions the tool and the inline-link citation rule
        let section = web_search_section(true);
        assert!(section.contains("web_search"));
        assert!(section.to_lowercase().contains("markdown link"));
        // injected into the chat prompt after the document, before voice
        let prompt = system_prompt("the doc body", &[], true, Some("dry wit"));
        let doc = prompt.find("the doc body").unwrap();
        let web = prompt.find("web_search").unwrap();
        let voice = prompt.find("Voice & tone").unwrap();
        assert!(doc < web && web < voice);
    }

    #[test]
    fn anthropic_web_search_tool_replaces_thinking_when_enabled() {
        let msgs =
            vec![ChatMessage { role: "user".into(), content: "hi".into(), raw_content: None }];

        // web search on: the tool is present and thinking is omitted (even on a
        // model that would otherwise get adaptive thinking)
        let on = anthropic_request_body("claude-opus-4-8", &msgs, "SYS", true, false, true);
        assert_eq!(on["tools"][0]["name"], WEB_SEARCH_TOOL_NAME);
        assert_eq!(on["tools"][0]["input_schema"]["properties"]["query"]["type"], "string");
        assert!(on.get("thinking").is_none(), "thinking must be omitted on search turns");

        // web search off: no tool, thinking restored
        let off = anthropic_request_body("claude-opus-4-8", &msgs, "SYS", true, false, false);
        assert!(off.get("tools").is_none());
        assert_eq!(off["thinking"]["type"], "adaptive");
    }

    #[test]
    fn openai_tool_definition_shape() {
        let tool = web_search_tool_openai();
        assert_eq!(tool["type"], "function");
        assert_eq!(tool["function"]["name"], WEB_SEARCH_TOOL_NAME);
        // OpenAI nests the schema under `parameters` (Anthropic uses input_schema)
        assert_eq!(tool["function"]["parameters"]["required"][0], "query");
    }
}
