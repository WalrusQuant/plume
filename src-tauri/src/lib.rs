mod ai;
mod commands;
mod error;
mod export;
mod preview;
mod storage;

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
            let conn = Connection::open(data_dir.join("markdown.db"))?;
            storage::init(&conn)?;
            app.manage(Db(Mutex::new(conn)));
            app.manage(ai::AiState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_documents,
            commands::create_document,
            commands::rename_document,
            commands::move_document,
            commands::delete_document,
            commands::get_document_content,
            commands::save_document_content,
            commands::render_preview,
            commands::render_linkedin_preview,
            commands::render_x_thread_preview,
            commands::render_x_article_preview,
            commands::list_export_targets,
            commands::export_document,
            commands::list_folders,
            commands::create_folder,
            commands::rename_folder,
            commands::delete_folder,
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
            commands::send_assistant_message,
            commands::send_inline_edit,
            commands::stop_assistant,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
