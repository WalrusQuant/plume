//! Plain-text export — all formatting stripped to a clean readable string.
//! Used by the `.txt` file export and as a generic "copy as plain text" target.
//! Lists keep their markers; headings become plain lines; code blocks are kept
//! verbatim; links flatten to "text (url)"; images to "[alt: url]".

use comrak::nodes::{AstNode, NodeValue};

pub fn render(content: &str) -> String {
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, content, &crate::preview::options());
    let mut out = String::new();
    blocks(root, &mut out, 0);
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out.trim().to_string()
}

fn blocks<'a>(node: &'a AstNode<'a>, out: &mut String, depth: usize) {
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Heading(_) | NodeValue::Paragraph => {
                let text = inline(child);
                if !text.trim().is_empty() {
                    out.push_str(&text);
                    out.push_str("\n\n");
                }
            }
            NodeValue::List(list) => {
                let text = super::render_plain_list(
                    child,
                    list.list_type,
                    list.start,
                    depth,
                    "  ",
                    inline,
                );
                out.push_str(&text);
                out.push('\n');
            }
            NodeValue::CodeBlock(cb) => {
                out.push_str(cb.literal.trim_end());
                out.push_str("\n\n");
            }
            NodeValue::ThematicBreak => out.push_str("---\n\n"),
            _ => blocks(child, out, depth),
        }
    }
}

fn inline<'a>(node: &'a AstNode<'a>) -> String {
    let mut out = String::new();
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Text(t) => out.push_str(t),
            NodeValue::Code(c) => out.push_str(&c.literal),
            NodeValue::Emph | NodeValue::Strong | NodeValue::Strikethrough => {
                out.push_str(&inline(child))
            }
            NodeValue::Link(link) => {
                let text = inline(child);
                if text.is_empty() || text == link.url {
                    out.push_str(&link.url);
                } else {
                    out.push_str(&format!("{text} ({})", link.url));
                }
            }
            NodeValue::Image(img) => {
                let alt = inline(child);
                if !alt.is_empty() {
                    out.push_str(&format!("[{alt}: {}]", img.url));
                }
            }
            NodeValue::SoftBreak => out.push(' '),
            NodeValue::LineBreak => out.push('\n'),
            _ => out.push_str(&inline(child)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_formatting_keeps_text() {
        let out = render("# Title\n\n**bold** and *em* and ~~str~~\n\nplain");
        assert_eq!(out, "Title\n\nbold and em and str\n\nplain");
    }

    #[test]
    fn links_flatten() {
        assert_eq!(
            render("see [site](https://x.com) now"),
            "see site (https://x.com) now",
        );
    }

    #[test]
    fn lists_keep_markers() {
        let out = render("- one\n- two\n\n1. a\n2. b");
        assert!(out.contains("• one"));
        assert!(out.contains("1. a"));
    }
}
