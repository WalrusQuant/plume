//! Reddit export — Reddit's post/comment composer accepts markdown, so we copy
//! the source verbatim. Reddit's flavor is close to CommonMark (it does NOT
//! support footnotes; those render as literal `[^1]`). We strip footnote
//! definitions and references so the paste is clean.

use comrak::nodes::{AstNode, NodeValue};

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
            // Drop footnote definitions entirely (Reddit renders them as noise).
            NodeValue::FootnoteDefinition(_) => {}
            _ => emit_node(child, out),
        }
    }
}

/// Re-serialize the AST back to markdown, skipping footnote-reference inline
/// nodes. comrak doesn't ship a markdown writer, so we reconstruct the common
/// block/inline shapes. For anything exotic we fall back to literal text.
fn emit_node<'a>(node: &'a AstNode<'a>, out: &mut String) {
    match &node.data.borrow().value {
        NodeValue::Document => emit(node, out),
        NodeValue::Paragraph => {
            inline_block(node, out);
            out.push_str("\n\n");
        }
        NodeValue::Heading(h) => {
            out.push_str(&"#".repeat(h.level.min(6) as usize));
            out.push(' ');
            inline_block(node, out);
            out.push('\n');
            out.push('\n');
        }
        NodeValue::CodeBlock(cb) => {
            let fence = "```";
            let lang = cb.info.trim();
            out.push_str(fence);
            if !lang.is_empty() {
                out.push_str(lang);
            }
            out.push('\n');
            out.push_str(cb.literal.trim_end_matches('\n'));
            out.push('\n');
            out.push_str(fence);
            out.push_str("\n\n");
        }
        NodeValue::List(list) => {
            let mut n = list.start;
            for item in node.children() {
                match list.list_type {
                    comrak::nodes::ListType::Bullet => out.push_str("- "),
                    comrak::nodes::ListType::Ordered => {
                        out.push_str(&format!("{n}. "));
                        n += 1;
                    }
                }
                // item children: paragraph(s) + nested list
                let mut first = true;
                for c in item.children() {
                    match &c.data.borrow().value {
                        NodeValue::List(_) => {
                            emit_node(c, out);
                        }
                        NodeValue::Paragraph => {
                            if !first {
                                out.push_str("  ");
                            }
                            inline_block(c, out);
                            out.push('\n');
                            first = false;
                        }
                        _ => {
                            inline_block(c, out);
                        }
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
        NodeValue::ThematicBreak => {
            out.push_str("---\n\n");
        }
        NodeValue::HtmlBlock(html) => {
            out.push_str(&html.literal);
        }
        NodeValue::Table(_) => {
            // Reddit supports tables — re-emit the source rows from the AST.
            emit_table(node, out);
            out.push('\n');
        }
        _ => {
            // Descend for any other container.
            for c in node.children() {
                emit_node(c, out);
            }
        }
    }
}

fn emit_table<'a>(node: &'a AstNode<'a>, out: &mut String) {
    let rows: Vec<Vec<String>> = node
        .children()
        .map(|row| {
            row.children()
                .map(|cell| {
                    let mut s = String::new();
                    inline_block(cell, &mut s);
                    s.trim().replace('|', "\\|").to_string()
                })
                .collect()
        })
        .collect();
    if rows.is_empty() {
        return;
    }
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    for (ri, row) in rows.iter().enumerate() {
        out.push('|');
        for cell in row.iter() {
            out.push(' ');
            out.push_str(cell);
            out.push_str(" |");
        }
        // pad missing cells
        for _ in row.len()..cols {
            out.push_str("  |");
        }
        out.push('\n');
        if ri == 0 {
            out.push('|');
            for _ in 0..cols {
                out.push_str(" --- |");
            }
            out.push('\n');
        }
    }
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
                out.push('[');
                inline_block(child, out);
                out.push_str("](");
                out.push_str(&link.url);
                if !link.title.is_empty() {
                    out.push_str(&format!(" \"{}\"", link.title));
                }
                out.push(')');
            }
            NodeValue::Image(img) => {
                out.push_str("![");
                inline_block(child, out);
                out.push_str("](");
                out.push_str(&img.url);
                out.push(')');
            }
            // Reddit renders footnote refs as noise — drop them.
            NodeValue::FootnoteReference(_) => {}
            NodeValue::SoftBreak => out.push('\n'),
            NodeValue::LineBreak => {
                out.push_str("  \n");
            }
            NodeValue::HtmlInline(h) => out.push_str(h),
            _ => inline_block(child, out),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_and_bold_survive() {
        let out = render("# Title\n\n**bold** here");
        assert!(out.contains("# Title"));
        assert!(out.contains("**bold** here"));
    }

    #[test]
    fn footnote_refs_dropped() {
        let out = render("text[^1]\n\n[^1]: note");
        assert!(!out.contains("[^1]"));
        assert!(!out.contains("note"));
    }

    #[test]
    fn tables_re_emitted() {
        let out = render("| a | b |\n| - | - |\n| 1 | 2 |");
        assert!(out.contains("| a | b |"));
        assert!(out.contains("| --- |"));
        assert!(out.contains("| 1 | 2 |"));
    }

    #[test]
    fn links_re_emitted() {
        let out = render("[x](https://e.com)");
        assert!(out.contains("[x](https://e.com)"));
    }
}
