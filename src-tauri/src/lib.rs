mod ai;
mod commands;
mod embed;
mod error;
mod export;
mod preview;
mod storage;
mod websearch;

use std::sync::Mutex;

use rusqlite::Connection;
use tauri::Manager;

use commands::Db;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("markdown.db");
            // Safety net: snapshot the DB before migrations can touch it. If a
            // future migration half-applies, the user can roll back by hand.
            // Cheap for the small DBs this app produces; skipped on first run.
            if db_path.exists() {
                let bak = data_dir.join("markdown.db.bak");
                let _ = std::fs::copy(&db_path, &bak);
            }
            let conn = Connection::open(&db_path)?;
            storage::init(&conn)?;
            app.manage(Db(Mutex::new(conn)));
            app.manage(ai::AiState::default());

            // Semantic notebook: a background worker keeps the chunk index in
            // sync. It owns its own DB connection (never the Db mutex) and the
            // one shared embedding model; writes nudge it via the channel.
            let (tx, rx) = tokio::sync::mpsc::channel::<()>(8);
            let embedder = std::sync::Arc::new(embed::FastEmbedder::new(data_dir.clone()));
            app.manage(embed::EmbedState {
                tx,
                embedder: embedder.clone(),
            });
            tauri::async_runtime::spawn(embed::run_worker(
                app.handle().clone(),
                db_path.clone(),
                embedder,
                rx,
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_documents,
            commands::create_document,
            commands::rename_document,
            commands::update_idea_name,
            commands::update_document_type,
            commands::move_document,
            commands::delete_document,
            commands::get_document_content,
            commands::save_document_content,
            commands::search_documents,
            commands::render_preview,
            commands::render_linkedin_preview,
            commands::render_x_thread_preview,
            commands::render_x_article_preview,
            commands::list_export_targets,
            commands::export_document,
            commands::list_folders,
            commands::create_folder,
            commands::rename_folder,
            commands::set_folder_active,
            commands::delete_folder,
            commands::reorder_documents,
            commands::reorder_folders,
            commands::list_chats,
            commands::create_chat,
            commands::rename_chat,
            commands::delete_chat,
            commands::get_chat_messages,
            commands::save_chat_messages,
            commands::create_snapshot,
            commands::list_snapshots,
            commands::get_snapshot_content,
            commands::set_api_key,
            commands::has_api_key,
            commands::delete_api_key,
            commands::set_tavily_key,
            commands::has_tavily_key,
            commands::delete_tavily_key,
            commands::send_assistant_message,
            commands::send_inline_edit,
            commands::send_idea_expand,
            commands::send_content_multiply,
            commands::stop_assistant,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
