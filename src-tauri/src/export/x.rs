//! X (Twitter) export. Two targets share one plain-text block render:
//!   - `x-thread`: segment the doc into numbered posts (≤280 chars), keeping
//!     code blocks intact and links flattened to "text (url)".
//!   - `x-article`: rich HTML paste limited to what the X Article composer
//!     keeps (headings, bold/italic/strike, lists, quotes, links, images);
//!     tables and code blocks flatten to plain text. Plus a plain-text fallback.
//!
//! X has no markdown, so v1 emits plain text. Unicode styling like the
//! LinkedIn exporter could be layered on later by extracting a shared
//! styled-block walker; with only two such targets it isn't worth the
//! coupling yet, so the block walk here is deliberately separate from
//! `export/linkedin.rs` (which adds Unicode styling on the same shape).

use comrak::nodes::{AstNode, ListType, NodeValue};

/// X's hard limit per post.
const HARD_LIMIT: usize = 280;
/// Default packing limit — headroom for the "\n\nn/N" suffix on threads of up
/// to 999 posts. Longer threads re-pack with a smaller limit (see
/// `render_thread`) so the wider suffix still fits within the hard limit.
const MAX_POST_CHARS: usize = 270;
/// Visual divider between posts in the single clipboard/preview blob.
const POST_SEPARATOR: &str = "\n\n━━━━━\n\n";

/// How a block may be broken when it overflows a single post.
enum Split {
    /// Prose: break on word boundaries.
    Words,
    /// Lists: break on line boundaries (keeps bullets/numbers intact).
    Lines,
    /// Code blocks: never break, even if they overflow the limit.
    Never,
}

struct Block {
    text: String,
    split: Split,
}

fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// Worst-case length of the "\n\nn/N" numbering suffix for a thread of `total`
/// posts (a single-post thread is left unnumbered).
fn suffix_len(total: usize) -> usize {
    if total <= 1 {
        return 0;
    }
    let digits = total.to_string().len();
    2 + digits + 1 + digits // "\n\n" + n + "/" + N
}

/// Segment the document into numbered posts. Packs at MAX_POST_CHARS, then —
/// if the thread is long enough that the numbering suffix would push posts
/// past the 280 hard limit (≥1000 posts) — re-packs with a tighter limit.
pub fn render_thread(content: &str) -> Vec<String> {
    let blocks = collect_blocks(content);
    let mut limit = MAX_POST_CHARS;
    loop {
        let posts = pack(&blocks, limit);
        if posts.len() <= 1 || limit + suffix_len(posts.len()) <= HARD_LIMIT {
            return number(posts);
        }
        // the limit strictly decreases each pass and suffix_len is bounded,
        // so this converges in a couple of iterations
        limit = HARD_LIMIT - suffix_len(posts.len());
    }
}

/// The joined thread (numbered posts + dividers) — exactly what is copied.
pub fn render_thread_text(content: &str) -> String {
    render_thread(content).join(POST_SEPARATOR)
}

/// Flat plain-text rendering — the text/plain fallback for the Article paste.
pub fn render_plain(content: &str) -> String {
    collect_blocks(content)
        .iter()
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim()
        .to_string()
}

/// HTML for the X Article composer. X keeps only a subset of formatting —
/// headings, bold/italic/strikethrough, links, bullet/numbered lists,
/// blockquotes, images, dividers — so this emits ONLY that. Tables and code
/// blocks (which X drops) are flattened to plain paragraphs; footnotes and raw
/// HTML are dropped. The result is what actually survives the paste, so the
/// X-Article preview matches the destination rather than the app's render.
pub fn render_article_html(content: &str) -> String {
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, content, &crate::preview::options());
    let mut out = String::new();
    article_blocks(root, &mut out);
    out.trim().to_string()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn esc_attr(s: &str) -> String {
    esc(s).replace('"', "&quot;")
}

fn article_blocks<'a>(node: &'a AstNode<'a>, out: &mut String) {
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Heading(h) => {
                let level = h.level.clamp(1, 6);
                let mut inner = String::new();
                article_inline(child, &mut inner);
                out.push_str(&format!("<h{level}>{inner}</h{level}>\n"));
            }
            NodeValue::Paragraph => {
                let mut inner = String::new();
                article_inline(child, &mut inner);
                if !inner.trim().is_empty() {
                    out.push_str(&format!("<p>{inner}</p>\n"));
                }
            }
            NodeValue::List(list) => article_list(child, list.list_type, out),
            NodeValue::BlockQuote => {
                out.push_str("<blockquote>\n");
                article_blocks(child, out);
                out.push_str("</blockquote>\n");
            }
            // X has no code blocks — flatten to a plain paragraph (line breaks kept)
            NodeValue::CodeBlock(cb) => {
                let text = esc(cb.literal.trim_end()).replace('\n', "<br>");
                if !text.is_empty() {
                    out.push_str(&format!("<p>{text}</p>\n"));
                }
            }
            // X has no tables — flatten each row to a plain paragraph
            NodeValue::Table(_) => article_table(child, out),
            NodeValue::ThematicBreak => out.push_str("<hr>\n"),
            // footnotes and raw HTML are dropped; descend into other containers
            NodeValue::FootnoteDefinition(_) | NodeValue::HtmlBlock(_) => {}
            _ => article_blocks(child, out),
        }
    }
}

fn article_list<'a>(list_node: &'a AstNode<'a>, list_type: ListType, out: &mut String) {
    let tag = match list_type {
        ListType::Bullet => "ul",
        ListType::Ordered => "ol",
    };
    out.push_str(&format!("<{tag}>\n"));
    for item in list_node.children() {
        out.push_str("<li>");
        article_item(item, out);
        out.push_str("</li>\n");
    }
    out.push_str(&format!("</{tag}>\n"));
}

fn article_item<'a>(item: &'a AstNode<'a>, out: &mut String) {
    for child in item.children() {
        match &child.data.borrow().value {
            // tight-list items hold a paragraph; render its text inline (no <p>)
            NodeValue::Paragraph => article_inline(child, out),
            NodeValue::List(inner) => article_list(child, inner.list_type, out),
            _ => article_inline(child, out),
        }
    }
}

fn article_table<'a>(table: &'a AstNode<'a>, out: &mut String) {
    for row in table.children() {
        let cells: Vec<String> = row
            .children()
            .map(|cell| {
                let mut c = String::new();
                article_inline(cell, &mut c);
                c.trim().to_string()
            })
            .filter(|c| !c.is_empty())
            .collect();
        if !cells.is_empty() {
            out.push_str(&format!("<p>{}</p>\n", cells.join(" — ")));
        }
    }
}

fn article_inline<'a>(node: &'a AstNode<'a>, out: &mut String) {
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Text(t) => out.push_str(&esc(t)),
            // X has no inline code — emit the literal as plain text
            NodeValue::Code(c) => out.push_str(&esc(&c.literal)),
            NodeValue::Strong => {
                out.push_str("<strong>");
                article_inline(child, out);
                out.push_str("</strong>");
            }
            NodeValue::Emph => {
                out.push_str("<em>");
                article_inline(child, out);
                out.push_str("</em>");
            }
            NodeValue::Strikethrough => {
                out.push_str("<del>");
                article_inline(child, out);
                out.push_str("</del>");
            }
            NodeValue::Link(link) => {
                let mut text = String::new();
                article_inline(child, &mut text);
                let text = if text.is_empty() { esc(&link.url) } else { text };
                out.push_str(&format!("<a href=\"{}\">{text}</a>", esc_attr(&link.url)));
            }
            NodeValue::Image(img) => {
                let mut alt = String::new();
                article_inline(child, &mut alt);
                out.push_str(&format!("<img src=\"{}\" alt=\"{}\">", esc_attr(&img.url), esc_attr(&alt)));
            }
            NodeValue::SoftBreak => out.push(' '),
            NodeValue::LineBreak => out.push_str("<br>"),
            // raw inline HTML and footnote refs are dropped
            NodeValue::HtmlInline(_) | NodeValue::FootnoteReference(_) => {}
            _ => article_inline(child, out),
        }
    }
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
                let text = render_list(child, list.list_type, list.start, 0);
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
            // block quotes and other containers: descend, treat inner blocks normally
            _ => walk(child, blocks),
        }
    }
}

fn render_list<'a>(
    list_node: &'a AstNode<'a>,
    list_type: ListType,
    start: usize,
    depth: usize,
) -> String {
    let indent = "  ".repeat(depth);
    let mut number = start;
    let mut out = String::new();
    for item in list_node.children() {
        let marker = match list_type {
            ListType::Bullet => "• ".to_string(),
            ListType::Ordered => {
                let m = format!("{number}. ");
                number += 1;
                m
            }
        };
        let mut first = true;
        for child in item.children() {
            match &child.data.borrow().value {
                NodeValue::List(inner) => {
                    out.push_str(&render_list(child, inner.list_type, inner.start, depth + 1));
                    out.push('\n');
                }
                _ => {
                    let prefix = if first {
                        format!("{indent}{marker}")
                    } else {
                        format!("{indent}   ")
                    };
                    out.push_str(&prefix);
                    out.push_str(&inline_text(child));
                    out.push('\n');
                    first = false;
                }
            }
        }
    }
    out.trim_end().to_string()
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

/// Greedily pack blocks into posts of at most `limit` chars, splitting any
/// block that overflows.
fn pack(blocks: &[Block], limit: usize) -> Vec<String> {
    let mut posts: Vec<String> = Vec::new();
    let mut current = String::new();
    for block in blocks {
        let pieces = match block.split {
            Split::Never => vec![block.text.clone()],
            Split::Words => split_by_words(&block.text, limit),
            Split::Lines => split_by_lines(&block.text, limit),
        };
        for piece in pieces {
            // a blank line ("\n\n") joins consecutive blocks within a post
            let joined = char_len(&current) + 2 + char_len(&piece);
            if !current.is_empty() && joined > limit {
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
        let posts = render_thread("Just a short thought.");
        assert_eq!(posts, vec!["Just a short thought.".to_string()]);
    }

    #[test]
    fn long_paragraph_splits_into_numbered_posts() {
        let content = "lorem ipsum dolor sit amet ".repeat(40); // ~1080 chars
        let posts = render_thread(&content);
        assert!(posts.len() > 1, "expected multiple posts");
        for post in &posts {
            assert!(char_len(post) <= 280, "post over limit: {}", char_len(post));
            assert!(post.contains('/'), "post missing numbering: {post:?}");
        }
        assert!(posts.last().unwrap().ends_with(&format!("{n}/{n}", n = posts.len())));
    }

    #[test]
    fn thousand_post_thread_stays_under_hard_limit() {
        // ≥1000 posts makes the "n/N" suffix 9+ chars; the re-pack must keep
        // every numbered post within X's 280 hard limit
        let content = "lorem ipsum dolor sit amet ".repeat(11_000); // ~297k chars
        let posts = render_thread(&content);
        assert!(posts.len() >= 1000, "expected ≥1000 posts, got {}", posts.len());
        for post in &posts {
            assert!(char_len(post) <= HARD_LIMIT, "post over limit: {}", char_len(post));
        }
    }

    #[test]
    fn code_block_stays_intact() {
        let content = "Intro.\n\n```\nfn main() {\n    println!(\"hi\");\n}\n```\n\nOutro.";
        let joined = render_thread(content).join("\n");
        assert!(joined.contains("fn main() {"));
        assert!(joined.contains("    println!(\"hi\");"));
    }

    #[test]
    fn links_are_flattened() {
        assert_eq!(render_plain("See [site](https://x.com) ok"), "See site (https://x.com) ok");
    }

    #[test]
    fn lists_render_with_markers() {
        let text = render_plain("- one\n- two\n\n1. a\n2. b");
        assert!(text.contains("• one"));
        assert!(text.contains("• two"));
        assert!(text.contains("1. a"));
        assert!(text.contains("2. b"));
    }

    #[test]
    fn article_html_keeps_supported_formatting() {
        let html = render_article_html("# Title\n\n**bold** and *em* and ~~no~~\n\n- one\n- two");
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>em</em>"));
        assert!(html.contains("<del>no</del>"));
        assert!(html.contains("<ul>"));
        assert!(html.contains("<li>one</li>"));
    }

    #[test]
    fn article_html_flattens_what_x_drops() {
        let md = "| a | b |\n| - | - |\n| 1 | 2 |\n\n```\ncode here\n```\n\ntext[^1]\n\n[^1]: note";
        let html = render_article_html(md);
        // tables and code blocks must not reach X as <table>/<pre>
        assert!(!html.contains("<table"), "table leaked: {html}");
        assert!(!html.contains("<pre"), "code block leaked: {html}");
        assert!(!html.contains("<code"), "code leaked: {html}");
        // their text survives, flattened into paragraphs
        assert!(html.contains("1 — 2"), "table not flattened: {html}");
        assert!(html.contains("code here"), "code text lost: {html}");
        // footnote markers are dropped
        assert!(!html.contains("footnote"), "footnote leaked: {html}");
    }

    #[test]
    fn very_long_url_is_hard_split() {
        let url = format!("https://example.com/{}", "a".repeat(400));
        let posts = render_thread(&url);
        assert!(posts.len() > 1);
        for post in &posts {
            assert!(char_len(post) <= 280);
        }
    }
}
