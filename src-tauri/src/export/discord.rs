//! Discord export — Discord's composer accepts a markdown subset (headings,
//! bold, italic, underline, strike, inline + fenced code, blockquotes via `>`,
//! lists, spoilers `||`, and links). We copy the source verbatim, stripping
//! footnote definitions/refs (Discord renders them as literal noise) and
//! mapping image syntax to a plain link (Discord doesn't inline remote images
//! in messages anyway). Tables are not supported by Discord and are flattened.
//!
//! Reuses the Reddit re-serializer shape but flattens tables to bullet rows.

use comrak::nodes::{AstNode, ListType, NodeValue};

pub fn render(content: &str) -> String {
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, content, &crate::preview::options());
    let mut out = String::new();
    emit(root, &mut out);
    out.trim().to_string() + "\n"
}

fn emit<'a>(node: &'a AstNode<'a>, out: &mut String) {
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::FootnoteDefinition(_) => {}
            _ => emit_node(child, out),
        }
    }
}

fn emit_node<'a>(node: &'a AstNode<'a>, out: &mut String) {
    match &node.data.borrow().value {
        NodeValue::Document => emit(node, out),
        NodeValue::Paragraph => {
            inline_block(node, out);
            out.push_str("\n\n");
        }
        NodeValue::Heading(h) => {
            out.push_str(&"#".repeat(h.level.min(3) as usize));
            out.push(' ');
            inline_block(node, out);
            out.push_str("\n\n");
        }
        NodeValue::CodeBlock(cb) => {
            let lang = cb.info.trim();
            out.push_str("```");
            if !lang.is_empty() {
                out.push_str(lang);
            }
            out.push('\n');
            out.push_str(cb.literal.trim_end_matches('\n'));
            out.push('\n');
            out.push_str("```\n\n");
        }
        NodeValue::List(list) => {
            let mut n = list.start;
            for item in node.children() {
                match list.list_type {
                    ListType::Bullet => out.push_str("- "),
                    ListType::Ordered => {
                        out.push_str(&format!("{n}. "));
                        n += 1;
                    }
                }
                let mut first = true;
                for c in item.children() {
                    match &c.data.borrow().value {
                        NodeValue::List(_) => emit_node(c, out),
                        NodeValue::Paragraph => {
                            if !first {
                                out.push_str("  ");
                            }
                            inline_block(c, out);
                            out.push('\n');
                            first = false;
                        }
                        _ => inline_block(c, out),
                    }
                }
            }
            out.push('\n');
        }
        NodeValue::BlockQuote => {
            out.push_str("> ");
            let mut inner = String::new();
            for c in node.children() {
                emit_node(c, &mut inner);
            }
            out.push_str(&inner.trim_end().replace('\n', "\n> "));
            out.push_str("\n\n");
        }
        NodeValue::ThematicBreak => out.push_str("---\n\n"),
        NodeValue::HtmlBlock(html) => out.push_str(&html.literal),
        // Discord has no tables — flatten to bullets.
        NodeValue::Table(_) => flatten_table(node, out),
        _ => {
            for c in node.children() {
                emit_node(c, out);
            }
        }
    }
}

fn flatten_table<'a>(node: &'a AstNode<'a>, out: &mut String) {
    let mut ri = 0usize;
    for row in node.children() {
        let cells: Vec<String> = row
            .children()
            .map(|cell| {
                let mut s = String::new();
                inline_block(cell, &mut s);
                s.trim().to_string()
            })
            .filter(|c| !c.is_empty())
            .collect();
        if cells.is_empty() {
            ri += 1;
            continue;
        }
        if ri == 0 {
            out.push_str("**");
            out.push_str(&cells.join(" | "));
            out.push_str("**\n");
        } else {
            out.push_str("- ");
            out.push_str(&cells.join(" — "));
            out.push('\n');
        }
        ri += 1;
    }
    out.push('\n');
}

fn inline_block<'a>(node: &'a AstNode<'a>, out: &mut String) {
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Text(t) => out.push_str(t),
            NodeValue::Code(c) => {
                out.push('`');
                out.push_str(&c.literal);
                out.push('`');
            }
            NodeValue::Emph => {
                out.push('*');
                inline_block(child, out);
                out.push('*');
            }
            NodeValue::Strong => {
                out.push_str("**");
                inline_block(child, out);
                out.push_str("**");
            }
            NodeValue::Strikethrough => {
                out.push_str("~~");
                inline_block(child, out);
                out.push_str("~~");
            }
            NodeValue::Link(link) => {
                inline_block(child, out);
                out.push_str(&format!(" ({})", link.url));
            }
            NodeValue::Image(img) => out.push_str(&img.url),
            NodeValue::FootnoteReference(_) => {}
            NodeValue::SoftBreak => out.push('\n'),
            NodeValue::LineBreak => out.push('\n'),
            NodeValue::HtmlInline(h) => out.push_str(h),
            _ => inline_block(child, out),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_markdown_formatting() {
        let out = render("# Title\n\n**bold** and *em*");
        assert!(out.contains("# Title"));
        assert!(out.contains("**bold**"));
        assert!(out.contains("*em*"));
    }

    #[test]
    fn table_flattened_to_bullets() {
        let out = render("| a | b |\n| - | - |\n| 1 | 2 |");
        assert!(!out.contains("| --- |"));
        assert!(out.contains("**a | b**"));
        assert!(out.contains("- 1 — 2"));
    }

    #[test]
    fn footnotes_dropped() {
        let out = render("hi[^1]\n\n[^1]: x");
        assert!(!out.contains("[^1]"));
    }
}
