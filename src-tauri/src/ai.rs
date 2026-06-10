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

pub fn start_stream(
    app: AppHandle,
    state: &AiState,
    stream_id: String,
    provider: Provider,
    model: Option<String>,
    messages: Vec<ChatMessage>,
    document_content: String,
) -> Result<()> {
    let Some(api_key) = get_api_key(&app, provider)? else {
        return Err(Error::InvalidInput("no API key configured".into()));
    };
    let model = model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| provider.default_model().to_string());

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
                stream_anthropic(&task_app, &task_id, &api_key, &model, messages, &document_content)
                    .await
            }
            Provider::Openrouter => {
                stream_openrouter(&task_app, &task_id, &api_key, &model, messages, &document_content)
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

/// Send a provider request and emit a token event per SSE payload. `extract`
/// is the only provider-specific part: pull the text delta out of an event
/// (Ok(None) to skip it) or return the provider's in-stream error.
async fn stream_sse(
    app: &AppHandle,
    stream_id: &str,
    provider_label: &str,
    request: reqwest::RequestBuilder,
    extract: impl Fn(&serde_json::Value) -> Result<Option<String>>,
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
        let Ok(event) = serde_json::from_str::<serde_json::Value>(data) else {
            return Ok(());
        };
        if let Some(text) = extract(&event)? {
            if !text.is_empty() {
                let _ = app.emit(EVT_TOKEN, json!({ "id": stream_id, "text": text }));
            }
        }
        Ok(())
    })
    .await
}

async fn stream_anthropic(
    app: &AppHandle,
    stream_id: &str,
    api_key: &str,
    model: &str,
    messages: Vec<ChatMessage>,
    document_content: &str,
) -> Result<()> {
    let body = json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "system": system_prompt(document_content),
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
            Ok(event["delta"]["text"].as_str().map(String::from))
        }
        Some("error") => {
            let msg = event["error"]["message"].as_str().unwrap_or("stream error");
            Err(Error::InvalidInput(msg.to_string()))
        }
        _ => Ok(None),
    })
    .await
}

async fn stream_openrouter(
    app: &AppHandle,
    stream_id: &str,
    api_key: &str,
    model: &str,
    messages: Vec<ChatMessage>,
    document_content: &str,
) -> Result<()> {
    let mut all_messages = vec![json!({
        "role": "system",
        "content": system_prompt(document_content),
    })];
    all_messages.extend(messages.iter().map(|m| json!({"role": m.role, "content": m.content})));

    let body = json!({
        "model": model,
        "messages": all_messages,
        "stream": true,
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
        Ok(event["choices"][0]["delta"]["content"].as_str().map(String::from))
    })
    .await
}
