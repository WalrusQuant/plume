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
    }];
    run_stream(app, state, stream_id, provider, model, system, messages)
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
    }];
    run_stream(app, state, stream_id, provider, model, system, messages)
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
    run_stream(app, state, stream_id, provider, model, system, messages)
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
    let messages = vec![ChatMessage { role: "user".into(), content: instruction }];
    run_stream(app, state, stream_id, provider, model, system, messages)
}

/// Shared driver: load the key, abort any in-flight stream, then spawn the
/// provider request as the new in-flight task. `system` is the fully-built
/// system prompt (chat or inline); `model` is already resolved.
fn run_stream(
    app: AppHandle,
    state: &AiState,
    stream_id: String,
    provider: Provider,
    model: String,
    system: String,
    messages: Vec<ChatMessage>,
) -> Result<()> {
    let Some(api_key) = get_api_key(&app, provider)? else {
        return Err(Error::InvalidInput("no API key configured".into()));
    };

    let mut current = state.current.lock().expect("ai mutex poisoned");
    if let Some((old_id, handle)) = current.take() {
        handle.abort();
        let _ = app.emit(EVT_DONE, json!({ "id": old_id }));
    }
    let task_app = app.clone();
    let task_id = stream_id.clone();
    let task = tauri::async_runtime::spawn(async move {
        let result = match provider {
            Provider::Anthropic => {
                stream_anthropic(&task_app, &task_id, &api_key, &model, messages, &system).await
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
        let _ = app.emit(EVT_DONE, json!({ "id": id }));
    }
}

/// Read SSE `data:` payloads from a response, calling `on_event` per payload.
async fn for_each_sse_data(
    response: reqwest::Response,
    mut on_event: impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Error::InvalidInput(format!("stream error: {e}")))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim_end().to_string();
            buffer.drain(..=pos);
            if let Some(data) = line.strip_prefix("data: ") {
                on_event(data)?;
            }
        }
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

async fn stream_anthropic(
    app: &AppHandle,
    stream_id: &str,
    api_key: &str,
    model: &str,
    messages: Vec<ChatMessage>,
    system: &str,
) -> Result<()> {
    let body = json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "system": system,
        "messages": messages,
        "thinking": {"type": "adaptive"},
        "stream": true,
    });
    let request = reqwest::Client::new()
        .post(ANTHROPIC_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .json(&body);

    stream_sse(app, stream_id, "Anthropic", request, |event| match event["type"].as_str() {
        Some("content_block_delta") if event["delta"]["type"] == "text_delta" => {
            Ok(event["delta"]["text"].as_str().map_or(Chunk::None, |t| Chunk::Token(t.into())))
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
    .await
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
        .header("http-referer", "https://github.com/adamwickwire/markdown")
        .header("x-title", "Markdown")
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
