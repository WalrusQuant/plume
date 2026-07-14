//! Semantic-index primitives: document chunking, the `Embedder` abstraction,
//! and f32-vector ⇄ BLOB (de)serialization for the `chunks` table.
//!
//! Slice 1 is deliberately model-free — everything here compiles and is
//! unit-tested without downloading a model or linking onnxruntime. The real
//! fastembed-backed `Embedder` arrives in Slice 2; tests use `FakeEmbedder`.
//!
//! Chunking reuses the app's single comrak parse (`preview::options()`) — same
//! engine as the preview, AI context, and every exporter (never a second one).
//!
//! Slice 1 lands the primitives; the worker (Slice 2) and chat tool (Slice 3)
//! wire them up. Until then several are only exercised by tests, so dead_code
//! is allowed module-wide.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use comrak::nodes::{AstNode, NodeValue};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use rusqlite::Connection;
use tauri::{AppHandle, Emitter};

use crate::error::{Error, Result};

/// Embedding dimensionality (bge-small-en-v1.5). One vector = 384 × f32.
pub const EMBED_DIM: usize = 384;
/// A serialized embedding is `EMBED_DIM` little-endian f32s = 1536 bytes.
pub const EMBED_BYTES: usize = EMBED_DIM * 4;

/// Rough target words per chunk. We only split at block boundaries, so an
/// oversized single block (a very long paragraph or code block) stays whole —
/// a chunk may exceed this, but a code block is never split.
const CHUNK_TARGET_WORDS: usize = 280;

/// Produces vector embeddings. The real impl (Slice 2) wraps fastembed; tests
/// use a deterministic fake. `embed_query` and `embed_passages` are split
/// because bge wants an instruction prefix on the query only (see plan D2).
pub trait Embedder: Send + Sync {
    /// Embed document chunks (no query prefix). One vector per input, in order.
    fn embed_passages(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    /// Embed a single search query (with the bge retrieval prefix, in the real impl).
    fn embed_query(&self, query: &str) -> Result<Vec<f32>>;
}

/// Split a document into ordered chunks of plain text, ~`CHUNK_TARGET_WORDS`
/// each, breaking only at top-level block boundaries. Headings start a new
/// chunk (section boundary); a code block is emitted whole and never split.
/// Pure — no I/O. Returns `[]` for empty/whitespace-only input.
pub fn chunk_document(content: &str) -> Vec<String> {
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, content, &crate::preview::options());

    let mut chunks: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut buf_words = 0usize;

    for block in root.children() {
        let is_heading = matches!(block.data.borrow().value, NodeValue::Heading(_));
        // A heading opens a new section: flush what we have, then start fresh
        // with the heading as this chunk's leading context.
        if is_heading && !buf.is_empty() {
            push_chunk(&mut buf, &mut buf_words, &mut chunks);
        }

        let text = block_text(block);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        if !buf.is_empty() {
            buf.push_str("\n\n");
        }
        buf.push_str(text);
        buf_words += text.split_whitespace().count();

        // Don't flush right after a heading — keep it attached to the body that
        // follows so a lone heading never becomes its own chunk.
        if buf_words >= CHUNK_TARGET_WORDS && !is_heading {
            push_chunk(&mut buf, &mut buf_words, &mut chunks);
        }
    }
    push_chunk(&mut buf, &mut buf_words, &mut chunks);
    chunks
}

/// Flush the accumulator into `chunks` if it holds non-whitespace text.
fn push_chunk(buf: &mut String, buf_words: &mut usize, chunks: &mut Vec<String>) {
    let trimmed = buf.trim();
    if !trimmed.is_empty() {
        chunks.push(trimmed.to_string());
    }
    buf.clear();
    *buf_words = 0;
}

/// Plain text of a top-level block, code blocks included verbatim. This is what
/// we embed — retrieval wants the words, not the markdown punctuation.
fn block_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut out = String::new();
    collect_text(node, &mut out);
    out
}

fn collect_text<'a>(node: &'a AstNode<'a>, out: &mut String) {
    match &node.data.borrow().value {
        NodeValue::Text(t) => out.push_str(t),
        NodeValue::Code(c) => out.push_str(&c.literal),
        NodeValue::CodeBlock(cb) => out.push_str(&cb.literal),
        NodeValue::HtmlBlock(h) => out.push_str(&h.literal),
        NodeValue::HtmlInline(h) => out.push_str(h),
        NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
        _ => {}
    }
    for child in node.children() {
        collect_text(child, out);
    }
}

/// Serialize an embedding to its little-endian f32 BLOB (`EMBED_BYTES` long).
pub fn embedding_to_blob(v: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(v.len() * 4);
    for f in v {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    bytes
}

/// Deserialize a `chunks.embedding` BLOB back to f32s. The dimension is no
/// longer fixed (the active model may be 384/768/1024-dim), so we require only a
/// non-empty, 4-byte-aligned blob — a length that isn't a whole number of f32s
/// means a corrupt row. Cross-model comparison is prevented upstream: switching
/// models wipes the index, and `run_semantic_search` skips any chunk whose dim
/// doesn't match the query.
pub fn blob_to_embedding(blob: &[u8]) -> Result<Vec<f32>> {
    if blob.is_empty() || blob.len() % 4 != 0 {
        return Err(Error::InvalidInput(format!(
            "embedding blob is {} bytes, not a whole number of f32s",
            blob.len()
        )));
    }
    Ok(blob
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

/// L2-normalize in place. Normalizing on write (plan D5) lets query-time cosine
/// be a plain dot product. A zero vector is left as-is.
pub fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Cosine similarity. Equals the dot product when both inputs are L2-normalized;
/// we divide by the norms anyway so an un-normalized vector can't skew ranking.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

/// One selectable embedding model. Each has its own dimension and retrieval
/// prefixes, so switching between them is not interchangeable — vectors from
/// different models live in different spaces and can't be compared. The chunk
/// index is therefore always single-model; a switch wipes and re-embeds it
/// (`storage::clear_index_for_reembed`).
pub struct ModelChoice {
    /// Stable id persisted in `app_settings` and sent over IPC.
    pub id: &'static str,
    /// Human label for the Settings dropdown.
    pub label: &'static str,
    /// The fastembed variant to load.
    pub model: EmbeddingModel,
    /// Output dimensionality (informational; the model dictates it at runtime).
    pub dim: usize,
    /// fastembed's `model_code` — also the hf-hub cache folder stem, so the
    /// on-disk dir is `{cache_dir}/models--{repo with '/' → '--'}`.
    pub repo: &'static str,
    /// Prefix prepended to a search query before embedding (model-specific:
    /// bge uses a retrieval instruction, e5 uses "query: ", MiniLM none).
    pub query_prefix: &'static str,
    /// Prefix prepended to document chunks (e5 uses "passage: "; others none).
    pub passage_prefix: &'static str,
    /// Approximate download size, shown before the user commits to it.
    pub size_label: &'static str,
    /// One-line guidance for the dropdown.
    pub note: &'static str,
}

/// The curated, vetted set offered in Settings → Local search. All are fp32
/// (non-quantized) — the int8 variants failed at inference under the bundled
/// onnxruntime (see the bge-small note in `with_model`). Order is display order.
pub static CURATED_MODELS: &[ModelChoice] = &[
    ModelChoice {
        id: "minilm-l6",
        label: "MiniLM-L6 — fastest",
        model: EmbeddingModel::AllMiniLML6V2,
        dim: 384,
        repo: "Qdrant/all-MiniLM-L6-v2-onnx",
        query_prefix: "",
        passage_prefix: "",
        size_label: "~90 MB",
        note: "Smallest and fastest. Good English search on a light footprint.",
    },
    ModelChoice {
        id: "bge-small",
        label: "BGE-small — recommended",
        model: EmbeddingModel::BGESmallENV15,
        dim: 384,
        repo: "Xenova/bge-small-en-v1.5",
        query_prefix: "Represent this sentence for searching relevant passages: ",
        passage_prefix: "",
        size_label: "~130 MB",
        note: "Balanced quality and size. The default.",
    },
    ModelChoice {
        id: "bge-base",
        label: "BGE-base — higher quality",
        model: EmbeddingModel::BGEBaseENV15,
        dim: 768,
        repo: "Xenova/bge-base-en-v1.5",
        query_prefix: "Represent this sentence for searching relevant passages: ",
        passage_prefix: "",
        size_label: "~440 MB",
        note: "Stronger retrieval for a larger download.",
    },
    ModelChoice {
        id: "e5-multilingual",
        label: "E5-small — multilingual",
        model: EmbeddingModel::MultilingualE5Small,
        dim: 384,
        repo: "intfloat/multilingual-e5-small",
        query_prefix: "query: ",
        passage_prefix: "passage: ",
        size_label: "~470 MB",
        note: "For notes in languages other than English.",
    },
];

/// The default model id when nothing is persisted — matches the shipped index,
/// so existing users' 384-dim chunks stay valid with no forced re-embed.
pub const DEFAULT_MODEL_ID: &str = "bge-small";

/// Look up a curated model by id.
pub fn find_model(id: &str) -> Option<&'static ModelChoice> {
    CURATED_MODELS.iter().find(|m| m.id == id)
}

/// The default model (guaranteed present in the catalog).
pub fn default_model() -> &'static ModelChoice {
    find_model(DEFAULT_MODEL_ID).expect("default model id must be in CURATED_MODELS")
}

/// A curated model as shown in the Settings dropdown, with its per-machine
/// installed flag resolved.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedModelInfo {
    pub id: String,
    pub label: String,
    pub dim: usize,
    pub size_label: String,
    pub note: String,
    pub installed: bool,
}

/// On-disk state of the local embedding model, surfaced to the Settings UI.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    /// Id of the currently-active model (from the curated catalog).
    pub active_model_id: String,
    /// True once the active model's files are present and loadable.
    pub installed: bool,
    /// Absolute cache folder for the active model, installed or not.
    pub path: String,
    /// Bytes on disk under `path` (0 when not installed).
    pub size_bytes: u64,
}

/// Recursively look for any `.onnx` file under `dir` (depth-limited to the
/// hf-hub `snapshots/<hash>/` layout). Presence of the model weights is our
/// "installed" signal — a bare cache folder or aborted download has none.
fn contains_onnx(dir: &Path) -> bool {
    fn walk(dir: &Path, depth: usize) -> bool {
        if depth == 0 {
            return false;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if walk(&path, depth - 1) {
                    return true;
                }
            } else if path.extension().is_some_and(|e| e == "onnx") {
                return true;
            }
        }
        false
    }
    walk(dir, 5)
}

/// Total bytes of regular files under `dir` (follows the hf-hub tree, including
/// the deduped `blobs/`). Best-effort — unreadable entries are skipped.
fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.metadata() {
                Ok(m) if m.is_dir() => total += dir_size(&path),
                Ok(m) => total += m.len(),
                Err(_) => {}
            }
        }
    }
    total
}

/// The real fastembed-backed embedder. The active model is chosen in Settings
/// (`choice`), loaded lazily, but only ever downloaded by an explicit
/// `ensure_loaded` (the "Download model" button) — the background worker refuses
/// to index until `is_installed` is true, so startup never blocks and nothing is
/// fetched behind the user's back. One shared session behind a `Mutex` (plan D3);
/// every call runs on a blocking thread, never the async reactor.
pub struct FastEmbedder {
    cache_dir: PathBuf,
    /// The active model. Swapping it (Settings dropdown) drops the loaded
    /// session so the next call loads the newly-selected one.
    choice: Mutex<&'static ModelChoice>,
    model: Mutex<Option<TextEmbedding>>,
}

impl FastEmbedder {
    pub fn new(cache_dir: PathBuf, choice: &'static ModelChoice) -> Self {
        Self {
            cache_dir,
            choice: Mutex::new(choice),
            model: Mutex::new(None),
        }
    }

    /// The currently-selected model. `&'static` so the guard is dropped
    /// immediately (references are `Copy`), never held across the model lock.
    pub fn current_choice(&self) -> &'static ModelChoice {
        *self.choice.lock().unwrap()
    }

    /// Switch the active model, dropping any loaded session. The caller is
    /// responsible for wiping + re-indexing (vectors aren't cross-comparable).
    pub fn set_choice(&self, choice: &'static ModelChoice) {
        *self.choice.lock().unwrap() = choice;
        *self.model.lock().unwrap() = None;
    }

    /// True once the model has been loaded — lets the worker emit a one-time
    /// "downloading" status before the first (potentially slow) load.
    pub fn is_loaded(&self) -> bool {
        self.model.lock().unwrap().is_some()
    }

    /// hf-hub cache folder for the active model, whether or not it's downloaded.
    pub fn model_dir(&self) -> PathBuf {
        self.model_dir_for(self.current_choice())
    }

    fn model_dir_for(&self, choice: &ModelChoice) -> PathBuf {
        self.cache_dir
            .join(format!("models--{}", choice.repo.replace('/', "--")))
    }

    /// True when the active model is present on disk — i.e. an `.onnx` file
    /// exists under its cache folder. A bare/partial folder counts as not
    /// installed so a half-finished download doesn't masquerade as ready.
    pub fn is_installed(&self) -> bool {
        contains_onnx(&self.model_dir())
    }

    /// The whole curated catalog with each model's installed flag resolved,
    /// for the Settings dropdown.
    pub fn list_models(&self) -> Vec<EmbedModelInfo> {
        CURATED_MODELS
            .iter()
            .map(|m| EmbedModelInfo {
                id: m.id.to_string(),
                label: m.label.to_string(),
                dim: m.dim,
                size_label: m.size_label.to_string(),
                note: m.note.to_string(),
                installed: contains_onnx(&self.model_dir_for(m)),
            })
            .collect()
    }

    /// On-disk status of the active model for the Settings UI.
    pub fn status(&self) -> ModelStatus {
        let choice = self.current_choice();
        let dir = self.model_dir_for(choice);
        let installed = contains_onnx(&dir);
        ModelStatus {
            active_model_id: choice.id.to_string(),
            installed,
            path: dir.to_string_lossy().into_owned(),
            size_bytes: if installed { dir_size(&dir) } else { 0 },
        }
    }

    /// Force the lazy download + load of the active model. This is the ONLY path
    /// that fetches a model — the background worker never downloads (it waits for
    /// this). Blocks on the network; callers run it off the async reactor.
    pub fn ensure_loaded(&self) -> Result<()> {
        self.with_model(|_| Ok(()))
    }

    /// Drop the in-memory session and delete the active model's cache folder,
    /// then report the resulting status. The model lock is held across the delete
    /// so a concurrent worker pass can't re-download into the folder mid-removal.
    /// Stored chunks are left untouched — search keeps working on already-indexed
    /// docs; only new/edited docs stall until the model is downloaded again.
    pub fn remove_files(&self) -> Result<ModelStatus> {
        let mut guard = self.model.lock().unwrap();
        *guard = None;
        let dir = self.model_dir();
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        drop(guard);
        Ok(self.status())
    }

    /// Run `f` with the lazily-initialized active model. The first call
    /// downloads + loads it (cache dir pinned to app data dir — plan D1, the
    /// crate default is cwd-relative and wrong for a bundled `.app`); later calls
    /// reuse it. All curated models are fp32: the int8 (`…Q`) variants fail at
    /// inference under the bundled onnxruntime — a `SkipLayerNormalization` op is
    /// missing a LayerNorm weight input.
    fn with_model<T>(&self, f: impl FnOnce(&TextEmbedding) -> Result<T>) -> Result<T> {
        let choice = self.current_choice();
        let mut guard = self.model.lock().unwrap();
        if guard.is_none() {
            let opts = InitOptions::new(choice.model.clone()).with_cache_dir(self.cache_dir.clone());
            let model = TextEmbedding::try_new(opts)
                .map_err(|e| Error::Embed(format!("load embedding model: {e}")))?;
            *guard = Some(model);
        }
        f(guard.as_ref().expect("model just initialized"))
    }
}

impl Embedder for FastEmbedder {
    fn embed_passages(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let prefix = self.current_choice().passage_prefix;
        let prefixed: Vec<String> = if prefix.is_empty() {
            texts.to_vec()
        } else {
            texts.iter().map(|t| format!("{prefix}{t}")).collect()
        };
        self.with_model(|model| {
            let mut out = model
                .embed(prefixed, None)
                .map_err(|e| Error::Embed(format!("embed passages: {e}")))?;
            for v in &mut out {
                normalize(v);
            }
            Ok(out)
        })
    }

    fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        let prefix = self.current_choice().query_prefix;
        self.with_model(|model| {
            let mut out = model
                .embed(vec![format!("{prefix}{query}")], None)
                .map_err(|e| Error::Embed(format!("embed query: {e}")))?;
            let mut v = out
                .pop()
                .ok_or_else(|| Error::Embed("empty query embedding".into()))?;
            normalize(&mut v);
            Ok(v)
        })
    }
}

/// Shared state for the embed pipeline: `tx` nudges the worker after a write
/// (managed like `Db`/`AiState`); `embedder` is the one shared model instance,
/// reused by the chat query path in Slice 3 (hence `Arc`).
pub struct EmbedState {
    pub tx: tokio::sync::mpsc::Sender<()>,
    pub embedder: Arc<FastEmbedder>,
}

const EVT_EMBED_STATUS: &str = "embed:status";

/// Open the worker's own DB connection (plan D4): a second reader/writer on the
/// WAL DB with a busy timeout, so a long backfill never touches the app's `Db`
/// mutex and can't freeze the UI. Foreign keys on for the same cascade rules.
fn open_worker_conn(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA busy_timeout = 5000; PRAGMA foreign_keys = ON;")?;
    Ok(conn)
}

/// Background indexing loop. Runs one backfill at startup, then re-runs whenever
/// a write signals (debounced to coalesce a burst). Fails soft: a bad pass logs
/// and leaves docs dirty for the next signal, never panics the task.
pub async fn run_worker(
    app: AppHandle,
    db_path: PathBuf,
    embedder: Arc<FastEmbedder>,
    mut rx: tokio::sync::mpsc::Receiver<()>,
) {
    loop {
        if let Err(e) = process_pending(&app, &db_path, &embedder).await {
            eprintln!("[embed] indexing pass failed (will retry): {e}");
        }
        // Block until the next write nudge; None = channel closed (shutdown).
        if rx.recv().await.is_none() {
            break;
        }
        // Coalesce a burst of edits into a single pass.
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        while rx.try_recv().is_ok() {}
    }
}

/// One indexing pass: embed every dirty document. Each doc is chunked, embedded
/// on a blocking thread (CPU-bound — plan D3), and stored in its own tx.
async fn process_pending(app: &AppHandle, db_path: &Path, embedder: &Arc<FastEmbedder>) -> Result<()> {
    let conn = open_worker_conn(db_path)?;
    let ids = crate::storage::docs_needing_embedding(&conn)?;
    if ids.is_empty() {
        return Ok(());
    }
    // The worker never downloads: if the user hasn't installed the model from
    // Settings yet, leave the docs dirty and signal the UI. They'll be indexed
    // on the next pass once the model is present (download nudges the worker).
    if !embedder.is_installed() {
        let _ = app.emit(EVT_EMBED_STATUS, "needs-model");
        return Ok(());
    }
    if !embedder.is_loaded() {
        let _ = app.emit(EVT_EMBED_STATUS, "downloading");
    }
    for id in ids {
        // Read content + its version together; the doc may have been deleted
        // between the dirty-list query and now — skip it.
        let Ok((content, updated_at)) = crate::storage::content_for_embedding(&conn, &id) else {
            continue;
        };
        let texts = chunk_document(&content);
        if texts.is_empty() {
            // Nothing to embed (empty doc). Clear stale chunks and stamp the
            // version so it isn't rescanned every pass.
            crate::storage::replace_chunks(&conn, &id, &[], &updated_at)?;
            continue;
        }
        let emb = embedder.clone();
        let owned = texts.clone();
        let vecs = tokio::task::spawn_blocking(move || emb.embed_passages(&owned))
            .await
            .map_err(|e| Error::Embed(format!("embed task join: {e}")))??;
        let rows: Vec<(usize, &str, &[f32])> = texts
            .iter()
            .zip(&vecs)
            .enumerate()
            .map(|(i, (t, v))| (i, t.as_str(), v.as_slice()))
            .collect();
        // Stamp embedded_at to the version we embedded, not now() — an edit that
        // landed during embedding will have a newer updated_at and re-dirty the doc.
        crate::storage::replace_chunks(&conn, &id, &rows, &updated_at)?;
    }
    let _ = app.emit(EVT_EMBED_STATUS, "ready");
    Ok(())
}

/// Deterministic, model-free `Embedder` for tests: a normalized bag-of-words
/// vector (FNV-hashed word → dimension). Lexical overlap → higher cosine, which
/// is enough to prove the chunk → store → rank pipeline without a real model.
#[cfg(test)]
pub struct FakeEmbedder;

#[cfg(test)]
impl FakeEmbedder {
    fn embed_one(text: &str) -> Vec<f32> {
        let mut v = vec![0.0f32; EMBED_DIM];
        for word in text.split_whitespace() {
            let mut h: u32 = 2166136261;
            for b in word.to_lowercase().bytes() {
                h = h.wrapping_mul(16777619) ^ b as u32;
            }
            v[h as usize % EMBED_DIM] += 1.0;
        }
        normalize(&mut v);
        v
    }
}

#[cfg(test)]
impl Embedder for FakeEmbedder {
    fn embed_passages(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| Self::embed_one(t)).collect())
    }
    fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        Ok(Self::embed_one(query))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_split_at_headings_and_keep_code_intact() {
        let md = "\
# Alpha
Some prose about widgets and gears.

# Beta
```rust
fn keep_me_whole() { let x = 1; }
```
More prose here about beta topics.
";
        let chunks = chunk_document(md);
        // Two headings → at least two chunks.
        assert!(chunks.len() >= 2, "expected multiple chunks, got {chunks:?}");
        // The code block survives verbatim inside a single chunk.
        assert!(
            chunks.iter().any(|c| c.contains("fn keep_me_whole() { let x = 1; }")),
            "code block must stay intact in one chunk: {chunks:?}"
        );
        // Heading text is captured (context for the section body).
        assert!(chunks[0].contains("Alpha"));
    }

    #[test]
    fn empty_document_yields_no_chunks() {
        assert!(chunk_document("").is_empty());
        assert!(chunk_document("   \n\n  \t").is_empty());
    }

    #[test]
    fn blob_roundtrips_at_any_dimension_and_rejects_misaligned() {
        // Round-trips whatever dimension the active model produced (here 768).
        let mut v: Vec<f32> = (0..768).map(|i| i as f32 * 0.001 - 0.19).collect();
        normalize(&mut v);
        let blob = embedding_to_blob(&v);
        assert_eq!(blob.len(), 768 * 4);
        assert_eq!(blob_to_embedding(&blob).unwrap(), v);
        // A blob that isn't a whole number of f32s is corrupt → rejected.
        assert!(blob_to_embedding(&blob[..blob.len() - 1]).is_err());
        // An empty blob is rejected too.
        assert!(blob_to_embedding(&[]).is_err());
    }

    /// Fresh, isolated temp dir for a test (no `tempfile` dev-dep). Cleaned up
    /// on entry so a prior crashed run can't leak state into this one.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("plume-embed-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Populate `cache_dir` with a fake hf-hub model folder holding a dummy
    /// `.onnx`, mimicking a completed download.
    fn seed_model(cache_dir: &Path, folder: &str, onnx_bytes: usize) {
        let snap = cache_dir.join(folder).join("snapshots").join("deadbeef");
        std::fs::create_dir_all(&snap).unwrap();
        std::fs::write(snap.join("model.onnx"), vec![0u8; onnx_bytes]).unwrap();
    }

    #[test]
    fn model_dir_follows_hf_hub_naming() {
        let e = FastEmbedder::new(PathBuf::from("/cache"), default_model());
        assert_eq!(
            e.model_dir(),
            PathBuf::from("/cache/models--Xenova--bge-small-en-v1.5")
        );
    }

    #[test]
    fn default_model_and_dim_are_bge_small() {
        assert_eq!(default_model().id, "bge-small");
        assert_eq!(default_model().dim, 384);
        assert!(find_model("nope").is_none());
    }

    #[test]
    fn bare_folder_is_not_installed() {
        let cache = scratch("bare");
        // A folder with no .onnx (aborted/partial download) must read as absent.
        let e = FastEmbedder::new(cache.clone(), default_model());
        std::fs::create_dir_all(e.model_dir().join("blobs")).unwrap();
        assert!(!e.is_installed());
        assert!(!e.status().installed);
        std::fs::remove_dir_all(&cache).ok();
    }

    #[test]
    fn remove_clears_only_the_active_model() {
        let cache = scratch("lifecycle");
        seed_model(&cache, "models--Xenova--bge-small-en-v1.5", 2048); // active (default)
        seed_model(&cache, "models--Qdrant--all-MiniLM-L6-v2-onnx", 1024); // a different model
        std::fs::create_dir_all(cache.join("keep-me")).unwrap();

        let e = FastEmbedder::new(cache.clone(), default_model());
        let before = e.status();
        assert_eq!(before.active_model_id, "bge-small");
        assert!(before.installed);
        assert!(before.size_bytes >= 2048, "size {} too small", before.size_bytes);

        let after = e.remove_files().unwrap();
        assert!(!after.installed);
        assert_eq!(after.size_bytes, 0);
        assert!(!cache.join("models--Xenova--bge-small-en-v1.5").exists());
        // Other installed models and unrelated entries are untouched.
        assert!(cache.join("models--Qdrant--all-MiniLM-L6-v2-onnx").exists());
        assert!(cache.join("keep-me").exists());
        std::fs::remove_dir_all(&cache).ok();
    }

    #[test]
    fn switching_model_changes_dir_and_installed_view() {
        let cache = scratch("switch");
        // Only MiniLM is on disk.
        seed_model(&cache, "models--Qdrant--all-MiniLM-L6-v2-onnx", 512);
        let e = FastEmbedder::new(cache.clone(), default_model());
        assert!(!e.is_installed(), "bge-small isn't installed");

        e.set_choice(find_model("minilm-l6").unwrap());
        assert_eq!(e.current_choice().id, "minilm-l6");
        assert!(e.is_installed(), "minilm dir exists → installed after switch");
        assert!(e.model_dir().ends_with("models--Qdrant--all-MiniLM-L6-v2-onnx"));

        // list_models reflects per-model install state regardless of active one.
        let list = e.list_models();
        let minilm = list.iter().find(|m| m.id == "minilm-l6").unwrap();
        let bge = list.iter().find(|m| m.id == "bge-small").unwrap();
        assert!(minilm.installed && !bge.installed);
        std::fs::remove_dir_all(&cache).ok();
    }

    #[test]
    fn fake_embedder_ranks_lexical_overlap_higher() {
        let e = FakeEmbedder;
        let docs = vec![
            "rust ownership borrow checker lifetimes".to_string(),
            "sourdough bread baking flour water".to_string(),
        ];
        let vecs = e.embed_passages(&docs).unwrap();
        let q = e.embed_query("how does the borrow checker work in rust").unwrap();
        let sim_rust = cosine(&q, &vecs[0]);
        let sim_bread = cosine(&q, &vecs[1]);
        assert!(
            sim_rust > sim_bread,
            "rust doc should rank above bread doc ({sim_rust} vs {sim_bread})"
        );
    }
}
