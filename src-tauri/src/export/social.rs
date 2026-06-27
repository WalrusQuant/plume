//! Shared segmentation for character-limited social targets (X 280, Mastodon
//! 500, Bluesky 300, Instagram Threads 500). Extracted from `x.rs` once a
//! second char-limited target (Mastodon) appeared — the comment in x.rs that
//! said "with only two such targets it isn't worth the coupling yet" no longer
//! holds at four.
//!
//! Splits a doc into numbered posts under a hard char limit, packing on word
//! boundaries (prose), line boundaries (lists), or never (code blocks).

use comrak::nodes::{AstNode, NodeValue};

/// How a block may be broken when it overflows a single post.
enum Split {
    Words,
    Lines,
    Never,
}

struct Block {
    text: String,
    split: Split,
}

/// Visual divider between posts in the single clipboard/preview blob. Kept
/// subtle so it pastes cleanly into any compose box.
const POST_SEPARATOR: &str = "\n\n━━━━━\n\n";

fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// Worst-case length of the "\n\nn/N" numbering suffix for a thread of `total`
/// posts (a single-post thread is left unnumbered).
fn suffix_len(total: usize, hard_limit: usize) -> usize {
    // Numbering is suppressed for single posts, and threads longer than the
    // point where the suffix would overflow the hard limit are re-packed, so
    // the suffix always fits. We just compute its length for the packing loop.
    let _ = hard_limit;
    if total <= 1 {
        return 0;
    }
    let digits = total.to_string().len();
    2 + digits + 1 + digits // "\n\n" + n + "/" + N
}

/// Segment the document into numbered posts, each at most `hard_limit` chars.
/// Packs at `hard_limit - 10` headroom by default, then — if a long thread's
/// numbering suffix would push posts past the limit — re-packs tighter.
pub fn render_thread(content: &str, hard_limit: usize) -> Vec<String> {
    let blocks = collect_blocks(content);
    let initial = hard_limit.saturating_sub(10);
    let mut limit = initial.max(hard_limit.min(40));
    loop {
        let posts = pack(&blocks, limit, hard_limit);
        if posts.len() <= 1 || limit + suffix_len(posts.len(), hard_limit) <= hard_limit {
            return number(posts);
        }
        limit = hard_limit - suffix_len(posts.len(), hard_limit);
    }
}

/// The joined thread (numbered posts + dividers) — exactly what is copied.
pub fn render_thread_text(content: &str, hard_limit: usize) -> String {
    render_thread(content, hard_limit).join(POST_SEPARATOR)
}

/// Flat plain-text rendering — used as a text/plain fallback.
pub fn render_plain(content: &str) -> String {
    collect_blocks(content)
        .iter()
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim()
        .to_string()
}

fn collect_blocks(content: &str) -> Vec<Block> {
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, content, &crate::preview::options());
    let mut blocks = Vec::new();
    walk(root, &mut blocks);
    blocks
}

fn walk<'a>(node: &'a AstNode<'a>, blocks: &mut Vec<Block>) {
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Heading(_) | NodeValue::Paragraph => {
                let text = inline_text(child);
                if !text.trim().is_empty() {
                    blocks.push(Block { text, split: Split::Words });
                }
            }
            NodeValue::List(list) => {
                let text = super::render_plain_list(
                    child,
                    list.list_type,
                    list.start,
                    0,
                    "  ",
                    inline_text,
                );
                if !text.trim().is_empty() {
                    blocks.push(Block { text, split: Split::Lines });
                }
            }
            NodeValue::CodeBlock(cb) => {
                blocks.push(Block {
                    text: cb.literal.trim_end().to_string(),
                    split: Split::Never,
                });
            }
            NodeValue::ThematicBreak => {}
            _ => walk(child, blocks),
        }
    }
}

fn inline_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut out = String::new();
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Text(t) => out.push_str(t),
            NodeValue::Code(c) => out.push_str(&c.literal),
            NodeValue::Emph | NodeValue::Strong | NodeValue::Strikethrough => {
                out.push_str(&inline_text(child))
            }
            NodeValue::Link(link) => {
                let text = inline_text(child);
                if text.is_empty() || text == link.url {
                    out.push_str(&link.url);
                } else {
                    out.push_str(&format!("{text} ({})", link.url));
                }
            }
            NodeValue::Image(img) => {
                let alt = inline_text(child);
                if !alt.is_empty() {
                    out.push_str(&format!("[{alt}: {}]", img.url));
                }
            }
            NodeValue::SoftBreak => out.push(' '),
            NodeValue::LineBreak => out.push('\n'),
            _ => out.push_str(&inline_text(child)),
        }
    }
    out
}

/// Greedily pack blocks into posts. `limit` is the soft pack target; a single
/// piece is never grown past `hard_limit` (it is split instead).
fn pack(blocks: &[Block], limit: usize, hard_limit: usize) -> Vec<String> {
    let pack_limit = limit.min(hard_limit);
    let mut posts: Vec<String> = Vec::new();
    let mut current = String::new();
    for block in blocks {
        let pieces = match block.split {
            Split::Never => vec![block.text.clone()],
            Split::Words => split_by_words(&block.text, pack_limit),
            Split::Lines => split_by_lines(&block.text, pack_limit),
        };
        for piece in pieces {
            let joined = char_len(&current) + 2 + char_len(&piece);
            if !current.is_empty() && joined > pack_limit {
                posts.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push_str("\n\n");
            }
            current.push_str(&piece);
        }
    }
    let last = current.trim();
    if !last.is_empty() {
        posts.push(last.to_string());
    }
    posts
}

fn split_by_words(text: &str, limit: usize) -> Vec<String> {
    if char_len(text) <= limit {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if char_len(word) > limit {
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            chunks.extend(hard_split(word, limit));
            continue;
        }
        let extra = if current.is_empty() { 0 } else { 1 };
        if !current.is_empty() && char_len(&current) + extra + char_len(word) > limit {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn split_by_lines(text: &str, limit: usize) -> Vec<String> {
    if char_len(text) <= limit {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if char_len(line) > limit {
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            chunks.extend(split_by_words(line, limit));
            continue;
        }
        let extra = if current.is_empty() { 0 } else { 1 };
        if !current.is_empty() && char_len(&current) + extra + char_len(line) > limit {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Last-resort split of a single token longer than the limit (e.g. a URL).
fn hard_split(word: &str, limit: usize) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut piece = String::new();
    for c in word.chars() {
        if char_len(&piece) == limit {
            pieces.push(std::mem::take(&mut piece));
        }
        piece.push(c);
    }
    if !piece.is_empty() {
        pieces.push(piece);
    }
    pieces
}

/// Append " n/N" numbering. A single-post thread is left unnumbered.
fn number(posts: Vec<String>) -> Vec<String> {
    let total = posts.len();
    if total <= 1 {
        return posts;
    }
    posts
        .into_iter()
        .enumerate()
        .map(|(i, post)| format!("{post}\n\n{}/{total}", i + 1))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_content_is_single_unnumbered_post() {
        let posts = render_thread("Just a short thought.", 280);
        assert_eq!(posts, vec!["Just a short thought.".to_string()]);
    }

    #[test]
    fn long_paragraph_splits_within_mastodon_limit() {
        let content = "lorem ipsum dolor sit amet ".repeat(40); // ~1080 chars
        let posts = render_thread(&content, 500);
        assert!(posts.len() > 1);
        for post in &posts {
            assert!(char_len(post) <= 500, "over limit: {}", char_len(post));
        }
    }

    #[test]
    fn code_block_stays_intact() {
        let content = "Intro.\n\n```\nfn main() {\n    println!(\"hi\");\n}\n```\n\nOutro.";
        let joined = render_thread(content, 280).join("\n");
        assert!(joined.contains("fn main() {"));
    }
}
