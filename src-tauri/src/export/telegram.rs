//! Telegram export — Telegram's composer supports a limited HTML subset
//! (`<b>`, `<i>`, `<u>`, `<s>`, `<a href>`, `<code>`, `<pre>`, `<blockquote>`).
//! No headings, no lists, no tables, no images. We emit Telegram-flavored HTML
//! that can be pasted with "Paste as HTML" or sent via the Bot API parse mode
//! `HTML`. Lists flatten to plain lines; headings become bold lines.

use comrak::nodes::{AstNode, ListType, NodeValue};

pub fn render(content: &str) -> String {
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, content, &crate::preview::options());
    let mut out = String::new();
    blocks(root, &mut out);
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out.trim().to_string()
}

fn blocks<'a>(node: &'a AstNode<'a>, out: &mut String) {
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Heading(_) => {
                let mut inner = String::new();
                inline(child, &mut inner);
                if !inner.trim().is_empty() {
                    out.push_str(&format!("<b>{inner}</b>\n\n"));
                }
            }
            NodeValue::Paragraph => {
                let mut inner = String::new();
                inline(child, &mut inner);
                if !inner.trim().is_empty() {
                    out.push_str(&inner);
                    out.push_str("\n\n");
                }
            }
            NodeValue::List(list) => render_list(child, list.list_type, list.start, out),
            NodeValue::CodeBlock(cb) => {
                let text = esc(cb.literal.trim_end());
                if !text.is_empty() {
                    out.push_str(&format!("<pre>{text}</pre>\n\n"));
                }
            }
            NodeValue::BlockQuote => {
                let mut inner = String::new();
                blocks(child, &mut inner);
                out.push_str(&format!("<blockquote>{}</blockquote>\n\n", inner.trim()));
            }
            NodeValue::ThematicBreak => out.push_str("---\n\n"),
            // Telegram has no tables — flatten rows to lines.
            NodeValue::Table(_) => flatten_table(child, out),
            _ => blocks(child, out),
        }
    }
}

fn render_list<'a>(node: &'a AstNode<'a>, list_type: ListType, start: usize, out: &mut String) {
    let mut number = start;
    for item in node.children() {
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
                NodeValue::List(inner) => render_list(child, inner.list_type, inner.start, out),
                _ => {
                    let mut inner = String::new();
                    inline(child, &mut inner);
                    let prefix = if first { &marker } else { "   " };
                    out.push_str(prefix);
                    out.push_str(&inner);
                    out.push('\n');
                    first = false;
                }
            }
        }
    }
    out.push('\n');
}

fn flatten_table<'a>(node: &'a AstNode<'a>, out: &mut String) {
    let mut ri = 0usize;
    for row in node.children() {
        let cells: Vec<String> = row
            .children()
            .map(|cell| {
                let mut s = String::new();
                inline(cell, &mut s);
                s.trim().to_string()
            })
            .filter(|c| !c.is_empty())
            .collect();
        if cells.is_empty() {
            ri += 1;
            continue;
        }
        if ri == 0 {
            out.push_str(&format!("<b>{}</b>\n", cells.join(" | ")));
        } else {
            out.push_str(&format!("{}\n", cells.join(" | ")));
        }
        ri += 1;
    }
    out.push('\n');
}

fn inline<'a>(node: &'a AstNode<'a>, out: &mut String) {
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Text(t) => out.push_str(&esc(t)),
            NodeValue::Code(c) => out.push_str(&format!("<code>{}</code>", esc(&c.literal))),
            NodeValue::Emph => {
                out.push_str("<i>");
                inline(child, out);
                out.push_str("</i>");
            }
            NodeValue::Strong => {
                out.push_str("<b>");
                inline(child, out);
                out.push_str("</b>");
            }
            NodeValue::Strikethrough => {
                out.push_str("<s>");
                inline(child, out);
                out.push_str("</s>");
            }
            NodeValue::Link(link) => {
                let mut text = String::new();
                inline(child, &mut text);
                let text = if text.trim().is_empty() { esc(&link.url) } else { text };
                out.push_str(&format!("<a href=\"{}\">{text}</a>", esc_attr(&link.url)));
            }
            // Telegram can't inline remote images in messages — drop to alt text.
            NodeValue::Image(_) => {
                let alt = inline_collect(child);
                if !alt.trim().is_empty() {
                    out.push_str(&esc(&alt));
                }
            }
            NodeValue::SoftBreak => out.push('\n'),
            NodeValue::LineBreak => out.push('\n'),
            NodeValue::HtmlBlock(_) | NodeValue::HtmlInline(_) | NodeValue::FootnoteReference(_) => {}
            _ => inline(child, out),
        }
    }
}

fn inline_collect<'a>(node: &'a AstNode<'a>) -> String {
    let mut s = String::new();
    inline(node, &mut s);
    s
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn esc_attr(s: &str) -> String {
    esc(s).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_telegram_tags() {
        let out = render("**b** *i* ~~s~~ `c`");
        assert!(out.contains("<b>b</b>"));
        assert!(out.contains("<i>i</i>"));
        assert!(out.contains("<s>s</s>"));
        assert!(out.contains("<code>c</code>"));
    }

    #[test]
    fn code_block_becomes_pre() {
        let out = render("```\nfn x()\n```");
        assert!(out.contains("<pre>fn x()</pre>"));
    }

    #[test]
    fn link_uses_anchor() {
        let out = render("[x](https://e.com)");
        assert!(out.contains("<a href=\"https://e.com\">x</a>"));
    }

    #[test]
    fn escapes_ampersand() {
        let out = render("a & b");
        assert!(out.contains("a &amp; b"));
    }
}
