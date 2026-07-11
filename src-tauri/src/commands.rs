use std::sync::Mutex;

use rusqlite::Connection;
use tauri::{AppHandle, State};

use crate::ai::{self, AiState, ChatMessage, DocReference, Provider};
use crate::embed::EmbedState;
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
    embed: State<EmbedState>,
    name: String,
    doc_type: Option<DocType>,
    content: Option<String>,
) -> Result<Document> {
    let doc = db.with(|conn| storage::create_document(conn, &name, doc_type, content.as_deref()))?;
    let _ = embed.tx.try_send(());
    Ok(doc)
}

#[tauri::command]
pub fn rename_document(db: State<Db>, id: String, name: String) -> Result<Document> {
    db.with(|conn| storage::rename_document(conn, &id, &name))
}

#[tauri::command]
pub fn update_idea_name(
    db: State<Db>,
    id: String,
    name: String,
    explicit: bool,
) -> Result<Document> {
    db.with(|conn| storage::update_idea_name(conn, &id, &name, explicit))
}

#[tauri::command]
pub fn update_document_type(
    db: State<Db>,
    embed: State<EmbedState>,
    id: String,
    doc_type: DocType,
    name: String,
    explicit: bool,
) -> Result<Document> {
    // Promoting an idea to a real doc makes it eligible for indexing.
    let doc = db.with(|conn| storage::update_document_type(conn, &id, doc_type, &name, explicit))?;
    let _ = embed.tx.try_send(());
    Ok(doc)
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
pub fn save_document_content(
    db: State<Db>,
    embed: State<EmbedState>,
    id: String,
    content: String,
) -> Result<()> {
    db.with(|conn| storage::save_document_content(conn, &id, &content))?;
    let _ = embed.tx.try_send(());
    Ok(())
}

#[tauri::command]
pub fn search_documents(db: State<Db>, query: String) -> Result<Vec<storage::SearchHit>> {
    db.with(|conn| storage::search_documents(conn, &query))
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
pub fn set_folder_active(db: State<Db>, id: String, active: bool) -> Result<Folder> {
    db.with(|conn| storage::set_folder_active(conn, &id, active))
}

#[tauri::command]
pub fn delete_folder(db: State<Db>, id: String) -> Result<()> {
    db.with(|conn| storage::delete_folder(conn, &id))
}

#[tauri::command]
pub fn reorder_documents(db: State<Db>, ids: Vec<String>) -> Result<()> {
    db.with(|conn| storage::reorder_documents(conn, &ids))
}

#[tauri::command]
pub fn reorder_folders(db: State<Db>, ids: Vec<String>) -> Result<()> {
    db.with(|conn| storage::reorder_folders(conn, &ids))
}

#[tauri::command]
pub fn render_linkedin_preview(content: String) -> String {
    export::linkedin::render(&content)
}

#[tauri::command]
pub fn render_x_thread_preview(content: String) -> String {
    export::x::render_thread_text(&content)
}

#[tauri::command]
pub fn render_x_article_preview(content: String) -> String {
    // bound to the export renderer so the preview tracks the actual paste
    export::x::render_article_html(&content)
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
        "x-thread" => Ok(ExportOutput::Clipboard {
            text: export::x::render_thread_text(&content),
        }),
        "x-article" => Ok(ExportOutput::ClipboardHtml {
            html: export::x::render_article_html(&content),
            plain: export::x::render_plain(&content),
        }),
        "mastodon" => Ok(ExportOutput::Clipboard {
            text: export::mastodon::render_thread_text(&content),
        }),
        "bluesky" => Ok(ExportOutput::Clipboard {
            text: export::bluesky::render_thread_text(&content),
        }),
        "threads" => Ok(ExportOutput::Clipboard {
            text: export::threads::render_thread_text(&content),
        }),
        "reddit" => Ok(ExportOutput::Clipboard {
            text: export::reddit::render(&content),
        }),
        "discord" => Ok(ExportOutput::Clipboard {
            text: export::discord::render(&content),
        }),
        "telegram" => Ok(ExportOutput::Clipboard {
            text: export::telegram::render(&content),
        }),
        "google-docs" => Ok(ExportOutput::ClipboardHtml {
            html: export::richhtml::render(&content, export::richhtml::Flavor::Bare),
            plain: export::plaintext::render(&content),
        }),
        "newsletter" => Ok(ExportOutput::ClipboardHtml {
            html: export::richhtml::render(&content, export::richhtml::Flavor::Newsletter),
            plain: export::plaintext::render(&content),
        }),
        "markdown" => {
            let c = content.clone();
            let bytes = tauri::async_runtime::spawn_blocking(move || {
                export::markdown::render(&c).as_bytes().to_vec()
            })
            .await
            .map_err(|e| Error::InvalidInput(format!("export task failed: {e}")))?;
            save_to_file(&app, &doc_name, target, bytes).await
        }
        "plaintext-file" | "plaintext" => {
            let c = content.clone();
            let bytes = tauri::async_runtime::spawn_blocking(move || {
                export::plaintext::render(&c).into_bytes()
            })
            .await
            .map_err(|e| Error::InvalidInput(format!("export task failed: {e}")))?;
            save_to_file(&app, &doc_name, target, bytes).await
        }
        "html" => {
            // Rendering is CPU-bound (and docx decodes/re-encodes every embedded
            // image) — run it off the async runtime so a big export can't stall
            // the app or starve other commands.
            let (c, n) = (content.clone(), doc_name.clone());
            let bytes = tauri::async_runtime::spawn_blocking(move || {
                export::html::render(&c, &n).into_bytes()
            })
            .await
            .map_err(|e| Error::InvalidInput(format!("export task failed: {e}")))?;
            save_to_file(&app, &doc_name, target, bytes).await
        }
        "rtf" => {
            let c = content.clone();
            let bytes = tauri::async_runtime::spawn_blocking(move || {
                export::rtf::render(&c).into_bytes()
            })
            .await
            .map_err(|e| Error::InvalidInput(format!("export task failed: {e}")))?;
            save_to_file(&app, &doc_name, target, bytes).await
        }
        "docx" => {
            let c = content.clone();
            let bytes = tauri::async_runtime::spawn_blocking(move || export::docx::render(&c))
                .await
                .map_err(|e| Error::InvalidInput(format!("export task failed: {e}")))??;
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
pub fn list_chats(db: State<Db>, document_id: String) -> Result<Vec<storage::Chat>> {
    db.with(|conn| storage::list_chats(conn, &document_id))
}

#[tauri::command]
pub fn create_chat(
    db: State<Db>,
    document_id: String,
    title: Option<String>,
) -> Result<storage::Chat> {
    db.with(|conn| storage::create_chat(conn, &document_id, title.as_deref()))
}

#[tauri::command]
pub fn rename_chat(db: State<Db>, chat_id: String, title: String) -> Result<storage::Chat> {
    db.with(|conn| storage::rename_chat(conn, &chat_id, &title))
}

#[tauri::command]
pub fn delete_chat(db: State<Db>, chat_id: String) -> Result<()> {
    db.with(|conn| storage::delete_chat(conn, &chat_id))
}

#[tauri::command]
pub fn get_chat_messages(db: State<Db>, chat_id: String) -> Result<Vec<storage::StoredChatMessage>> {
    db.with(|conn| storage::get_chat_messages(conn, &chat_id))
}

#[tauri::command]
pub fn save_chat_messages(
    db: State<Db>,
    chat_id: String,
    messages: Vec<storage::StoredChatMessage>,
) -> Result<()> {
    db.with_mut(|conn| storage::save_chat_messages(conn, &chat_id, &messages))
}

#[tauri::command]
pub fn create_snapshot(
    db: State<Db>,
    document_id: String,
    content: String,
    cause: storage::SnapshotCause,
) -> Result<Option<storage::SnapshotMeta>> {
    db.with(|conn| storage::create_snapshot(conn, &document_id, &content, cause))
}

#[tauri::command]
pub fn list_snapshots(db: State<Db>, document_id: String) -> Result<Vec<storage::SnapshotMeta>> {
    db.with(|conn| storage::list_snapshots(conn, &document_id))
}

#[tauri::command]
pub fn get_snapshot_content(db: State<Db>, snapshot_id: String) -> Result<String> {
    db.with(|conn| storage::get_snapshot_content(conn, &snapshot_id))
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
pub fn set_tavily_key(app: AppHandle, key: String) -> Result<()> {
    ai::set_tavily_key(&app, &key)
}

#[tauri::command]
pub fn has_tavily_key(app: AppHandle) -> Result<bool> {
    Ok(ai::get_tavily_key(&app)?.is_some())
}

#[tauri::command]
pub fn delete_tavily_key(app: AppHandle) -> Result<()> {
    ai::delete_tavily_key(&app)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn send_assistant_message(
    app: AppHandle,
    state: State<AiState>,
    stream_id: String,
    provider: Provider,
    model: Option<String>,
    messages: Vec<ChatMessage>,
    document_content: String,
    references: Vec<DocReference>,
    web_search: bool,
    voice: Option<String>,
) -> Result<()> {
    ai::start_stream(
        app,
        &state,
        stream_id,
        provider,
        model,
        messages,
        document_content,
        references,
        web_search,
        voice,
    )
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn send_inline_edit(
    app: AppHandle,
    state: State<AiState>,
    stream_id: String,
    provider: Provider,
    model: Option<String>,
    instruction: String,
    selected_text: String,
    document_content: String,
    voice: Option<String>,
) -> Result<()> {
    ai::start_inline_stream(
        app,
        &state,
        stream_id,
        provider,
        model,
        instruction,
        selected_text,
        document_content,
        voice,
    )
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn send_idea_expand(
    app: AppHandle,
    state: State<AiState>,
    stream_id: String,
    provider: Provider,
    model: Option<String>,
    idea: String,
    target_label: String,
    voice: Option<String>,
) -> Result<()> {
    ai::start_expand_stream(app, &state, stream_id, provider, model, idea, target_label, voice)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn send_content_multiply(
    app: AppHandle,
    state: State<AiState>,
    stream_id: String,
    provider: Provider,
    model: Option<String>,
    source_content: String,
    target: DocType,
    target_label: String,
    voice: Option<String>,
) -> Result<()> {
    ai::start_content_multiply_stream(
        app,
        &state,
        stream_id,
        provider,
        model,
        source_content,
        target,
        target_label,
        voice,
    )
}

#[tauri::command]
pub fn stop_assistant(app: AppHandle, state: State<AiState>) {
    ai::stop_stream(&app, &state)
}
