//! Generic rich-HTML paste for targets that keep formatting on paste — Google
//! Docs, Substack, Medium, Notion, Ghost. Produces a clean, self-consistent
//! fragment (no full document wrapper, no inline styles) using semantic tags
//! that survive a "paste and keep formatting" into a contenteditable composer.
//!
//! Distinct from `html.rs` (a full standalone `.html` file with its own CSS)
//! and from `x.rs::render_article_html` (which trims to the X-Article subset).

use comrak::nodes::{AstNode, ListType, NodeValue};

/// Inline styles hint — Newsletter/Substack keeps inline color/font-size hints
/// better than bare tags, so we offer a styled variant for the "Newsletter"
/// target. Google Docs keeps bare semantic tags well, so it uses `Bare`.
#[derive(Clone, Copy)]
pub enum Flavor {
    Bare,
    Newsletter,
}

pub fn render(content: &str, flavor: Flavor) -> String {
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, content, &crate::preview::options());
    let mut out = String::new();
    blocks(root, &mut out, flavor);
    out.trim().to_string()
}

fn blocks<'a>(node: &'a AstNode<'a>, out: &mut String, flavor: Flavor) {
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Heading(h) => {
                let level = h.level.clamp(1, 6);
                let mut inner = String::new();
                inline(child, &mut inner, flavor);
                if !inner.trim().is_empty() {
                    if matches!(flavor, Flavor::Newsletter) && level <= 2 {
                        let style = if level == 1 {
                            " style=\"font-size:1.8em;line-height:1.2;margin:1.2em 0 .5em;font-weight:700\""
                        } else {
                            " style=\"font-size:1.4em;line-height:1.25;margin:1.1em 0 .4em;font-weight:600\""
                        };
                        out.push_str(&format!("<h{level}{style}>{inner}</h{level}>\n"));
                    } else {
                        out.push_str(&format!("<h{level}>{inner}</h{level}>\n"));
                    }
                }
            }
            NodeValue::Paragraph => {
                let mut inner = String::new();
                inline(child, &mut inner, flavor);
                if !inner.trim().is_empty() {
                    out.push_str(&format!("<p>{inner}</p>\n"));
                }
            }
            NodeValue::List(list) => emit_list(child, list.list_type, out, flavor),
            NodeValue::CodeBlock(cb) => {
                let text = esc(cb.literal.trim_end());
                let lang = cb.info.trim();
                let cls = if lang.is_empty() {
                    String::new()
                } else {
                    format!(" class=\"language-{lang}\"")
                };
                out.push_str(&format!("<pre><code{cls}>{text}</code></pre>\n"));
            }
            NodeValue::BlockQuote => {
                let mut inner = String::new();
                blocks(child, &mut inner, flavor);
                out.push_str(&format!("<blockquote>\n{inner}</blockquote>\n"));
            }
            NodeValue::ThematicBreak => out.push_str("<hr>\n"),
            NodeValue::Table(_) => table(child, out),
            NodeValue::FootnoteDefinition(_) | NodeValue::HtmlBlock(_) => {}
            _ => blocks(child, out, flavor),
        }
    }
}

fn emit_list<'a>(node: &'a AstNode<'a>, list_type: ListType, out: &mut String, flavor: Flavor) {
    let tag = match list_type {
        ListType::Bullet => "ul",
        ListType::Ordered => "ol",
    };
    out.push_str(&format!("<{tag}>\n"));
    for entry in node.children() {
        out.push_str("<li>");
        item(entry, out, flavor);
        out.push_str("</li>\n");
    }
    out.push_str(&format!("</{tag}>\n"));
}

fn item<'a>(node: &'a AstNode<'a>, out: &mut String, flavor: Flavor) {
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Paragraph => inline(child, out, flavor),
            NodeValue::List(inner) => emit_list(child, inner.list_type, out, flavor),
            _ => inline(child, out, flavor),
        }
    }
}

fn table<'a>(node: &'a AstNode<'a>, out: &mut String) {
    out.push_str("<table>\n");
    let mut ri = 0usize;
    for row in node.children() {
        out.push_str("<tr>");
        for cell in row.children() {
            let mut c = String::new();
            inline(cell, &mut c, Flavor::Bare);
            let tag = if ri == 0 { "th" } else { "td" };
            out.push_str(&format!("<{tag}>{}</{tag}>", c.trim()));
        }
        out.push_str("</tr>\n");
        ri += 1;
    }
    out.push_str("</table>\n");
}

fn inline<'a>(node: &'a AstNode<'a>, out: &mut String, flavor: Flavor) {
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Text(t) => out.push_str(&esc(t)),
            NodeValue::Code(c) => out.push_str(&format!("<code>{}</code>", esc(&c.literal))),
            NodeValue::Emph => {
                out.push_str("<em>");
                inline(child, out, flavor);
                out.push_str("</em>");
            }
            NodeValue::Strong => {
                out.push_str("<strong>");
                inline(child, out, flavor);
                out.push_str("</strong>");
            }
            NodeValue::Strikethrough => {
                out.push_str("<s>");
                inline(child, out, flavor);
                out.push_str("</s>");
            }
            NodeValue::Link(link) => {
                let mut text = String::new();
                inline(child, &mut text, flavor);
                let text = if text.trim().is_empty() { esc(&link.url) } else { text };
                out.push_str(&format!("<a href=\"{}\">{text}</a>", esc_attr(&link.url)));
            }
            NodeValue::Image(img) => {
                let mut alt = String::new();
                inline(child, &mut alt, flavor);
                out.push_str(&format!("<img src=\"{}\" alt=\"{}\">", esc_attr(&img.url), esc_attr(&alt)));
            }
            NodeValue::SoftBreak => out.push(' '),
            NodeValue::LineBreak => out.push_str("<br>"),
            NodeValue::HtmlInline(_) | NodeValue::FootnoteReference(_) => {}
            _ => inline(child, out, flavor),
        }
    }
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
    fn bare_keeps_semantic_tags() {
        let html = render("# T\n\n**b** *i* ~~s~~\n\n- one\n- two", Flavor::Bare);
        assert!(html.contains("<h1>T</h1>"));
        assert!(html.contains("<strong>b</strong>"));
        assert!(html.contains("<em>i</em>"));
        assert!(html.contains("<s>s</s>"));
        assert!(html.contains("<ul>"));
        assert!(html.contains("<li>one</li>"));
    }

    #[test]
    fn newsletter_adds_inline_h1_style() {
        let html = render("# T\n\ntext", Flavor::Newsletter);
        assert!(html.contains("<h1 style=\""));
        assert!(html.contains("font-size:1.8em"));
    }

    #[test]
    fn table_emits_th_and_td() {
        let html = render("| a | b |\n| - | - |\n| 1 | 2 |", Flavor::Bare);
        assert!(html.contains("<th>a</th>"));
        assert!(html.contains("<td>1</td>"));
    }

    #[test]
    fn code_block_has_language_class() {
        let html = render("```rust\nfn x()\n```", Flavor::Bare);
        assert!(html.contains("class=\"language-rust\""));
    }
}
