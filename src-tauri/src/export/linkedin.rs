//! LinkedIn has no markdown — formatting survives only as Unicode styled
//! characters (Mathematical Sans-Serif Bold/Italic), bullets, and plain text.
//! Links are flattened to "text (url)".

use comrak::nodes::{AstNode, ListType, NodeValue};

#[derive(Debug, Clone, Copy, Default)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
}

pub fn render(content: &str) -> String {
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, content, &crate::preview::options());
    let mut out = String::new();
    render_blocks(root, &mut out, 0);
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out.trim().to_string()
}

fn render_blocks<'a>(node: &'a AstNode<'a>, out: &mut String, depth: usize) {
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Heading(_) => {
                let text = inline_text(child, Style { bold: true, ..Style::default() });
                out.push_str(&text);
                out.push_str("\n\n");
            }
            NodeValue::Paragraph => {
                out.push_str(&inline_text(child, Style::default()));
                out.push_str("\n\n");
            }
            NodeValue::List(list) => {
                render_list(child, list.list_type, list.start, out, depth);
                out.push('\n');
            }
            NodeValue::CodeBlock(cb) => {
                out.push_str(cb.literal.trim_end());
                out.push_str("\n\n");
            }
            NodeValue::BlockQuote => {
                render_blocks(child, out, depth);
            }
            NodeValue::ThematicBreak => {
                out.push_str("———\n\n");
            }
            _ => {
                render_blocks(child, out, depth);
            }
        }
    }
}

fn render_list<'a>(
    list_node: &'a AstNode<'a>,
    list_type: ListType,
    start: usize,
    out: &mut String,
    depth: usize,
) {
    let indent = "   ".repeat(depth);
    let mut number = start;
    for item in list_node.children() {
        let marker = match list_type {
            ListType::Bullet => "• ".to_string(),
            ListType::Ordered => {
                let m = format!("{number}. ");
                number += 1;
                m
            }
        };
        for (i, child) in item.children().enumerate() {
            match &child.data.borrow().value {
                NodeValue::List(inner) => {
                    render_list(child, inner.list_type, inner.start, out, depth + 1);
                }
                _ => {
                    let prefix = if i == 0 { format!("{indent}{marker}") } else { format!("{indent}   ") };
                    out.push_str(&prefix);
                    out.push_str(&inline_text(child, Style::default()));
                    out.push('\n');
                }
            }
        }
    }
}

fn inline_text<'a>(node: &'a AstNode<'a>, style: Style) -> String {
    let mut out = String::new();
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Text(t) => out.push_str(&styled(t, style)),
            NodeValue::Code(c) => out.push_str(&c.literal),
            NodeValue::Emph => out.push_str(&inline_text(child, Style { italic: true, ..style })),
            NodeValue::Strong => out.push_str(&inline_text(child, Style { bold: true, ..style })),
            NodeValue::Strikethrough => {
                out.push_str(&inline_text(child, Style { strike: true, ..style }))
            }
            NodeValue::Link(link) => {
                let text = inline_text(child, Style::default());
                if text.is_empty() || text == link.url {
                    out.push_str(&link.url);
                } else {
                    out.push_str(&format!("{text} ({})", link.url));
                }
            }
            NodeValue::Image(img) => {
                let alt = inline_text(child, Style::default());
                if !alt.is_empty() {
                    out.push_str(&format!("[{alt}: {}]", img.url));
                }
            }
            NodeValue::SoftBreak => out.push(' '),
            NodeValue::LineBreak => out.push('\n'),
            _ => out.push_str(&inline_text(child, style)),
        }
    }
    out
}

/// Map ASCII letters/digits to Unicode Mathematical Sans-Serif variants.
fn styled(text: &str, style: Style) -> String {
    let mapped: String = text
        .chars()
        .map(|c| map_char(c, style.bold, style.italic))
        .collect();
    if style.strike {
        // combining long stroke overlay after each char
        mapped.chars().flat_map(|c| [c, '\u{0336}']).collect()
    } else {
        mapped
    }
}

fn map_char(c: char, bold: bool, italic: bool) -> char {
    let (upper, lower, digit): (u32, u32, Option<u32>) = match (bold, italic) {
        (true, false) => (0x1D5D4, 0x1D5EE, Some(0x1D7EC)), // sans-serif bold
        (false, true) => (0x1D608, 0x1D622, None),          // sans-serif italic
        (true, true) => (0x1D63C, 0x1D656, None),           // sans-serif bold italic
        (false, false) => return c,
    };
    let mapped = match c {
        'A'..='Z' => Some(upper + (c as u32 - 'A' as u32)),
        'a'..='z' => Some(lower + (c as u32 - 'a' as u32)),
        '0'..='9' => digit.map(|d| d + (c as u32 - '0' as u32)),
        _ => None,
    };
    mapped.and_then(char::from_u32).unwrap_or(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bold_maps_to_unicode() {
        let out = render("**Hi**");
        assert_eq!(out, "\u{1D5DB}\u{1D5F6}"); // 𝗛𝗶
    }

    #[test]
    fn headings_become_bold_lines() {
        let out = render("# Title\n\nBody text.");
        assert!(out.starts_with('\u{1D5E7}')); // 𝗧
        assert!(out.contains("Body text."));
    }

    #[test]
    fn links_flatten() {
        let out = render("See [my site](https://example.com) now");
        assert_eq!(out, "See my site (https://example.com) now");
    }

    #[test]
    fn lists_get_bullets_and_numbers() {
        let out = render("- one\n- two\n\n1. first\n2. second");
        assert!(out.contains("• one"));
        assert!(out.contains("• two"));
        assert!(out.contains("1. first"));
        assert!(out.contains("2. second"));
    }

    #[test]
    fn code_blocks_kept_plain() {
        let out = render("```\nlet x = 1;\n```");
        assert_eq!(out, "let x = 1;");
    }
}
