use std::collections::HashMap;
use std::sync::Mutex;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::{Error, Result};
use crate::storage::DocType;

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

/// Beta header enabling server-side context compaction.
const COMPACT_BETA: &str = "compact-2026-01-12";

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

fn keyring_entry(provider: Provider) -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, provider.key_name())
        .map_err(|e| Error::InvalidInput(format!("keychain unavailable: {e}")))
}

pub fn set_api_key(app: &AppHandle, provider: Provider, key: &str) -> Result<()> {
    let key = key.trim();
    if key.is_empty() {
        return Err(Error::InvalidInput("API key must not be empty".into()));
    }
    if cfg!(debug_assertions) {
        let mut keys = dev_keys_read(app)?;
        keys.insert(provider.key_name().to_string(), key.to_string());
        dev_keys_write(app, &keys)
    } else {
        keyring_entry(provider)?
            .set_password(key)
            .map_err(|e| Error::InvalidInput(format!("failed to store API key: {e}")))
    }
}

pub fn get_api_key(app: &AppHandle, provider: Provider) -> Result<Option<String>> {
    if cfg!(debug_assertions) {
        Ok(dev_keys_read(app)?.get(provider.key_name()).cloned())
    } else {
        match keyring_entry(provider)?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(Error::InvalidInput(format!("failed to read API key: {e}"))),
        }
    }
}

pub fn delete_api_key(app: &AppHandle, provider: Provider) -> Result<()> {
    if cfg!(debug_assertions) {
        let mut keys = dev_keys_read(app)?;
        keys.remove(provider.key_name());
        dev_keys_write(app, &keys)
    } else {
        match keyring_entry(provider)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(Error::InvalidInput(format!("failed to delete API key: {e}"))),
        }
    }
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

/// Chat system prompt. The model is a writing partner with the live document as
/// context; revisions of the whole doc come back in a ```markdown block so the
/// UI can apply them in one click. `references` are @-mentioned docs added as
/// background; `voice` stays last so style never overrides the format rules.
fn system_prompt(document_content: &str, references: &[DocReference], voice: Option<&str>) -> String {
    format!(
        "You are the AI writing partner in Plume, a local-first desktop app for content \
         creators who write in markdown and publish across platforms (blogs, \
         newsletters, LinkedIn, X). The user is editing the document below — help them \
         review, improve, or generate content. Be concise and direct. When you propose a \
         revised version of the whole document, give the complete markdown in a \
         ```markdown code block so it can be applied in one click.\n\n\
         Current document content:\n---\n{document_content}\n---{references}{voice}",
        references = references_section(references),
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
    run_stream(app, state, stream_id, provider, model, system, messages, false, false)
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
    run_stream(app, state, stream_id, provider, model, system, messages, false, false)
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
    voice: Option<String>,
) -> Result<()> {
    let model = model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| provider.default_model().to_string());
    let system = system_prompt(&document_content, &references, voice.as_deref());
    // chat: the system prefix (instructions + document) is re-sent every turn —
    // cache it so unchanged-document follow-ups read back at ~0.1× input price.
    // Enable server-side compaction so a long master chat stays bounded without
    // hard-dropping context (Anthropic + supported model only).
    let compact = provider == Provider::Anthropic && supports_compaction(&model);
    run_stream(app, state, stream_id, provider, model, system, messages, true, compact)
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
    run_stream(app, state, stream_id, provider, model, system, messages, false, false)
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
) -> Result<()> {
    let Some(api_key) = get_api_key(&app, provider)? else {
        return Err(Error::InvalidInput("no API key configured".into()));
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
                )
                .await
            }
            Provider::Openrouter => {
                stream_openrouter(&task_app, &task_id, &api_key, &model, messages, &system).await
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
    Err(Error::InvalidInput(format!("{provider} API error {status}: {detail}")))
}

/// One meaningful piece extracted from an SSE event.
enum Chunk {
    /// A text delta to append to the response.
    Token(String),
    /// A token-usage update (absolute counts; 0 means "unknown, leave as-is").
    Usage { input: u64, output: u64 },
    /// Nothing of interest in this event.
    None,
}

#[derive(Default)]
struct Usage {
    input: u64,
    output: u64,
}

/// Send a provider request and emit a token event per SSE payload. `extract`
/// is the only provider-specific part: pull the text delta or usage out of an
/// event, or return the provider's in-stream error. A final `assistant:usage`
/// event is emitted once the stream completes.
async fn stream_sse(
    app: &AppHandle,
    stream_id: &str,
    provider_label: &str,
    request: reqwest::RequestBuilder,
    extract: impl Fn(&serde_json::Value) -> Result<Chunk>,
) -> Result<()> {
    let response = request
        .send()
        .await
        .map_err(|e| Error::InvalidInput(format!("request failed: {e}")))?;
    let response = error_for_status(response, provider_label).await?;

    let mut usage = Usage::default();
    for_each_sse_data(response, |data| {
        if data == "[DONE]" {
            return Ok(());
        }
        let Ok(event) = serde_json::from_str::<serde_json::Value>(data) else {
            return Ok(());
        };
        match extract(&event)? {
            Chunk::Token(text) => {
                if !text.is_empty() {
                    let _ = app.emit(EVT_TOKEN, json!({ "id": stream_id, "text": text }));
                }
            }
            // providers report usage incrementally (e.g. input first, output
            // last); keep the latest non-zero value for each field
            Chunk::Usage { input, output } => {
                if input > 0 {
                    usage.input = input;
                }
                if output > 0 {
                    usage.output = output;
                }
            }
            Chunk::None => {}
        }
        Ok(())
    })
    .await?;

    if usage.input > 0 || usage.output > 0 {
        let _ = app.emit(
            EVT_USAGE,
            json!({ "id": stream_id, "inputTokens": usage.input, "outputTokens": usage.output }),
        );
    }
    Ok(())
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
/// turns on server-side context compaction (chat only).
fn anthropic_request_body(
    model: &str,
    messages: &[ChatMessage],
    system: &str,
    cache_system: bool,
    compact: bool,
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
    if supports_adaptive_thinking(model) {
        body["thinking"] = json!({"type": "adaptive"});
    }
    if compact {
        // default trigger (150K input tokens); the API summarizes older history
        // into a compaction block and continues from the summary
        body["context_management"] = json!({ "edits": [{ "type": "compact_20260112" }] });
    }
    body
}

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
) -> Result<()> {
    let body = anthropic_request_body(model, &messages, system, cache_system, compact);
    let mut request = reqwest::Client::new()
        .post(ANTHROPIC_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json");
    if compact {
        request = request.header("anthropic-beta", COMPACT_BETA);
    }
    let request = request.json(&body);

    // Accumulate the assistant turn so a compaction block can be round-tripped:
    // (visible_text, compaction_summary). Mutex (not RefCell) so the streaming
    // future stays `Send` — `extract` is held across the SSE `.await` loop.
    let captured = Mutex::new((String::new(), Option::<String>::None));

    stream_sse(app, stream_id, "Anthropic", request, |event| match event["type"].as_str() {
        Some("content_block_delta") if event["delta"]["type"] == "text_delta" => {
            let text = event["delta"]["text"].as_str().unwrap_or_default();
            captured.lock().expect("capture mutex poisoned").0.push_str(text);
            Ok(if text.is_empty() { Chunk::None } else { Chunk::Token(text.into()) })
        }
        // the compaction summary arrives as a single complete delta; capture it
        // but never surface it as visible reply text
        Some("content_block_delta") if event["delta"]["type"] == "compaction_delta" => {
            if let Some(summary) = event["delta"]["content"].as_str() {
                captured.lock().expect("capture mutex poisoned").1 = Some(summary.to_string());
            }
            Ok(Chunk::None)
        }
        // input_tokens arrive in message_start; output_tokens accumulate in message_delta
        Some("message_start") => Ok(anthropic_usage(&event["message"]["usage"])),
        Some("message_delta") => Ok(anthropic_usage(&event["usage"])),
        Some("error") => {
            let msg = event["error"]["message"].as_str().unwrap_or("stream error");
            Err(Error::InvalidInput(msg.to_string()))
        }
        _ => Ok(Chunk::None),
    })
    .await?;

    // Only on success: if a compaction block was produced, emit the full
    // content-block array so the frontend can persist it and replay it verbatim.
    // (Aborted/errored turns never reach here, so no partial blocks persist.)
    let (text, compaction) = captured.into_inner().expect("capture mutex poisoned");
    if let Some(summary) = compaction {
        let content = json!([
            { "type": "compaction", "content": summary },
            { "type": "text", "text": text },
        ]);
        let _ = app.emit(EVT_CONTENT, json!({ "id": stream_id, "content": content }));
    }
    Ok(())
}

/// Pull a usage chunk from an Anthropic `usage` object. The reported input is
/// the prompt total including any cached tokens.
fn anthropic_usage(u: &serde_json::Value) -> Chunk {
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    Chunk::Usage {
        input: n("input_tokens") + n("cache_read_input_tokens") + n("cache_creation_input_tokens"),
        output: n("output_tokens"),
    }
}

async fn stream_openrouter(
    app: &AppHandle,
    stream_id: &str,
    api_key: &str,
    model: &str,
    messages: Vec<ChatMessage>,
    system: &str,
) -> Result<()> {
    let mut all_messages = vec![json!({
        "role": "system",
        "content": system,
    })];
    all_messages.extend(messages.iter().map(|m| json!({"role": m.role, "content": m.content})));

    let body = json!({
        "model": model,
        "messages": all_messages,
        "stream": true,
        // ask for a final usage chunk (OpenAI-compatible streaming option)
        "stream_options": {"include_usage": true},
    });
    let request = reqwest::Client::new()
        .post(OPENROUTER_URL)
        .header("authorization", format!("Bearer {api_key}"))
        .header("content-type", "application/json")
        .header("http-referer", "https://github.com/WalrusQuant/plume")
        .header("x-title", "Plume")
        .json(&body);

    stream_sse(app, stream_id, "OpenRouter", request, |event| {
        if let Some(msg) = event["error"]["message"].as_str() {
            return Err(Error::InvalidInput(msg.to_string()));
        }
        // the final chunk carries usage with an empty choices array
        if event["usage"].is_object() {
            return Ok(Chunk::Usage {
                input: event["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
                output: event["usage"]["completion_tokens"].as_u64().unwrap_or(0),
            });
        }
        Ok(event["choices"][0]["delta"]["content"]
            .as_str()
            .map_or(Chunk::None, |t| Chunk::Token(t.into())))
    })
    .await
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
        let cached = anthropic_request_body("claude-opus-4-8", &msgs, "SYS", true, false);
        assert!(cached["system"].is_array(), "cached system must be a content-block array");
        assert_eq!(cached["system"][0]["text"], "SYS");
        assert_eq!(cached["system"][0]["cache_control"]["type"], "ephemeral");

        // one-shot paths: plain string system, no cache-write premium
        let plain = anthropic_request_body("claude-opus-4-8", &msgs, "SYS", false, false);
        assert!(plain["system"].is_string());
        assert_eq!(plain["system"], "SYS");
        assert!(plain["system"][0]["cache_control"].is_null());

        // caching is orthogonal to the existing model-gated thinking field
        assert_eq!(cached["thinking"]["type"], "adaptive");
        let haiku = anthropic_request_body("claude-haiku-4-5", &msgs, "SYS", false, false);
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
        let off = anthropic_request_body("claude-opus-4-8", &plain_msgs, "SYS", true, false);
        assert!(off.get("context_management").is_none());
        let on = anthropic_request_body("claude-opus-4-8", &plain_msgs, "SYS", true, true);
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
        let body = anthropic_request_body("claude-opus-4-8", &with_blocks, "SYS", true, true);
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
        let body = anthropic_request_body("claude-opus-4-8", &msgs, "SYS", true, true);
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
        assert!(system_prompt("doc", &[], v).contains("terse, lowercase, dry wit"));
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
        assert!(!system_prompt("doc", &[], None).contains("Voice & tone"));
        assert!(!system_prompt("doc", &[], Some("   ")).contains("Voice & tone"));
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
        assert!(!system_prompt("doc", &[], None).contains("Referenced"));
    }

    #[test]
    fn references_injected_after_document_before_voice() {
        let refs = vec![DocReference {
            name: "Pricing notes".into(),
            content: "we charge $9/mo".into(),
        }];
        let prompt = system_prompt("the live doc", &refs, Some("terse and dry"));
        // reference content is present and labeled
        assert!(prompt.contains("Referenced: \"Pricing notes\""));
        assert!(prompt.contains("we charge $9/mo"));
        // ordering: editable doc → references → voice (style stays last)
        let doc = prompt.find("the live doc").unwrap();
        let refs_at = prompt.find("Referenced:").unwrap();
        let voice = prompt.find("Voice & tone").unwrap();
        assert!(doc < refs_at && refs_at < voice);
    }
}
