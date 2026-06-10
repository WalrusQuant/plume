use rusqlite::{Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Document type drives template lookup and, later, export defaults.
/// Stored as its kebab-case string; validated here, not by a CHECK constraint
/// (SQLite can't alter one later).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DocType {
    BlogPost,
    Newsletter,
    LinkedinPost,
    XThread,
    Skill,
    ClaudeMd,
    SystemPrompt,
    Runbook,
    Generic,
}

impl DocType {
    pub fn as_str(self) -> &'static str {
        match self {
            DocType::BlogPost => "blog-post",
            DocType::Newsletter => "newsletter",
            DocType::LinkedinPost => "linkedin-post",
            DocType::XThread => "x-thread",
            DocType::Skill => "skill",
            DocType::ClaudeMd => "claude-md",
            DocType::SystemPrompt => "system-prompt",
            DocType::Runbook => "runbook",
            DocType::Generic => "generic",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "blog-post" => Ok(DocType::BlogPost),
            "newsletter" => Ok(DocType::Newsletter),
            "linkedin-post" => Ok(DocType::LinkedinPost),
            "x-thread" => Ok(DocType::XThread),
            "skill" => Ok(DocType::Skill),
            "claude-md" => Ok(DocType::ClaudeMd),
            "system-prompt" => Ok(DocType::SystemPrompt),
            "runbook" => Ok(DocType::Runbook),
            "generic" => Ok(DocType::Generic),
            other => Err(Error::InvalidInput(format!("unknown document type: {other}"))),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub doc_type: DocType,
    pub folder_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Why a document snapshot was captured. Stored as its kebab-case string;
/// validated here, mirroring `DocType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapshotCause {
    AiEdit,
    Interval,
    Manual,
    Restore,
}

impl SnapshotCause {
    pub fn as_str(self) -> &'static str {
        match self {
            SnapshotCause::AiEdit => "ai-edit",
            SnapshotCause::Interval => "interval",
            SnapshotCause::Manual => "manual",
            SnapshotCause::Restore => "restore",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "ai-edit" => Ok(SnapshotCause::AiEdit),
            "interval" => Ok(SnapshotCause::Interval),
            "manual" => Ok(SnapshotCause::Manual),
            "restore" => Ok(SnapshotCause::Restore),
            other => Err(Error::InvalidInput(format!("unknown snapshot cause: {other}"))),
        }
    }
}

/// Snapshot metadata for the history list — content is fetched separately so
/// the list stays cheap even with large documents.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMeta {
    pub id: String,
    pub cause: SnapshotCause,
    pub word_count: usize,
    pub created_at: String,
}

/// Schema migrations, applied in order; `PRAGMA user_version` tracks progress.
/// Append-only: never edit an entry that has shipped.
const MIGRATIONS: &[&str] = &[
    // v1 — initial schema
    "CREATE TABLE folders (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL,
        parent_id TEXT REFERENCES folders(id) ON DELETE SET NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE TABLE documents (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL,
        type TEXT NOT NULL DEFAULT 'generic',
        folder_id TEXT REFERENCES folders(id) ON DELETE SET NULL,
        content TEXT NOT NULL DEFAULT '',
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );",
    // v2 — per-document AI chat threads
    "CREATE TABLE chat_messages (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        role TEXT NOT NULL,
        content TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE INDEX idx_chat_messages_doc ON chat_messages(document_id);",
    // v3 — document version snapshots (history / restore points)
    "CREATE TABLE snapshots (
        id TEXT PRIMARY KEY NOT NULL,
        document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        content TEXT NOT NULL,
        cause TEXT NOT NULL,
        word_count INTEGER NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE INDEX idx_snapshots_doc ON snapshots(document_id, created_at DESC);",
];

/// Open-time setup: pragmas + migrations. Call once per connection.
pub fn init(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;",
    )?;
    migrate(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    let version: usize =
        conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))? as usize;
    for (i, migration) in MIGRATIONS.iter().enumerate().skip(version) {
        conn.execute_batch(&format!(
            "BEGIN;\n{}\nPRAGMA user_version = {};\nCOMMIT;",
            migration,
            i + 1
        ))?;
    }
    Ok(())
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn validated_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::InvalidInput("name must not be empty".into()));
    }
    Ok(name)
}

fn document_from_row(row: &Row) -> rusqlite::Result<Document> {
    Ok(Document {
        id: row.get(0)?,
        name: row.get(1)?,
        doc_type: DocType::parse(row.get::<_, String>(2)?.as_str()).unwrap_or(DocType::Generic),
        folder_id: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

const DOC_COLUMNS: &str = "id, name, type, folder_id, created_at, updated_at";

fn get_document(conn: &Connection, id: &str) -> Result<Document> {
    conn.query_row(
        &format!("SELECT {DOC_COLUMNS} FROM documents WHERE id = ?1"),
        [id],
        document_from_row,
    )
    .optional()?
    .ok_or(Error::NotFound("document"))
}

pub fn list_documents(conn: &Connection) -> Result<Vec<Document>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {DOC_COLUMNS} FROM documents ORDER BY updated_at DESC"
    ))?;
    let docs = stmt
        .query_map([], document_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(docs)
}

pub fn create_document(
    conn: &Connection,
    name: &str,
    doc_type: Option<DocType>,
    content: Option<&str>,
) -> Result<Document> {
    let name = validated_name(name)?;
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    conn.execute(
        "INSERT INTO documents (id, name, type, folder_id, content, created_at, updated_at)
         VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?5)",
        rusqlite::params![
            id,
            name,
            doc_type.unwrap_or(DocType::Generic).as_str(),
            content.unwrap_or(""),
            ts
        ],
    )?;
    get_document(conn, &id)
}

pub fn rename_document(conn: &Connection, id: &str, name: &str) -> Result<Document> {
    let name = validated_name(name)?;
    let changed = conn.execute(
        "UPDATE documents SET name = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![id, name, now()],
    )?;
    if changed == 0 {
        return Err(Error::NotFound("document"));
    }
    get_document(conn, id)
}

pub fn move_document(conn: &Connection, id: &str, folder_id: Option<&str>) -> Result<Document> {
    if let Some(fid) = folder_id {
        let exists: Option<i64> = conn
            .query_row("SELECT 1 FROM folders WHERE id = ?1", [fid], |r| r.get(0))
            .optional()?;
        if exists.is_none() {
            return Err(Error::NotFound("folder"));
        }
    }
    let changed = conn.execute(
        "UPDATE documents SET folder_id = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![id, folder_id, now()],
    )?;
    if changed == 0 {
        return Err(Error::NotFound("document"));
    }
    get_document(conn, id)
}

pub fn delete_document(conn: &Connection, id: &str) -> Result<()> {
    let changed = conn.execute("DELETE FROM documents WHERE id = ?1", [id])?;
    if changed == 0 {
        return Err(Error::NotFound("document"));
    }
    Ok(())
}

pub fn get_document_content(conn: &Connection, id: &str) -> Result<String> {
    conn.query_row("SELECT content FROM documents WHERE id = ?1", [id], |row| {
        row.get(0)
    })
    .optional()?
    .ok_or(Error::NotFound("document"))
}

pub fn save_document_content(conn: &Connection, id: &str, content: &str) -> Result<()> {
    let changed = conn.execute(
        "UPDATE documents SET content = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![id, content, now()],
    )?;
    if changed == 0 {
        return Err(Error::NotFound("document"));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredChatMessage {
    pub role: String,
    pub content: String,
}

pub fn get_chat_messages(conn: &Connection, document_id: &str) -> Result<Vec<StoredChatMessage>> {
    let mut stmt = conn.prepare(
        "SELECT role, content FROM chat_messages WHERE document_id = ?1 ORDER BY id",
    )?;
    let messages = stmt
        .query_map([document_id], |row| {
            Ok(StoredChatMessage {
                role: row.get(0)?,
                content: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(messages)
}

/// Replace the whole thread for a document (simple + handles clear).
pub fn save_chat_messages(
    conn: &mut Connection,
    document_id: &str,
    messages: &[StoredChatMessage],
) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM chat_messages WHERE document_id = ?1", [document_id])?;
    {
        let ts = now();
        let mut stmt = tx.prepare(
            "INSERT INTO chat_messages (document_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for msg in messages {
            stmt.execute(rusqlite::params![document_id, msg.role, msg.content, ts])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Keep at most this many snapshots per document (most recent kept).
const MAX_SNAPSHOTS_PER_DOC: usize = 50;

fn word_count(content: &str) -> usize {
    content.split_whitespace().count()
}

/// Capture a version of a document. Returns `None` (no-op) when the content is
/// identical to the document's most recent snapshot, so periodic saves don't
/// pile up duplicates. Prunes to the most recent `MAX_SNAPSHOTS_PER_DOC`.
pub fn create_snapshot(
    conn: &Connection,
    document_id: &str,
    content: &str,
    cause: SnapshotCause,
) -> Result<Option<SnapshotMeta>> {
    let exists: Option<i64> = conn
        .query_row("SELECT 1 FROM documents WHERE id = ?1", [document_id], |r| r.get(0))
        .optional()?;
    if exists.is_none() {
        return Err(Error::NotFound("document"));
    }

    let latest: Option<String> = conn
        .query_row(
            "SELECT content FROM snapshots WHERE document_id = ?1
             ORDER BY created_at DESC, rowid DESC LIMIT 1",
            [document_id],
            |r| r.get(0),
        )
        .optional()?;
    if latest.as_deref() == Some(content) {
        return Ok(None);
    }

    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    let wc = word_count(content);
    conn.execute(
        "INSERT INTO snapshots (id, document_id, content, cause, word_count, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, document_id, content, cause.as_str(), wc as i64, ts],
    )?;
    prune_snapshots(conn, document_id)?;
    Ok(Some(SnapshotMeta { id, cause, word_count: wc, created_at: ts }))
}

fn prune_snapshots(conn: &Connection, document_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM snapshots
         WHERE document_id = ?1
           AND id NOT IN (
             SELECT id FROM snapshots WHERE document_id = ?1
             ORDER BY created_at DESC, rowid DESC LIMIT ?2
           )",
        rusqlite::params![document_id, MAX_SNAPSHOTS_PER_DOC as i64],
    )?;
    Ok(())
}

pub fn list_snapshots(conn: &Connection, document_id: &str) -> Result<Vec<SnapshotMeta>> {
    let mut stmt = conn.prepare(
        "SELECT id, cause, word_count, created_at FROM snapshots
         WHERE document_id = ?1 ORDER BY created_at DESC, rowid DESC",
    )?;
    let rows = stmt
        .query_map([document_id], |row| {
            let cause: String = row.get(1)?;
            Ok(SnapshotMeta {
                id: row.get(0)?,
                cause: SnapshotCause::parse(&cause).unwrap_or(SnapshotCause::Manual),
                word_count: row.get::<_, i64>(2)? as usize,
                created_at: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn get_snapshot_content(conn: &Connection, snapshot_id: &str) -> Result<String> {
    conn.query_row("SELECT content FROM snapshots WHERE id = ?1", [snapshot_id], |r| r.get(0))
        .optional()?
        .ok_or(Error::NotFound("snapshot"))
}

fn folder_from_row(row: &Row) -> rusqlite::Result<Folder> {
    Ok(Folder {
        id: row.get(0)?,
        name: row.get(1)?,
        parent_id: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

const FOLDER_COLUMNS: &str = "id, name, parent_id, created_at, updated_at";

pub fn list_folders(conn: &Connection) -> Result<Vec<Folder>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {FOLDER_COLUMNS} FROM folders ORDER BY name COLLATE NOCASE"
    ))?;
    let folders = stmt
        .query_map([], folder_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(folders)
}

pub fn create_folder(conn: &Connection, name: &str) -> Result<Folder> {
    let name = validated_name(name)?;
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    conn.execute(
        "INSERT INTO folders (id, name, parent_id, created_at, updated_at)
         VALUES (?1, ?2, NULL, ?3, ?3)",
        rusqlite::params![id, name, ts],
    )?;
    conn.query_row(
        &format!("SELECT {FOLDER_COLUMNS} FROM folders WHERE id = ?1"),
        [&id],
        folder_from_row,
    )
    .map_err(Into::into)
}

pub fn rename_folder(conn: &Connection, id: &str, name: &str) -> Result<Folder> {
    let name = validated_name(name)?;
    let changed = conn.execute(
        "UPDATE folders SET name = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![id, name, now()],
    )?;
    if changed == 0 {
        return Err(Error::NotFound("folder"));
    }
    conn.query_row(
        &format!("SELECT {FOLDER_COLUMNS} FROM folders WHERE id = ?1"),
        [id],
        folder_from_row,
    )
    .map_err(Into::into)
}

pub fn delete_folder(conn: &Connection, id: &str) -> Result<()> {
    let changed = conn.execute("DELETE FROM folders WHERE id = ?1", [id])?;
    if changed == 0 {
        return Err(Error::NotFound("folder"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        conn
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = test_conn();
        migrate(&conn).unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version as usize, MIGRATIONS.len());
    }

    #[test]
    fn document_crud_roundtrip() {
        let conn = test_conn();
        let doc = create_document(&conn, "  My Post  ", Some(DocType::BlogPost), None).unwrap();
        assert_eq!(doc.name, "My Post"); // trimmed
        assert_eq!(doc.doc_type, DocType::BlogPost);
        assert_eq!(doc.folder_id, None);

        let renamed = rename_document(&conn, &doc.id, "Better Title").unwrap();
        assert_eq!(renamed.name, "Better Title");
        assert!(renamed.updated_at >= doc.updated_at);

        assert_eq!(list_documents(&conn).unwrap().len(), 1);
        delete_document(&conn, &doc.id).unwrap();
        assert!(list_documents(&conn).unwrap().is_empty());
    }

    #[test]
    fn default_type_is_generic() {
        let conn = test_conn();
        let doc = create_document(&conn, "Untitled", None, None).unwrap();
        assert_eq!(doc.doc_type, DocType::Generic);
    }

    #[test]
    fn content_save_and_load() {
        let conn = test_conn();
        let doc = create_document(&conn, "Draft", None, Some("# Hello")).unwrap();
        assert_eq!(get_document_content(&conn, &doc.id).unwrap(), "# Hello");

        save_document_content(&conn, &doc.id, "# Hello\n\nWorld.").unwrap();
        assert_eq!(
            get_document_content(&conn, &doc.id).unwrap(),
            "# Hello\n\nWorld."
        );
    }

    #[test]
    fn move_document_into_and_out_of_folder() {
        let conn = test_conn();
        let folder = create_folder(&conn, "Posts").unwrap();
        let doc = create_document(&conn, "Draft", None, None).unwrap();

        let moved = move_document(&conn, &doc.id, Some(&folder.id)).unwrap();
        assert_eq!(moved.folder_id.as_deref(), Some(folder.id.as_str()));

        let moved_out = move_document(&conn, &doc.id, None).unwrap();
        assert_eq!(moved_out.folder_id, None);
    }

    #[test]
    fn move_to_missing_folder_fails() {
        let conn = test_conn();
        let doc = create_document(&conn, "Draft", None, None).unwrap();
        let err = move_document(&conn, &doc.id, Some("nope")).unwrap_err();
        assert!(matches!(err, Error::NotFound("folder")));
    }

    #[test]
    fn deleting_folder_orphans_documents_not_deletes() {
        let conn = test_conn();
        let folder = create_folder(&conn, "Posts").unwrap();
        let doc = create_document(&conn, "Draft", None, None).unwrap();
        move_document(&conn, &doc.id, Some(&folder.id)).unwrap();

        delete_folder(&conn, &folder.id).unwrap();
        let docs = list_documents(&conn).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].folder_id, None); // ON DELETE SET NULL
    }

    #[test]
    fn missing_ids_return_not_found() {
        let conn = test_conn();
        assert!(matches!(
            get_document_content(&conn, "nope").unwrap_err(),
            Error::NotFound("document")
        ));
        assert!(matches!(
            rename_document(&conn, "nope", "x").unwrap_err(),
            Error::NotFound("document")
        ));
        assert!(matches!(
            delete_folder(&conn, "nope").unwrap_err(),
            Error::NotFound("folder")
        ));
    }

    #[test]
    fn empty_names_rejected() {
        let conn = test_conn();
        assert!(matches!(
            create_document(&conn, "   ", None, None).unwrap_err(),
            Error::InvalidInput(_)
        ));
        assert!(matches!(
            create_folder(&conn, "", ).unwrap_err(),
            Error::InvalidInput(_)
        ));
    }

    #[test]
    fn chat_messages_roundtrip_and_cascade() {
        let mut conn = test_conn();
        let doc = create_document(&conn, "Draft", None, None).unwrap();

        let thread = vec![
            StoredChatMessage { role: "user".into(), content: "hi".into() },
            StoredChatMessage { role: "assistant".into(), content: "hello".into() },
        ];
        save_chat_messages(&mut conn, &doc.id, &thread).unwrap();
        let loaded = get_chat_messages(&conn, &doc.id).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].role, "user");
        assert_eq!(loaded[1].content, "hello");

        // replace-all semantics (clear)
        save_chat_messages(&mut conn, &doc.id, &[]).unwrap();
        assert!(get_chat_messages(&conn, &doc.id).unwrap().is_empty());

        // deleting the document cascades its chat
        save_chat_messages(&mut conn, &doc.id, &thread).unwrap();
        delete_document(&conn, &doc.id).unwrap();
        let orphans: i64 = conn
            .query_row("SELECT COUNT(*) FROM chat_messages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(orphans, 0);
    }

    #[test]
    fn snapshots_capture_list_and_fetch() {
        let conn = test_conn();
        let doc = create_document(&conn, "Draft", None, Some("v1")).unwrap();

        let s1 = create_snapshot(&conn, &doc.id, "v1", SnapshotCause::Manual).unwrap();
        assert!(s1.is_some());
        // identical content to the latest snapshot is a no-op
        assert!(create_snapshot(&conn, &doc.id, "v1", SnapshotCause::Interval)
            .unwrap()
            .is_none());

        let s2 = create_snapshot(&conn, &doc.id, "v2 two words", SnapshotCause::Interval)
            .unwrap()
            .unwrap();
        assert_eq!(s2.word_count, 3);

        let list = list_snapshots(&conn, &doc.id).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, s2.id); // most recent first
        assert_eq!(list[0].cause, SnapshotCause::Interval);
        assert_eq!(get_snapshot_content(&conn, &s2.id).unwrap(), "v2 two words");
    }

    #[test]
    fn snapshots_cascade_on_document_delete() {
        let conn = test_conn();
        let doc = create_document(&conn, "Draft", None, None).unwrap();
        create_snapshot(&conn, &doc.id, "x", SnapshotCause::Manual).unwrap();
        delete_document(&conn, &doc.id).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn snapshots_pruned_to_cap() {
        let conn = test_conn();
        let doc = create_document(&conn, "Draft", None, None).unwrap();
        for i in 0..(MAX_SNAPSHOTS_PER_DOC + 10) {
            create_snapshot(&conn, &doc.id, &format!("content {i}"), SnapshotCause::Interval)
                .unwrap();
        }
        assert_eq!(list_snapshots(&conn, &doc.id).unwrap().len(), MAX_SNAPSHOTS_PER_DOC);
    }

    #[test]
    fn snapshot_on_missing_document_fails() {
        let conn = test_conn();
        assert!(matches!(
            create_snapshot(&conn, "nope", "x", SnapshotCause::Manual).unwrap_err(),
            Error::NotFound("document")
        ));
        assert!(matches!(
            get_snapshot_content(&conn, "nope").unwrap_err(),
            Error::NotFound("snapshot")
        ));
    }

    #[test]
    fn snapshot_cause_serde_roundtrip() {
        for c in [
            SnapshotCause::AiEdit,
            SnapshotCause::Interval,
            SnapshotCause::Manual,
            SnapshotCause::Restore,
        ] {
            assert_eq!(SnapshotCause::parse(c.as_str()).unwrap(), c);
            assert_eq!(serde_json::to_string(&c).unwrap(), format!("\"{}\"", c.as_str()));
        }
    }

    #[test]
    fn doc_type_serde_roundtrip() {
        for t in [
            DocType::BlogPost,
            DocType::Newsletter,
            DocType::LinkedinPost,
            DocType::XThread,
            DocType::Skill,
            DocType::ClaudeMd,
            DocType::SystemPrompt,
            DocType::Runbook,
            DocType::Generic,
        ] {
            assert_eq!(DocType::parse(t.as_str()).unwrap(), t);
            let json = serde_json::to_string(&t).unwrap();
            assert_eq!(json, format!("\"{}\"", t.as_str()));
        }
    }
}
