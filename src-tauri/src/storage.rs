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
    Plan,
    BuildLog,
    Idea,
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
            DocType::Plan => "plan",
            DocType::BuildLog => "build-log",
            DocType::Idea => "idea",
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
            "plan" => Ok(DocType::Plan),
            "build-log" => Ok(DocType::BuildLog),
            "idea" => Ok(DocType::Idea),
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
    /// True once the user has set the name deliberately (typed a title or
    /// renamed). False means the name is derived (ideas: from the first line)
    /// and may be auto-updated. The two states are mutually exclusive, so
    /// auto-derivation can never clobber a manual title.
    pub title_explicit: bool,
    /// Manual position within its sidebar section (folder docs / unfiled /
    /// Inbox). Lower sorts first. Independent of `updated_at` so reordering
    /// never disturbs recency-based views (shelf Recent, project freshness).
    pub sort_order: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Shelf curation: active projects sit on top, resting ones collapse
    /// below. This is the entire "planner" — no statuses beyond it.
    pub active: bool,
    /// Manual position among the folders. Lower sorts first; new folders append.
    pub sort_order: i64,
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

/// An AI chat thread within a document.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Chat {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Default title for a freshly created chat (matched on the frontend to know
/// when to auto-title from the first user message).
pub const DEFAULT_CHAT_TITLE: &str = "New chat";

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
    // v4 — multiple chat threads per document + per-message token usage
    "CREATE TABLE chats (
        id TEXT PRIMARY KEY NOT NULL,
        document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        title TEXT NOT NULL DEFAULT 'New chat',
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE INDEX idx_chats_doc ON chats(document_id, updated_at DESC);
    ALTER TABLE chat_messages ADD COLUMN chat_id TEXT REFERENCES chats(id) ON DELETE CASCADE;
    ALTER TABLE chat_messages ADD COLUMN input_tokens INTEGER;
    ALTER TABLE chat_messages ADD COLUMN output_tokens INTEGER;
    CREATE INDEX idx_chat_messages_chat ON chat_messages(chat_id);",
    // v5 — explicit-vs-derived title flag (ideas: manual title vs first-line)
    "ALTER TABLE documents ADD COLUMN title_explicit INTEGER NOT NULL DEFAULT 0;",
    // v6 — full-text search index over document titles + bodies. Standalone
    // fts5 table keyed by the TEXT doc id (UNINDEXED — UUIDs aren't searchable
    // text; documents has no integer rowid alias, so external-content is out).
    // Kept in sync by explicit upserts in the write path (fts_index/fts_delete),
    // not triggers. The backfill indexes any pre-existing documents on upgrade.
    "CREATE VIRTUAL TABLE documents_fts USING fts5(
        id UNINDEXED,
        name,
        content,
        tokenize = 'unicode61'
    );
    INSERT INTO documents_fts (id, name, content)
        SELECT id, name, content FROM documents;",
    // v7 — project active flag (the shelf's only curation: active on top,
    // resting below). Defaults to active so an upgraded shelf isn't empty.
    "ALTER TABLE folders ADD COLUMN active INTEGER NOT NULL DEFAULT 1;",
    // v8 — manual sort order for the sidebar/shelf. Backfill preserves the
    // existing implicit order exactly: documents ranked by updated_at DESC,
    // folders by name (NOCASE). A rowid tiebreak keeps rows with equal keys
    // from sharing a rank (which would make the manual order ambiguous).
    "ALTER TABLE documents ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
     UPDATE documents SET sort_order = (
        SELECT COUNT(*) FROM documents d2
        WHERE d2.updated_at > documents.updated_at
           OR (d2.updated_at = documents.updated_at AND d2.rowid < documents.rowid)
     );
     ALTER TABLE folders ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
     UPDATE folders SET sort_order = (
        SELECT COUNT(*) FROM folders f2
        WHERE f2.name < folders.name COLLATE NOCASE
           OR (f2.name = folders.name COLLATE NOCASE AND f2.rowid < folders.rowid)
     );",
    // v9 — raw assistant content blocks (JSON array) for an assistant turn that
    // carries a server-side compaction summary, so it round-trips verbatim on
    // replay (the Anthropic API drops everything before the compaction block).
    // NULL for every existing row and for plain text/user turns.
    "ALTER TABLE chat_messages ADD COLUMN raw_content TEXT;",
    // v10 — semantic chunk index (bge-small, 384-dim, L2-normalized f32 BLOB).
    // Chunks cascade with their document (real FK, unlike the fts5 mirror).
    // `embedded_at` NULL means the doc needs (re)embedding; the background
    // worker (Slice 2) reconciles it. Ideas are never embedded (see D6).
    "CREATE TABLE chunks (
        id          TEXT PRIMARY KEY NOT NULL,
        document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        ordinal     INTEGER NOT NULL,
        content     TEXT NOT NULL,
        embedding   BLOB NOT NULL,           -- 384 x f32 LE = 1536 bytes
        created_at  TEXT NOT NULL
    );
    CREATE INDEX idx_chunks_doc ON chunks(document_id);
    ALTER TABLE documents ADD COLUMN embedded_at TEXT;",
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
        // Each migration runs in its own transaction. On error the `Transaction`
        // guard rolls back automatically (its Drop impl issues ROLLBACK), so a
        // half-applied migration can never leave the connection in an open
        // transaction or advance user_version past the applied statements.
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(migration)?;
        tx.execute(&format!("PRAGMA user_version = {}", i + 1), [])?;
        tx.commit()?;
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
        title_explicit: row.get(6)?,
        sort_order: row.get(7)?,
    })
}

const DOC_COLUMNS: &str =
    "id, name, type, folder_id, created_at, updated_at, title_explicit, sort_order";

fn get_document(conn: &Connection, id: &str) -> Result<Document> {
    conn.query_row(
        &format!("SELECT {DOC_COLUMNS} FROM documents WHERE id = ?1"),
        [id],
        document_from_row,
    )
    .optional()?
    .ok_or(Error::NotFound("document"))
}

/// Re-read a document's name+content and upsert it into the FTS index. Delete +
/// insert keeps the standalone fts5 table consistent (no UPDATE on an UNINDEXED
/// key). Re-reading the row means callers never pass name/content, so the index
/// can't drift from the document. Call after any write that changes name/content.
fn fts_index(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM documents_fts WHERE id = ?1", [id])?;
    conn.execute(
        "INSERT INTO documents_fts (id, name, content)
         SELECT id, name, content FROM documents WHERE id = ?1",
        [id],
    )?;
    Ok(())
}

/// Remove a document from the FTS index (fts5 doesn't participate in FK cascades).
fn fts_delete(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM documents_fts WHERE id = ?1", [id])?;
    Ok(())
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
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO documents (id, name, type, folder_id, content, created_at, updated_at, sort_order)
         VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?5,
                 COALESCE((SELECT MIN(sort_order) FROM documents), 1) - 1)",
        rusqlite::params![
            id,
            name,
            doc_type.unwrap_or(DocType::Generic).as_str(),
            content.unwrap_or(""),
            ts
        ],
    )?;
    fts_index(&tx, &id)?;
    tx.commit()?;
    get_document(conn, &id)
}

pub fn rename_document(conn: &Connection, id: &str, name: &str) -> Result<Document> {
    let name = validated_name(name)?;
    // A rename is always a deliberate act → the name becomes explicit.
    let tx = conn.unchecked_transaction()?;
    let changed = tx.execute(
        "UPDATE documents SET name = ?2, title_explicit = 1, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![id, name, now()],
    )?;
    if changed == 0 {
        return Err(Error::NotFound("document"));
    }
    fts_index(&tx, id)?;
    tx.commit()?;
    get_document(conn, id)
}

/// Set an idea's name along with its explicit/derived flag. The capture modal
/// uses this for both cases: a typed title (`explicit = true`, sticks) and an
/// empty title (`explicit = false`, derived from the first line). Unlike
/// `rename_document` it can return a name to the derived state.
pub fn update_idea_name(
    conn: &Connection,
    id: &str,
    name: &str,
    explicit: bool,
) -> Result<Document> {
    let name = validated_name(name)?;
    let tx = conn.unchecked_transaction()?;
    let changed = tx.execute(
        "UPDATE documents SET name = ?2, title_explicit = ?3, updated_at = ?4 WHERE id = ?1",
        rusqlite::params![id, name, explicit, now()],
    )?;
    if changed == 0 {
        return Err(Error::NotFound("document"));
    }
    fts_index(&tx, id)?;
    tx.commit()?;
    get_document(conn, id)
}

/// Convert a document to another type, atomically locking in a real title.
/// Used to promote an idea into a regular document (it then leaves the Inbox,
/// which filters purely on `type`).
pub fn update_document_type(
    conn: &Connection,
    id: &str,
    doc_type: DocType,
    name: &str,
    explicit: bool,
) -> Result<Document> {
    let name = validated_name(name)?;
    let tx = conn.unchecked_transaction()?;
    let changed = tx.execute(
        "UPDATE documents SET type = ?2, name = ?3, title_explicit = ?4, updated_at = ?5 WHERE id = ?1",
        rusqlite::params![id, doc_type.as_str(), name, explicit, now()],
    )?;
    if changed == 0 {
        return Err(Error::NotFound("document"));
    }
    fts_index(&tx, id)?;
    tx.commit()?;
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
    // Land at the top of the destination section: a stale sort_order from the
    // old folder would otherwise drop it into an arbitrary mid-list position.
    let changed = conn.execute(
        "UPDATE documents SET folder_id = ?2, updated_at = ?3,
                sort_order = COALESCE((SELECT MIN(sort_order) FROM documents), 1) - 1
         WHERE id = ?1",
        rusqlite::params![id, folder_id, now()],
    )?;
    if changed == 0 {
        return Err(Error::NotFound("document"));
    }
    get_document(conn, id)
}

/// Apply a manual ordering to documents: each id's `sort_order` becomes its
/// index in `ids`. Reorder-only — never touches `updated_at` (reordering must
/// not pollute recency views). Unknown ids are silently skipped (a 0-row
/// UPDATE) so a benign race against a concurrent delete can't fail the drop.
pub fn reorder_documents(conn: &Connection, ids: &[String]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    for (i, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE documents SET sort_order = ?2 WHERE id = ?1",
            rusqlite::params![id, i as i64],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn delete_document(conn: &Connection, id: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let changed = tx.execute("DELETE FROM documents WHERE id = ?1", [id])?;
    if changed == 0 {
        return Err(Error::NotFound("document"));
    }
    fts_delete(&tx, id)?;
    tx.commit()?;
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
    let tx = conn.unchecked_transaction()?;
    let changed = tx.execute(
        "UPDATE documents SET content = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![id, content, now()],
    )?;
    if changed == 0 {
        return Err(Error::NotFound("document"));
    }
    fts_index(&tx, id)?;
    tx.commit()?;
    Ok(())
}

/// A full-text search result: enough to render a ranked sidebar row and open the
/// doc on click. Content is represented by a highlighted `snippet`, not the full
/// body (kept cheap, like list_documents omits content).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub doc_type: DocType,
    pub snippet: String,
}

/// Build a SAFE FTS5 MATCH query from arbitrary user input. Each whitespace token
/// is stripped to alphanumerics and wrapped as a quoted prefix term (`"foo"*`).
/// Quoting neutralizes FTS5 operators (AND/OR/NOT/NEAR), column filters (`col:`),
/// and punctuation that would otherwise be a parse error; the trailing `*` gives
/// prefix matching. Returns None when nothing searchable remains.
fn fts_query(raw: &str) -> Option<String> {
    let terms: Vec<String> = raw
        .split_whitespace()
        .map(|t| t.chars().filter(|c| c.is_alphanumeric()).collect::<String>())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{t}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

/// Full-text search over document titles + bodies (excludes Inbox ideas). Title
/// matches outrank body matches via bm25 column weights. Returns at most 50 hits,
/// most-relevant first. An empty/punctuation-only query yields no hits (not an
/// error, and no full-table scan).
pub fn search_documents(conn: &Connection, query: &str) -> Result<Vec<SearchHit>> {
    let Some(match_query) = fts_query(query) else {
        return Ok(Vec::new());
    };
    let mut stmt = conn.prepare(
        "SELECT f.id, d.name, d.type,
                snippet(documents_fts, 2, '[', ']', '…', 12)
         FROM documents_fts f
         JOIN documents d ON d.id = f.id
         WHERE documents_fts MATCH ?1 AND d.type <> 'idea'
         ORDER BY bm25(documents_fts, 0.0, 10.0, 1.0)
         LIMIT 50",
    )?;
    let hits = stmt
        .query_map([match_query], |row| {
            Ok(SearchHit {
                id: row.get(0)?,
                name: row.get(1)?,
                doc_type: DocType::parse(row.get::<_, String>(2)?.as_str())
                    .unwrap_or(DocType::Generic),
                snippet: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(hits)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<i64>,
    /// Raw assistant content-block array (JSON) when this turn carries a
    /// compaction summary; replayed verbatim so the summary round-trips. None
    /// for user turns, plain text replies, and legacy rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_content: Option<serde_json::Value>,
}

/// Parse the `raw_content` TEXT column. Malformed JSON degrades to None rather
/// than failing the whole read — a turn that can't replay its blocks just falls
/// back to plain text (compaction state forfeited, no crash).
fn parse_raw_content(stored: Option<String>) -> Option<serde_json::Value> {
    stored.and_then(|s| serde_json::from_str(&s).ok())
}

fn chat_from_row(row: &Row) -> rusqlite::Result<Chat> {
    Ok(Chat {
        id: row.get(0)?,
        title: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
    })
}

const CHAT_COLUMNS: &str = "id, title, created_at, updated_at";

pub fn create_chat(conn: &Connection, document_id: &str, title: Option<&str>) -> Result<Chat> {
    let exists: Option<i64> = conn
        .query_row("SELECT 1 FROM documents WHERE id = ?1", [document_id], |r| r.get(0))
        .optional()?;
    if exists.is_none() {
        return Err(Error::NotFound("document"));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    let title = title
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or(DEFAULT_CHAT_TITLE);
    conn.execute(
        "INSERT INTO chats (id, document_id, title, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        rusqlite::params![id, document_id, title, ts],
    )?;
    conn.query_row(
        &format!("SELECT {CHAT_COLUMNS} FROM chats WHERE id = ?1"),
        [&id],
        chat_from_row,
    )
    .map_err(Into::into)
}

pub fn list_chats(conn: &Connection, document_id: &str) -> Result<Vec<Chat>> {
    // Lazy migration: messages saved before v4 have chat_id IS NULL. Wrap any
    // such legacy thread for this document into one chat on first access.
    let orphans: i64 = conn.query_row(
        "SELECT COUNT(*) FROM chat_messages WHERE document_id = ?1 AND chat_id IS NULL",
        [document_id],
        |r| r.get(0),
    )?;
    if orphans > 0 {
        let chat = create_chat(conn, document_id, Some("Chat"))?;
        conn.execute(
            "UPDATE chat_messages SET chat_id = ?1 WHERE document_id = ?2 AND chat_id IS NULL",
            rusqlite::params![chat.id, document_id],
        )?;
    }
    let mut stmt = conn.prepare(&format!(
        "SELECT {CHAT_COLUMNS} FROM chats WHERE document_id = ?1 ORDER BY updated_at DESC, rowid DESC"
    ))?;
    let chats = stmt
        .query_map([document_id], chat_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(chats)
}

pub fn rename_chat(conn: &Connection, chat_id: &str, title: &str) -> Result<Chat> {
    let title = validated_name(title)?;
    let changed = conn.execute(
        "UPDATE chats SET title = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![chat_id, title, now()],
    )?;
    if changed == 0 {
        return Err(Error::NotFound("chat"));
    }
    conn.query_row(
        &format!("SELECT {CHAT_COLUMNS} FROM chats WHERE id = ?1"),
        [chat_id],
        chat_from_row,
    )
    .map_err(Into::into)
}

pub fn delete_chat(conn: &Connection, chat_id: &str) -> Result<()> {
    let changed = conn.execute("DELETE FROM chats WHERE id = ?1", [chat_id])?;
    if changed == 0 {
        return Err(Error::NotFound("chat"));
    }
    Ok(())
}

pub fn get_chat_messages(conn: &Connection, chat_id: &str) -> Result<Vec<StoredChatMessage>> {
    let mut stmt = conn.prepare(
        "SELECT role, content, input_tokens, output_tokens, raw_content FROM chat_messages
         WHERE chat_id = ?1 ORDER BY id",
    )?;
    let messages = stmt
        .query_map([chat_id], |row| {
            Ok(StoredChatMessage {
                role: row.get(0)?,
                content: row.get(1)?,
                input_tokens: row.get(2)?,
                output_tokens: row.get(3)?,
                raw_content: parse_raw_content(row.get(4)?),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(messages)
}

/// Replace the whole thread for a chat (simple + handles clear). Bumps the
/// chat's `updated_at` so active chats sort to the top of the list — but only
/// when something actually changed: merely opening a chat must not reorder the
/// list or rewrite message timestamps. Rows whose role+content are unchanged
/// keep their original `created_at`.
pub fn save_chat_messages(
    conn: &mut Connection,
    chat_id: &str,
    messages: &[StoredChatMessage],
) -> Result<()> {
    // chat_messages.document_id is NOT NULL (v2 schema); derive it from the chat.
    let document_id: Option<String> = conn
        .query_row("SELECT document_id FROM chats WHERE id = ?1", [chat_id], |r| r.get(0))
        .optional()?;
    let Some(document_id) = document_id else {
        return Err(Error::NotFound("chat"));
    };

    // Existing rows (with timestamps), in thread order.
    let existing: Vec<(StoredChatMessage, String)> = {
        let mut stmt = conn.prepare(
            "SELECT role, content, input_tokens, output_tokens, raw_content, created_at
             FROM chat_messages WHERE chat_id = ?1 ORDER BY id",
        )?;
        let rows = stmt
            .query_map([chat_id], |row| {
                Ok((
                    StoredChatMessage {
                        role: row.get(0)?,
                        content: row.get(1)?,
                        input_tokens: row.get(2)?,
                        output_tokens: row.get(3)?,
                        raw_content: parse_raw_content(row.get(4)?),
                    },
                    row.get(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };

    // raw_content is part of identity: a turn that *gains* a compaction block
    // (same role+content, new raw_content) must NOT be short-circuited as
    // unchanged, or the blocks would never persist (silent data loss).
    let identical = |a: &StoredChatMessage, b: &StoredChatMessage| {
        a.role == b.role
            && a.content == b.content
            && a.input_tokens == b.input_tokens
            && a.output_tokens == b.output_tokens
            && a.raw_content == b.raw_content
    };
    if existing.len() == messages.len()
        && existing.iter().zip(messages).all(|((e, _), m)| identical(e, m))
    {
        return Ok(()); // nothing changed — leave timestamps and sort order alone
    }

    let tx = conn.transaction()?;
    tx.execute("DELETE FROM chat_messages WHERE chat_id = ?1", [chat_id])?;
    let ts = now();
    {
        let mut stmt = tx.prepare(
            "INSERT INTO chat_messages
                (document_id, chat_id, role, content, input_tokens, output_tokens,
                 raw_content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for (i, msg) in messages.iter().enumerate() {
            // same message at the same position (token/raw_content updates don't
            // count as a different message) → keep its original timestamp
            let created_at = existing
                .get(i)
                .filter(|(e, _)| e.role == msg.role && e.content == msg.content)
                .map_or(ts.as_str(), |(_, t)| t.as_str());
            // serialize blocks to TEXT; None → SQL NULL. Propagate rather than
            // panic — a panic here would poison the app-wide DB Mutex.
            let raw_content = match msg.raw_content.as_ref() {
                Some(v) => Some(
                    serde_json::to_string(v).map_err(|e| Error::InvalidInput(e.to_string()))?,
                ),
                None => None,
            };
            stmt.execute(rusqlite::params![
                document_id,
                chat_id,
                msg.role,
                msg.content,
                msg.input_tokens,
                msg.output_tokens,
                raw_content,
                created_at
            ])?;
        }
    }
    tx.execute("UPDATE chats SET updated_at = ?2 WHERE id = ?1", rusqlite::params![chat_id, ts])?;
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
        active: row.get(5)?,
        sort_order: row.get(6)?,
    })
}

const FOLDER_COLUMNS: &str = "id, name, parent_id, created_at, updated_at, active, sort_order";

pub fn list_folders(conn: &Connection) -> Result<Vec<Folder>> {
    // Manual order; name is only the tiebreak for the backfilled defaults.
    let mut stmt = conn.prepare(&format!(
        "SELECT {FOLDER_COLUMNS} FROM folders ORDER BY sort_order, name COLLATE NOCASE"
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
        "INSERT INTO folders (id, name, parent_id, created_at, updated_at, sort_order)
         VALUES (?1, ?2, NULL, ?3, ?3,
                 COALESCE((SELECT MAX(sort_order) FROM folders), -1) + 1)",
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

pub fn set_folder_active(conn: &Connection, id: &str, active: bool) -> Result<Folder> {
    let changed = conn.execute(
        "UPDATE folders SET active = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![id, active, now()],
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

/// Apply a manual ordering to folders: each id's `sort_order` becomes its index
/// in `ids`. Mirrors `reorder_documents` — never touches `updated_at`, ignores
/// unknown ids.
pub fn reorder_folders(conn: &Connection, ids: &[String]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    for (i, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE folders SET sort_order = ?2 WHERE id = ?1",
            rusqlite::params![id, i as i64],
        )?;
    }
    tx.commit()?;
    Ok(())
}

// --- Semantic chunk index (v10) --------------------------------------------
// Storage layer for the `chunks` table. These plain fns over `&Connection` are
// consumed by the background embed worker (Slice 2) and the `search_notes`
// chat tool (Slice 3); Slice 1 lands + tests them via `embed::FakeEmbedder`.
// `#[allow(dead_code)]` until those callers exist.

/// A stored chunk with its decoded embedding, plus the parent document's name
/// for citation. Returned by `all_chunk_embeddings` for in-memory cosine rank.
#[allow(dead_code)]
#[derive(Debug)]
pub struct ChunkVec {
    pub id: String,
    pub document_id: String,
    pub doc_name: String,
    pub content: String,
    pub embedding: Vec<f32>,
}

/// Replace all chunks for a document in one transaction: delete the old rows,
/// insert `(ordinal, content, embedding)` in order, and stamp `embedded_at`.
/// Embeddings are stored as the L2-normalized f32 BLOB the caller passes (the
/// worker normalizes; see `embed::normalize`).
#[allow(dead_code)]
pub fn replace_chunks(
    conn: &Connection,
    document_id: &str,
    chunks: &[(usize, &str, &[f32])],
) -> Result<()> {
    let ts = now();
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM chunks WHERE document_id = ?1", [document_id])?;
    for (ordinal, content, embedding) in chunks {
        tx.execute(
            "INSERT INTO chunks (id, document_id, ordinal, content, embedding, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                uuid::Uuid::new_v4().to_string(),
                document_id,
                *ordinal as i64,
                content,
                crate::embed::embedding_to_blob(embedding),
                ts,
            ],
        )?;
    }
    tx.execute(
        "UPDATE documents SET embedded_at = ?2 WHERE id = ?1",
        rusqlite::params![document_id, ts],
    )?;
    tx.commit()?;
    Ok(())
}

/// Drop every chunk for a document (e.g. before a re-embed). Deleting the
/// document itself cascades via the FK, so this is only for the re-index path.
#[allow(dead_code)]
pub fn delete_chunks(conn: &Connection, document_id: &str) -> Result<()> {
    conn.execute("DELETE FROM chunks WHERE document_id = ?1", [document_id])?;
    Ok(())
}

/// Load every chunk with its embedding decoded, joined to its document's name.
/// Brute-force cosine ranking (plan D13) runs over this in memory — fine for a
/// writer's corpus (thousands of chunks = low-ms); swap to ANN only if measured.
#[allow(dead_code)]
pub fn all_chunk_embeddings(conn: &Connection) -> Result<Vec<ChunkVec>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.document_id, d.name, c.content, c.embedding
         FROM chunks c JOIN documents d ON d.id = c.document_id
         ORDER BY c.document_id, c.ordinal",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Vec<u8>>(4)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, document_id, doc_name, content, blob) = row?;
        out.push(ChunkVec {
            id,
            document_id,
            doc_name,
            content,
            embedding: crate::embed::blob_to_embedding(&blob)?,
        });
    }
    Ok(out)
}

/// Document ids that need (re)embedding: never embedded, or edited since. Ideas
/// are excluded (plan D6). The parentheses keep a NULL `embedded_at` from
/// leaking an idea through the OR.
#[allow(dead_code)]
pub fn docs_needing_embedding(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM documents
         WHERE (embedded_at IS NULL OR embedded_at < updated_at) AND type <> 'idea'",
    )?;
    let ids = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ids)
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
    fn fts5_is_available() {
        // The rusqlite "bundled" build must ship FTS5 (SQLITE_ENABLE_FTS5);
        // every search feature below depends on it. If this fails, the fix is a
        // Cargo/build change, not application code.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE VIRTUAL TABLE t USING fts5(x);")
            .expect("rusqlite bundled build must ship FTS5");
    }

    #[test]
    fn search_matches_title_and_body() {
        let conn = test_conn();
        let titled =
            create_document(&conn, "Rust Guide", None, Some("body about widgets")).unwrap();
        let body_only =
            create_document(&conn, "Cooking", None, Some("a recipe mentioning rust on a pan"))
                .unwrap();

        let hits = search_documents(&conn, "rust").unwrap();
        let ids: Vec<&str> = hits.iter().map(|h| h.id.as_str()).collect();
        assert!(ids.contains(&titled.id.as_str()));
        assert!(ids.contains(&body_only.id.as_str()));
        // title match outranks body-only match (bm25 name weight)
        assert_eq!(hits[0].id, titled.id);
    }

    #[test]
    fn search_reflects_edits_and_deletes() {
        let conn = test_conn();
        let doc = create_document(&conn, "Draft", None, Some("nothing here")).unwrap();
        assert!(search_documents(&conn, "kangaroo").unwrap().is_empty());

        save_document_content(&conn, &doc.id, "now mentions kangaroo").unwrap();
        assert_eq!(search_documents(&conn, "kangaroo").unwrap().len(), 1);

        rename_document(&conn, &doc.id, "Kangaroo Notes").unwrap();
        assert!(!search_documents(&conn, "kangaroo").unwrap().is_empty());

        delete_document(&conn, &doc.id).unwrap();
        assert!(search_documents(&conn, "kangaroo").unwrap().is_empty());
    }

    #[test]
    fn search_excludes_ideas() {
        let conn = test_conn();
        let idea =
            create_document(&conn, "Spark", Some(DocType::Idea), Some("xylophone thoughts")).unwrap();
        let doc =
            create_document(&conn, "Real", Some(DocType::BlogPost), Some("xylophone music")).unwrap();

        let hits = search_documents(&conn, "xylophone").unwrap();
        let ids: Vec<&str> = hits.iter().map(|h| h.id.as_str()).collect();
        assert!(ids.contains(&doc.id.as_str()));
        assert!(!ids.contains(&idea.id.as_str()));
    }

    #[test]
    fn search_ignores_fts_operators_safely() {
        let conn = test_conn();
        create_document(&conn, "Plus Test", None, Some("c++ and rust")).unwrap();
        // raw operator / punctuation / empty input must never error
        for q in ["c++", "foo AND bar", "\"unterminated", "NEAR", ""] {
            assert!(search_documents(&conn, q).is_ok(), "query {q:?} errored");
        }
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
    fn title_explicit_defaults_false() {
        let conn = test_conn();
        let doc = create_document(&conn, "New idea", Some(DocType::Idea), None).unwrap();
        assert!(!doc.title_explicit);
    }

    #[test]
    fn rename_sets_title_explicit() {
        let conn = test_conn();
        let doc = create_document(&conn, "New idea", Some(DocType::Idea), None).unwrap();
        let renamed = rename_document(&conn, &doc.id, "My Idea").unwrap();
        assert!(renamed.title_explicit);
    }

    #[test]
    fn update_idea_name_can_set_derived_then_explicit() {
        let conn = test_conn();
        let doc = create_document(&conn, "New idea", Some(DocType::Idea), None).unwrap();

        let derived = update_idea_name(&conn, &doc.id, "First line", false).unwrap();
        assert_eq!(derived.name, "First line");
        assert!(!derived.title_explicit);

        let explicit = update_idea_name(&conn, &doc.id, "Real Title", true).unwrap();
        assert!(explicit.title_explicit);

        // and back to derived
        let back = update_idea_name(&conn, &doc.id, "New first line", false).unwrap();
        assert!(!back.title_explicit);
    }

    #[test]
    fn update_document_type_converts_and_titles() {
        let conn = test_conn();
        let idea = create_document(&conn, "Buy milk", Some(DocType::Idea), Some("body")).unwrap();
        let converted =
            update_document_type(&conn, &idea.id, DocType::BlogPost, "Buy milk", true).unwrap();
        assert_eq!(converted.doc_type, DocType::BlogPost);
        assert_eq!(converted.name, "Buy milk");
        assert!(converted.title_explicit);

        // no longer an idea
        let ideas: Vec<_> = list_documents(&conn)
            .unwrap()
            .into_iter()
            .filter(|d| d.doc_type == DocType::Idea)
            .collect();
        assert!(ideas.is_empty());
    }

    #[test]
    fn update_document_type_missing_id_fails() {
        let conn = test_conn();
        let err =
            update_document_type(&conn, "nope", DocType::BlogPost, "X", true).unwrap_err();
        assert!(matches!(err, Error::NotFound("document")));
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
    fn create_folder_defaults_active() {
        let conn = test_conn();
        let folder = create_folder(&conn, "Posts").unwrap();
        assert!(folder.active);
        assert!(list_folders(&conn).unwrap()[0].active);
    }

    #[test]
    fn set_folder_active_toggles() {
        let conn = test_conn();
        let folder = create_folder(&conn, "Posts").unwrap();

        let rested = set_folder_active(&conn, &folder.id, false).unwrap();
        assert!(!rested.active);
        assert!(!list_folders(&conn).unwrap()[0].active);

        let woken = set_folder_active(&conn, &folder.id, true).unwrap();
        assert!(woken.active);

        assert!(matches!(
            set_folder_active(&conn, "nope", true).unwrap_err(),
            Error::NotFound("folder")
        ));
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

    fn msg(role: &str, content: &str) -> StoredChatMessage {
        StoredChatMessage {
            role: role.into(),
            content: content.into(),
            input_tokens: None,
            output_tokens: None,
            raw_content: None,
        }
    }

    #[test]
    fn chat_messages_roundtrip_and_cascade() {
        let mut conn = test_conn();
        let doc = create_document(&conn, "Draft", None, None).unwrap();
        let chat = create_chat(&conn, &doc.id, None).unwrap();
        assert_eq!(chat.title, DEFAULT_CHAT_TITLE);

        let thread = vec![
            msg("user", "hi"),
            StoredChatMessage {
                role: "assistant".into(),
                content: "hello".into(),
                input_tokens: Some(120),
                output_tokens: Some(8),
                raw_content: None,
            },
        ];
        save_chat_messages(&mut conn, &chat.id, &thread).unwrap();
        let loaded = get_chat_messages(&conn, &chat.id).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].role, "user");
        assert_eq!(loaded[1].content, "hello");
        assert_eq!(loaded[1].output_tokens, Some(8));

        // replace-all semantics (clear)
        save_chat_messages(&mut conn, &chat.id, &[]).unwrap();
        assert!(get_chat_messages(&conn, &chat.id).unwrap().is_empty());

        // deleting the document cascades its chats and their messages
        save_chat_messages(&mut conn, &chat.id, &thread).unwrap();
        delete_document(&conn, &doc.id).unwrap();
        let msgs: i64 = conn
            .query_row("SELECT COUNT(*) FROM chat_messages", [], |r| r.get(0))
            .unwrap();
        let chats: i64 = conn.query_row("SELECT COUNT(*) FROM chats", [], |r| r.get(0)).unwrap();
        assert_eq!(msgs, 0);
        assert_eq!(chats, 0);
    }

    #[test]
    fn resave_identical_thread_is_a_noop_and_appends_keep_timestamps() {
        let mut conn = test_conn();
        let doc = create_document(&conn, "Draft", None, None).unwrap();
        let chat = create_chat(&conn, &doc.id, Some("A")).unwrap();
        let thread = vec![msg("user", "hi"), msg("assistant", "hello")];
        save_chat_messages(&mut conn, &chat.id, &thread).unwrap();

        let stamps = |conn: &Connection| -> Vec<String> {
            let mut stmt = conn
                .prepare("SELECT created_at FROM chat_messages WHERE chat_id = ?1 ORDER BY id")
                .unwrap();
            let v = stmt
                .query_map([&chat.id], |r| r.get(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<String>>>()
                .unwrap();
            v
        };
        let created = stamps(&conn);
        let updated: String = conn
            .query_row("SELECT updated_at FROM chats WHERE id = ?1", [&chat.id], |r| r.get(0))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));

        // re-saving the identical thread (e.g. just opening the chat) must not
        // bump updated_at (sort order) or rewrite created_at
        save_chat_messages(&mut conn, &chat.id, &thread).unwrap();
        let updated_after: String = conn
            .query_row("SELECT updated_at FROM chats WHERE id = ?1", [&chat.id], |r| r.get(0))
            .unwrap();
        assert_eq!(updated, updated_after);
        assert_eq!(created, stamps(&conn));

        // appending a message keeps the existing rows' timestamps and bumps the chat
        let mut longer = thread.clone();
        longer.push(msg("user", "more"));
        save_chat_messages(&mut conn, &chat.id, &longer).unwrap();
        let after_append = stamps(&conn);
        assert_eq!(&after_append[..2], &created[..]);
        let updated_append: String = conn
            .query_row("SELECT updated_at FROM chats WHERE id = ?1", [&chat.id], |r| r.get(0))
            .unwrap();
        assert_ne!(updated, updated_append);
    }

    #[test]
    fn raw_content_roundtrips_and_persists_when_gained() {
        let mut conn = test_conn();
        let doc = create_document(&conn, "Draft", None, None).unwrap();
        let chat = create_chat(&conn, &doc.id, Some("A")).unwrap();

        // a plain thread, then the assistant turn later gains a compaction block
        let plain = vec![msg("user", "hi"), msg("assistant", "hello")];
        save_chat_messages(&mut conn, &chat.id, &plain).unwrap();
        let created: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT created_at FROM chat_messages WHERE chat_id = ?1 ORDER BY id")
                .unwrap();
            stmt.query_map([&chat.id], |r| r.get(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<String>>>()
                .unwrap()
        };
        std::thread::sleep(std::time::Duration::from_millis(5));

        let blocks = serde_json::json!([
            { "type": "compaction", "content": "summary so far" },
            { "type": "text", "text": "hello" },
        ]);
        let mut gained = plain.clone();
        gained[1].raw_content = Some(blocks.clone());

        // same role+content but new raw_content → must NOT short-circuit
        save_chat_messages(&mut conn, &chat.id, &gained).unwrap();
        let loaded = get_chat_messages(&conn, &chat.id).unwrap();
        assert_eq!(loaded[1].raw_content, Some(blocks));
        assert!(loaded[0].raw_content.is_none());

        // created_at preserved (raw_content change isn't a new message)
        let created_after: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT created_at FROM chat_messages WHERE chat_id = ?1 ORDER BY id")
                .unwrap();
            stmt.query_map([&chat.id], |r| r.get(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<String>>>()
                .unwrap()
        };
        assert_eq!(created, created_after);

        // re-saving the now-identical thread (incl. raw_content) is a no-op
        let updated: String = conn
            .query_row("SELECT updated_at FROM chats WHERE id = ?1", [&chat.id], |r| r.get(0))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        save_chat_messages(&mut conn, &chat.id, &gained).unwrap();
        let updated_after: String = conn
            .query_row("SELECT updated_at FROM chats WHERE id = ?1", [&chat.id], |r| r.get(0))
            .unwrap();
        assert_eq!(updated, updated_after);
    }

    #[test]
    fn v9_adds_nullable_raw_content() {
        let conn = Connection::open_in_memory().unwrap();
        // apply migrations up to (but not including) v9
        for (i, m) in MIGRATIONS.iter().take(MIGRATIONS.len() - 1).enumerate() {
            conn.execute_batch(&format!("BEGIN;\n{}\nPRAGMA user_version = {};\nCOMMIT;", m, i + 1))
                .unwrap();
        }
        // a pre-v9 chat message row (document_id NOT NULL since v2)
        conn.execute(
            "INSERT INTO documents (id, name, type, folder_id, content, created_at, updated_at, title_explicit, sort_order)
             VALUES ('d', 'D', 'generic', NULL, '', '2026-01', '2026-01', 0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chat_messages (document_id, role, content, created_at)
             VALUES ('d', 'user', 'hi', '2026-01')",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version as usize, MIGRATIONS.len());
        // existing row backfills to NULL raw_content
        let raw: Option<String> = conn
            .query_row("SELECT raw_content FROM chat_messages", [], |r| r.get(0))
            .unwrap();
        assert!(raw.is_none());
    }

    #[test]
    fn chat_crud_and_ordering() {
        let mut conn = test_conn();
        let doc = create_document(&conn, "Draft", None, None).unwrap();

        let a = create_chat(&conn, &doc.id, Some("First")).unwrap();
        let b = create_chat(&conn, &doc.id, None).unwrap();
        assert_eq!(list_chats(&conn, &doc.id).unwrap().len(), 2);

        // saving messages to `a` bumps its updated_at → sorts to the top
        save_chat_messages(&mut conn, &a.id, &[msg("user", "hi")]).unwrap();
        let chats = list_chats(&conn, &doc.id).unwrap();
        assert_eq!(chats[0].id, a.id);

        let renamed = rename_chat(&conn, &b.id, "  Renamed  ").unwrap();
        assert_eq!(renamed.title, "Renamed");

        delete_chat(&conn, &b.id).unwrap();
        assert_eq!(list_chats(&conn, &doc.id).unwrap().len(), 1);
        assert!(matches!(delete_chat(&conn, "nope").unwrap_err(), Error::NotFound("chat")));
    }

    #[test]
    fn legacy_messages_are_backfilled_into_a_chat() {
        let conn = test_conn();
        let doc = create_document(&conn, "Draft", None, None).unwrap();
        // simulate a pre-v4 thread: messages with no chat_id
        conn.execute(
            "INSERT INTO chat_messages (document_id, role, content, created_at)
             VALUES (?1, 'user', 'old question', ?2)",
            rusqlite::params![doc.id, now()],
        )
        .unwrap();

        let chats = list_chats(&conn, &doc.id).unwrap();
        assert_eq!(chats.len(), 1);
        let msgs = get_chat_messages(&conn, &chats[0].id).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "old question");

        // idempotent — a second list doesn't create another chat
        assert_eq!(list_chats(&conn, &doc.id).unwrap().len(), 1);
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
            DocType::Plan,
            DocType::BuildLog,
            DocType::Idea,
            DocType::Generic,
        ] {
            assert_eq!(DocType::parse(t.as_str()).unwrap(), t);
            let json = serde_json::to_string(&t).unwrap();
            assert_eq!(json, format!("\"{}\"", t.as_str()));
        }
    }

    /// sort_order of a document by id (via the public list).
    fn doc_order(conn: &Connection, id: &str) -> i64 {
        list_documents(conn).unwrap().into_iter().find(|d| d.id == id).unwrap().sort_order
    }

    #[test]
    fn reorder_documents_assigns_positions() {
        let conn = test_conn();
        let a = create_document(&conn, "A", None, None).unwrap();
        let b = create_document(&conn, "B", None, None).unwrap();
        let c = create_document(&conn, "C", None, None).unwrap();

        reorder_documents(&conn, &[c.id.clone(), a.id.clone(), b.id.clone()]).unwrap();
        assert_eq!(doc_order(&conn, &c.id), 0);
        assert_eq!(doc_order(&conn, &a.id), 1);
        assert_eq!(doc_order(&conn, &b.id), 2);
    }

    #[test]
    fn reorder_ignores_unknown_ids() {
        let conn = test_conn();
        let a = create_document(&conn, "A", None, None).unwrap();
        // a stale/unknown id (e.g. deleted mid-drag) must not error; the known
        // id still lands at its index
        reorder_documents(&conn, &["ghost".into(), a.id.clone()]).unwrap();
        assert_eq!(doc_order(&conn, &a.id), 1);
    }

    #[test]
    fn reorder_does_not_touch_updated_at() {
        let conn = test_conn();
        let a = create_document(&conn, "A", None, None).unwrap();
        let b = create_document(&conn, "B", None, None).unwrap();
        let stamp = |id: &str| {
            list_documents(&conn).unwrap().into_iter().find(|d| d.id == id).unwrap().updated_at
        };
        let (ua, ub) = (stamp(&a.id), stamp(&b.id));

        reorder_documents(&conn, &[b.id.clone(), a.id.clone()]).unwrap();
        assert_eq!(stamp(&a.id), ua);
        assert_eq!(stamp(&b.id), ub);
    }

    #[test]
    fn create_document_lands_at_top() {
        let conn = test_conn();
        let a = create_document(&conn, "A", None, None).unwrap();
        let b = create_document(&conn, "B", None, None).unwrap();
        // normalize to 0..n so "above the top" is unambiguous
        reorder_documents(&conn, &[a.id.clone(), b.id.clone()]).unwrap();
        let c = create_document(&conn, "C", None, None).unwrap();
        assert!(doc_order(&conn, &c.id) < doc_order(&conn, &a.id));
    }

    #[test]
    fn move_document_lands_at_top() {
        let conn = test_conn();
        let folder = create_folder(&conn, "F").unwrap();
        let a = create_document(&conn, "A", None, None).unwrap();
        let b = create_document(&conn, "B", None, None).unwrap();
        reorder_documents(&conn, &[a.id.clone(), b.id.clone()]).unwrap(); // a=0, b=1

        move_document(&conn, &b.id, Some(&folder.id)).unwrap();
        assert!(doc_order(&conn, &b.id) < doc_order(&conn, &a.id));
    }

    #[test]
    fn list_folders_uses_manual_order() {
        let conn = test_conn();
        let a = create_folder(&conn, "Alpha").unwrap();
        let b = create_folder(&conn, "Beta").unwrap();
        let c = create_folder(&conn, "Gamma").unwrap();
        // new folders append → creation order
        let names = |conn: &Connection| -> Vec<String> {
            list_folders(conn).unwrap().into_iter().map(|f| f.name).collect()
        };
        assert_eq!(names(&conn), vec!["Alpha", "Beta", "Gamma"]);

        reorder_folders(&conn, &[c.id.clone(), a.id.clone(), b.id.clone()]).unwrap();
        assert_eq!(names(&conn), vec!["Gamma", "Alpha", "Beta"]);

        // a freshly created folder still appends to the end
        create_folder(&conn, "Delta").unwrap();
        assert_eq!(names(&conn), vec!["Gamma", "Alpha", "Beta", "Delta"]);
    }

    #[test]
    fn v8_backfill_ranks_existing_rows() {
        let conn = Connection::open_in_memory().unwrap();
        // apply migrations up to (but not including) v8
        for (i, m) in MIGRATIONS.iter().take(7).enumerate() {
            conn.execute_batch(&format!("BEGIN;\n{}\nPRAGMA user_version = {};\nCOMMIT;", m, i + 1))
                .unwrap();
        }
        // raw rows with controlled order keys (title_explicit exists since v5)
        conn.execute(
            "INSERT INTO documents (id, name, type, folder_id, content, created_at, updated_at, title_explicit)
             VALUES ('old', 'Old', 'generic', NULL, '', '2026-01', '2026-01', 0),
                    ('new', 'New', 'generic', NULL, '', '2026-03', '2026-03', 0),
                    ('mid', 'Mid', 'generic', NULL, '', '2026-02', '2026-02', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO folders (id, name, parent_id, created_at, updated_at, active)
             VALUES ('fb', 'Beta', NULL, '2026-01', '2026-01', 1),
                    ('fa', 'alpha', NULL, '2026-01', '2026-01', 1)",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version as usize, MIGRATIONS.len());

        // documents ranked newest-first → backfilled order matches list order
        let docs = list_documents(&conn).unwrap();
        assert_eq!(docs.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), vec!["new", "mid", "old"]);
        assert_eq!(docs.iter().find(|d| d.id == "new").unwrap().sort_order, 0);
        assert_eq!(docs.iter().find(|d| d.id == "old").unwrap().sort_order, 2);

        // folders ranked by NOCASE name → alpha before Beta
        let folders = list_folders(&conn).unwrap();
        assert_eq!(folders.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), vec!["alpha", "Beta"]);
        assert_eq!(folders.iter().find(|f| f.id == "fa").unwrap().sort_order, 0);
        assert_eq!(folders.iter().find(|f| f.id == "fb").unwrap().sort_order, 1);
    }

    /// Embed two documents with the deterministic `FakeEmbedder`, store their
    /// chunks, then rank the whole index against a query by cosine — the doc
    /// that shares the query's vocabulary must come out on top. Exercises the
    /// full chunk → embed → replace_chunks → all_chunk_embeddings round-trip
    /// (BLOB encode/decode included) with no model download.
    #[test]
    fn semantic_ranking_end_to_end() {
        use crate::embed::{self, Embedder, FakeEmbedder};
        let conn = test_conn();
        let embedder = FakeEmbedder;

        let rust_doc = create_document(
            &conn,
            "Rust Notes",
            None,
            Some("# Borrow checker\nRust ownership lifetimes borrow checker move semantics."),
        )
        .unwrap();
        let bread_doc = create_document(
            &conn,
            "Baking Notes",
            None,
            Some("# Sourdough\nBread flour water starter fermentation crumb crust."),
        )
        .unwrap();

        for doc in [&rust_doc, &bread_doc] {
            let content = get_document_content(&conn, &doc.id).unwrap();
            let texts = embed::chunk_document(&content);
            assert!(!texts.is_empty());
            let vecs = embedder.embed_passages(&texts).unwrap();
            let rows: Vec<(usize, &str, &[f32])> = texts
                .iter()
                .zip(&vecs)
                .enumerate()
                .map(|(i, (t, v))| (i, t.as_str(), v.as_slice()))
                .collect();
            replace_chunks(&conn, &doc.id, &rows).unwrap();
        }

        // embedded_at got stamped; both docs leave the "needs embedding" set.
        assert!(docs_needing_embedding(&conn).unwrap().is_empty());

        // Rank the corpus against a Rust query.
        let q = embedder
            .embed_query("how does the borrow checker enforce ownership")
            .unwrap();
        let mut ranked = all_chunk_embeddings(&conn).unwrap();
        assert_eq!(ranked.len(), 2);
        ranked.sort_by(|a, b| {
            embed::cosine(&q, &b.embedding)
                .partial_cmp(&embed::cosine(&q, &a.embedding))
                .unwrap()
        });
        assert_eq!(ranked[0].doc_name, "Rust Notes");

        // Deleting a document cascades its chunks away (real FK, unlike fts).
        delete_document(&conn, &rust_doc.id).unwrap();
        let remaining = all_chunk_embeddings(&conn).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].doc_name, "Baking Notes");
    }

    /// Ideas are never embedded (plan D6): the parenthesized predicate must not
    /// leak a NULL-`embedded_at` idea into the worker's work-list.
    #[test]
    fn docs_needing_embedding_excludes_ideas() {
        let conn = test_conn();
        create_document(&conn, "Real Doc", Some(DocType::Generic), Some("body")).unwrap();
        create_document(&conn, "An Idea", Some(DocType::Idea), Some("thought")).unwrap();

        let pending = docs_needing_embedding(&conn).unwrap();
        assert_eq!(pending.len(), 1, "only the non-idea doc is pending: {pending:?}");
    }
}
