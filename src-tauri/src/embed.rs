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

/// Deserialize a `chunks.embedding` BLOB back to f32s, asserting the exact
/// `EMBED_BYTES` length — a wrong-sized blob means a corrupt/foreign row.
pub fn blob_to_embedding(blob: &[u8]) -> Result<Vec<f32>> {
    if blob.len() != EMBED_BYTES {
        return Err(Error::InvalidInput(format!(
            "embedding blob is {} bytes, expected {EMBED_BYTES}",
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

/// bge wants a retrieval instruction on the query side only (plan D2); document
/// chunks are embedded bare. fastembed does no auto-prefixing, so we prepend it.
const QUERY_PREFIX: &str = "Represent this sentence for searching relevant passages: ";

/// The real fastembed-backed embedder. The ONNX model (bge-small int8, ~32 MB)
/// is loaded lazily on first use — app startup never blocks and never fails
/// offline; a first-run download failure just leaves docs dirty to retry. One
/// shared session behind a `Mutex` (plan D3); every call runs on a blocking
/// thread, never the async reactor.
pub struct FastEmbedder {
    cache_dir: PathBuf,
    model: Mutex<Option<TextEmbedding>>,
}

impl FastEmbedder {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            model: Mutex::new(None),
        }
    }

    /// True once the model has been loaded — lets the worker emit a one-time
    /// "downloading" status before the first (potentially slow) load.
    pub fn is_loaded(&self) -> bool {
        self.model.lock().unwrap().is_some()
    }

    /// Run `f` with the lazily-initialized model. The first call downloads +
    /// loads it (cache dir pinned to app data dir — plan D1, the crate default
    /// is cwd-relative and wrong for a bundled `.app`); later calls reuse it.
    fn with_model<T>(&self, f: impl FnOnce(&TextEmbedding) -> Result<T>) -> Result<T> {
        let mut guard = self.model.lock().unwrap();
        if guard.is_none() {
            let opts = InitOptions::new(EmbeddingModel::BGESmallENV15Q)
                .with_cache_dir(self.cache_dir.clone());
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
        self.with_model(|model| {
            let mut out = model
                .embed(texts.to_vec(), None)
                .map_err(|e| Error::Embed(format!("embed passages: {e}")))?;
            for v in &mut out {
                normalize(v);
            }
            Ok(out)
        })
    }

    fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        self.with_model(|model| {
            let mut out = model
                .embed(vec![format!("{QUERY_PREFIX}{query}")], None)
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
    fn blob_roundtrips_and_rejects_wrong_length() {
        let mut v: Vec<f32> = (0..EMBED_DIM).map(|i| i as f32 * 0.001 - 0.19).collect();
        normalize(&mut v);
        let blob = embedding_to_blob(&v);
        assert_eq!(blob.len(), EMBED_BYTES);
        let back = blob_to_embedding(&blob).unwrap();
        assert_eq!(v, back);
        // A short blob is rejected, never silently truncated.
        assert!(blob_to_embedding(&blob[..EMBED_BYTES - 4]).is_err());
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
