use std::sync::Mutex;

use rusqlite::Connection;
use tauri::{AppHandle, State};

use crate::ai::{self, AiState, ChatMessage, Provider};
use crate::error::{Error, Result};
use crate::export::{self, ExportOutput, ExportTarget};
use crate::storage::{self, DocType, Document, Folder};

/// App-wide database handle. rusqlite connections are not Sync, so commands
/// serialize access through this mutex; for a single-user local app that is
/// plenty.
pub struct Db(pub Mutex<Connection>);

impl Db {
    fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = self.0.lock().expect("db mutex poisoned");
        f(&conn)
    }

    fn with_mut<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut conn = self.0.lock().expect("db mutex poisoned");
        f(&mut conn)
    }
}

#[tauri::command]
pub fn list_documents(db: State<Db>) -> Result<Vec<Document>> {
    db.with(storage::list_documents)
}

#[tauri::command]
pub fn create_document(
    db: State<Db>,
    name: String,
    doc_type: Option<DocType>,
    content: Option<String>,
) -> Result<Document> {
    db.with(|conn| storage::create_document(conn, &name, doc_type, content.as_deref()))
}

#[tauri::command]
pub fn rename_document(db: State<Db>, id: String, name: String) -> Result<Document> {
    db.with(|conn| storage::rename_document(conn, &id, &name))
}

#[tauri::command]
pub fn move_document(db: State<Db>, id: String, folder_id: Option<String>) -> Result<Document> {
    db.with(|conn| storage::move_document(conn, &id, folder_id.as_deref()))
}

#[tauri::command]
pub fn delete_document(db: State<Db>, id: String) -> Result<()> {
    db.with(|conn| storage::delete_document(conn, &id))
}

#[tauri::command]
pub fn get_document_content(db: State<Db>, id: String) -> Result<String> {
    db.with(|conn| storage::get_document_content(conn, &id))
}

#[tauri::command]
pub fn save_document_content(db: State<Db>, id: String, content: String) -> Result<()> {
    db.with(|conn| storage::save_document_content(conn, &id, &content))
}

#[tauri::command]
pub fn render_preview(content: String) -> String {
    crate::preview::render_html(&content)
}

#[tauri::command]
pub fn list_folders(db: State<Db>) -> Result<Vec<Folder>> {
    db.with(storage::list_folders)
}

#[tauri::command]
pub fn create_folder(db: State<Db>, name: String) -> Result<Folder> {
    db.with(|conn| storage::create_folder(conn, &name))
}

#[tauri::command]
pub fn rename_folder(db: State<Db>, id: String, name: String) -> Result<Folder> {
    db.with(|conn| storage::rename_folder(conn, &id, &name))
}

#[tauri::command]
pub fn delete_folder(db: State<Db>, id: String) -> Result<()> {
    db.with(|conn| storage::delete_folder(conn, &id))
}

#[tauri::command]
pub fn render_linkedin_preview(content: String) -> String {
    export::linkedin::render(&content)
}

#[tauri::command]
pub fn list_export_targets() -> Vec<ExportTarget> {
    export::TARGETS.to_vec()
}

#[tauri::command]
pub async fn export_document(
    app: AppHandle,
    content: String,
    doc_name: String,
    target_id: String,
) -> Result<ExportOutput> {
    let target = export::TARGETS
        .iter()
        .find(|t| t.id == target_id)
        .ok_or_else(|| Error::InvalidInput(format!("unknown export target: {target_id}")))?;

    match target.id {
        "linkedin" => Ok(ExportOutput::Clipboard {
            text: export::linkedin::render(&content),
        }),
        "html" => {
            let bytes = export::html::render(&content, &doc_name).into_bytes();
            save_to_file(&app, &doc_name, target, bytes).await
        }
        "docx" => {
            let bytes = export::docx::render(&content)?;
            save_to_file(&app, &doc_name, target, bytes).await
        }
        other => Err(Error::InvalidInput(format!("unhandled export target: {other}"))),
    }
}

/// Native save dialog (blocking — run off the async runtime), then write.
async fn save_to_file(
    app: &AppHandle,
    doc_name: &str,
    target: &ExportTarget,
    bytes: Vec<u8>,
) -> Result<ExportOutput> {
    use tauri_plugin_dialog::DialogExt;

    let ext = target.ext.expect("file target has an extension");
    let default_name = format!("{doc_name}.{ext}");
    let dialog = app.dialog().file().set_file_name(&default_name).add_filter(target.label, &[ext]);

    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_save_file())
        .await
        .map_err(|e| Error::InvalidInput(format!("dialog task failed: {e}")))?;

    let Some(file_path) = picked else {
        return Ok(ExportOutput::Cancelled);
    };
    let path = file_path
        .into_path()
        .map_err(|e| Error::InvalidInput(format!("invalid save path: {e}")))?;
    std::fs::write(&path, bytes)?;
    Ok(ExportOutput::File { path: path.display().to_string() })
}

#[tauri::command]
pub fn get_chat_messages(db: State<Db>, document_id: String) -> Result<Vec<storage::StoredChatMessage>> {
    db.with(|conn| storage::get_chat_messages(conn, &document_id))
}

#[tauri::command]
pub fn save_chat_messages(
    db: State<Db>,
    document_id: String,
    messages: Vec<storage::StoredChatMessage>,
) -> Result<()> {
    db.with_mut(|conn| storage::save_chat_messages(conn, &document_id, &messages))
}

#[tauri::command]
pub fn set_api_key(app: AppHandle, provider: Provider, key: String) -> Result<()> {
    ai::set_api_key(&app, provider, &key)
}

#[tauri::command]
pub fn has_api_key(app: AppHandle, provider: Provider) -> Result<bool> {
    Ok(ai::get_api_key(&app, provider)?.is_some())
}

#[tauri::command]
pub fn delete_api_key(app: AppHandle, provider: Provider) -> Result<()> {
    ai::delete_api_key(&app, provider)
}

#[tauri::command]
pub fn send_assistant_message(
    app: AppHandle,
    state: State<AiState>,
    provider: Provider,
    model: Option<String>,
    messages: Vec<ChatMessage>,
    document_content: String,
) -> Result<()> {
    ai::start_stream(app, &state, provider, model, messages, document_content)
}

#[tauri::command]
pub fn stop_assistant(app: AppHandle, state: State<AiState>) {
    ai::stop_stream(&app, &state)
}
