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

use comrak::nodes::{AstNode, NodeValue};

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
