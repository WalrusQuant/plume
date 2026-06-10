use std::collections::HashMap;
use std::sync::Mutex;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::{Error, Result};

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

fn system_prompt(document_content: &str) -> String {
    format!(
        "You are an AI writing partner embedded in Markdown, a desktop writing app for \
         content creators who write in markdown and publish across platforms (blogs, \
         newsletters, LinkedIn, X). The user is editing a document; its current content \
         is below.\n\n\
         Current document content:\n---\n{document_content}\n---\n\n\
         Help the user review, improve, or generate content. Be concise. When you \
         suggest a revised version of the document, provide the complete markdown in a \
         ```markdown code block so it can be applied directly."
    )
}

/// System prompt for an inline edit: the model is editing a selected fragment
/// of the document and must return ONLY the replacement text — no preamble, no
/// code fences — so the result can be spliced straight back into the editor.
fn inline_system_prompt(document_content: &str, selected_text: &str) -> String {
    format!(
        "You are editing a fragment of a markdown document inside Markdown, a desktop \
         writing app. Apply the user's instruction to the selected text and return ONLY \
         the replacement text. Do not add any preamble, explanation, quotes, or code \
         fences — output only the edited fragment, ready to paste in place. Match the \
         surrounding markdown style and keep the user's voice.\n\n\
         Full document (for context):\n---\n{document_content}\n---\n\n\
         Selected text to edit:\n---\n{selected_text}\n---"
    )
}

/// System prompt for expanding a captured idea fragment into a full draft of
/// the given target document type. Returns ONLY the markdown body of the draft
/// (no preamble, no code fences) so it can be written straight into a new doc.
fn expand_system_prompt(idea: &str, target_label: &str) -> String {
    format!(
        "You are a writing partner inside Plume, a desktop writing app for content \
         creators. The user captured a rough idea and wants it expanded into a \
         structured first draft for a {target_label}. Develop the idea into a complete, \
         well-organized draft written in markdown: a clear angle, logical sections, and \
         concrete substance — not filler. Match the conventions of a {target_label}. \
         Return ONLY the markdown body of the draft: no preamble, no explanation, no \
         surrounding code fences.\n\n\
         The captured idea:\n---\n{idea}\n---"
    )
}

/// Expand a captured idea into a draft of `target_label`, streaming the draft
/// markdown over the same `assistant:*` events (filtered by stream id, so the
/// chat panel ignores it). Uses the provider's default (strong) model — this is
/// a generative job. Shares the single AiState slot (mutually exclusive with
/// chat/inline edit).
pub fn start_expand_stream(
    app: AppHandle,
    state: &AiState,
    stream_id: String,
    provider: Provider,
    model: Option<String>,
    idea: String,
    target_label: String,
) -> Result<()> {
    let model = model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| provider.default_model().to_string());
    let system = expand_system_prompt(&idea, &target_label);
    let messages = vec![ChatMessage {
        role: "user".into(),
        content: format!("Expand my idea into a {target_label} draft."),
    }];
    run_stream(app, state, stream_id, provider, model, system, messages)
}

pub fn start_stream(
    app: AppHandle,
    state: &AiState,
    stream_id: String,
    provider: Provider,
    model: Option<String>,
    messages: Vec<ChatMessage>,
    document_content: String,
) -> Result<()> {
    let model = model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| provider.default_model().to_string());
    let system = system_prompt(&document_content);
    run_stream(app, state, stream_id, provider, model, system, messages)
}

/// Start an inline edit: apply `instruction` to `selected_text`, streaming the
/// replacement back over the same `assistant:*` events (the frontend filters by
/// stream id, so the chat panel ignores it). Defaults to the provider's faster
/// model. Shares the single AiState slot — starting this aborts any chat stream
/// and vice versa.
pub fn start_inline_stream(
    app: AppHandle,
    state: &AiState,
    stream_id: String,
    provider: Provider,
    model: Option<String>,
    instruction: String,
    selected_text: String,
    document_content: String,
) -> Result<()> {
    let model = model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| provider.fast_model().to_string());
    let system = inline_system_prompt(&document_content, &selected_text);
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
        let prompt = inline_system_prompt("# Doc\n\nbody text", "body text");
        assert!(prompt.contains("body text"));
        assert!(prompt.contains("# Doc"));
        // the replace-only instruction is what keeps the output spliceable
        assert!(prompt.contains("ONLY the replacement text"));
        assert!(prompt.contains("code fences"));
    }

    #[test]
    fn expand_prompt_includes_idea_and_target() {
        let prompt = expand_system_prompt("a post about local-first apps", "Newsletter");
        assert!(prompt.contains("a post about local-first apps"));
        assert!(prompt.contains("Newsletter"));
        // body-only so it can be written straight into a new doc
        assert!(prompt.contains("ONLY the markdown body"));
    }
}
