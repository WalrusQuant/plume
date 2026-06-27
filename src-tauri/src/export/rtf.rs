//! RTF (Rich Text Format) export — a `.rtf` file that opens in Word, Pages,
//! TextEdit, LibreOffice, and pastes into Google Docs. RTF is a plain-text
//! format, so we generate it directly with no dependency. Supports headings
//! (sized bold), bold/italic/strikethrough, inline + block code, lists (text
//! bullets/numbers — universally readable), blockquotes (indented + greyed),
//! tables (native `trowd` cells), and hyperlinks via `\field` HYPERLINK.

use comrak::nodes::{AstNode, ListType, NodeValue};

pub fn render(content: &str) -> String {
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, content, &crate::preview::options());

    let mut body = String::new();
    body_blocks(root, &mut body, 0);

    let header = r#"{\fonttbl{\f0 Times New Roman;}{\f1 Menlo;}}{\colortbl;\red0\green0\blue255;\red96\green96\blue96;}"#;
    let body = body.trim_end_matches('\n').to_string();
    format!("{{\\rtf1\\ansi\\deff0\n{header}\n\\f0\n{body}\n}}\n")
}

fn body_blocks<'a>(node: &'a AstNode<'a>, out: &mut String, depth: usize) {
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Heading(h) => {
                let size = match h.level {
                    1 => 40,
                    2 => 32,
                    3 => 28,
                    4 => 26,
                    _ => 24,
                };
                let mut inner = String::new();
                emit_inline(child, &mut inner, Style::bold());
                if !inner.trim().is_empty() {
                    out.push_str(&format!("{{\\fs{size}\\b {inner}\\b0\\fs24}}\\par\n"));
                }
            }
            NodeValue::Paragraph => {
                let mut inner = String::new();
                emit_inline(child, &mut inner, Style::default());
                if !inner.trim().is_empty() {
                    out.push_str(&inner);
                    out.push_str("\\par\n");
                }
            }
            NodeValue::List(list) => {
                render_list(child, list.list_type, list.start, out, depth);
            }
            NodeValue::CodeBlock(cb) => {
                let text = rtf_escape(cb.literal.trim_end());
                if !text.is_empty() {
                    out.push_str(&format!("{{\\f1\\fs22 {text}}}\\par\n"));
                }
            }
            NodeValue::BlockQuote => {
                let mut inner = String::new();
                body_blocks(child, &mut inner, depth);
                out.push_str(&format!("{{\\li720\\cf2 {inner}}}\\par\n"));
            }
            NodeValue::ThematicBreak => out.push_str("{\\brdrb\\brdrs\\par}\\par\n"),
            NodeValue::Table(_) => render_table(child, out),
            _ => body_blocks(child, out, depth),
        }
    }
}

fn render_list<'a>(node: &'a AstNode<'a>, list_type: ListType, start: usize, out: &mut String, depth: usize) {
    let mut number = start;
    let indent = "  ".repeat(depth);
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
                NodeValue::List(inner) => {
                    render_list(child, inner.list_type, inner.start, out, depth + 1);
                }
                _ => {
                    let mut inner = String::new();
                    emit_inline(child, &mut inner, Style::default());
                    if inner.trim().is_empty() {
                        continue;
                    }
                    let prefix = if first {
                        format!("{indent}{marker}")
                    } else {
                        format!("{indent}   ")
                    };
                    out.push_str(&format!("{prefix}{inner}\\par\n"));
                    first = false;
                }
            }
        }
    }
}

fn render_table<'a>(node: &'a AstNode<'a>, out: &mut String) {
    let rows: Vec<Vec<String>> = node
        .children()
        .map(|row| {
            row.children()
                .map(|cell| {
                    let mut c = String::new();
                    emit_inline(cell, &mut c, Style::default());
                    c.trim().to_string()
                })
                .collect()
        })
        .collect();
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if cols == 0 {
        return;
    }
    let cellw = 9000 / cols as i32;
    out.push_str("\\trowd\\trqc\\trbrdrt\\brdrs\\trbrdrl\\brdrs\\trbrdrb\\brdrs\\trbrdrr\\brdrs\n");
    let mut pos = 0;
    for _ in 0..cols {
        pos += cellw;
        out.push_str(&format!("\\clbrdrt\\brdrs\\clbrdrl\\brdrs\\clbrdrb\\brdrs\\clbrdrr\\brdrs\\cellx{pos}"));
    }
    out.push('\n');
    for (ri, row) in rows.iter().enumerate() {
        for cell in row.iter() {
            let style = if ri == 0 { "\\b " } else { "" };
            let after = if ri == 0 { "\\b0 " } else { "" };
            out.push_str(&format!("{{\\intbl {style}{}{after}}}\\cell\n", rtf_escape(cell)));
        }
        out.push_str("\\row\n");
    }
}

#[derive(Clone, Copy, Default)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
}

impl Style {
    const fn bold() -> Self {
        Style { bold: true, italic: false, strike: false }
    }
    fn prefix(self) -> &'static str {
        // Group by single combined token to minimize control-word noise.
        match (self.bold, self.italic, self.strike) {
            (true, false, false) => "\\b ",
            (false, true, false) => "\\i ",
            (false, false, true) => "\\strike ",
            (true, true, false) => "\\b\\i ",
            (true, false, true) => "\\b\\strike ",
            (false, true, true) => "\\i\\strike ",
            (true, true, true) => "\\b\\i\\strike ",
            (false, false, false) => "",
        }
    }
    fn suffix(self) -> &'static str {
        match (self.bold, self.italic, self.strike) {
            (true, false, false) => "\\b0 ",
            (false, true, false) => "\\i0 ",
            (false, false, true) => "\\strike0 ",
            (true, true, false) => "\\b0\\i0 ",
            (true, false, true) => "\\b0\\strike0 ",
            (false, true, true) => "\\i0\\strike0 ",
            (true, true, true) => "\\b0\\i0\\strike0 ",
            (false, false, false) => "",
        }
    }
}

fn span(out: &mut String, body: &str, style: Style) {
    let p = style.prefix();
    if p.is_empty() {
        out.push_str(body);
    } else {
        out.push('{');
        out.push_str(p);
        out.push_str(body);
        out.push_str(style.suffix());
        out.push('}');
    }
}

fn emit_inline<'a>(node: &'a AstNode<'a>, out: &mut String, style: Style) {
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Text(t) => span(out, &rtf_escape(t), style),
            NodeValue::Code(c) => {
                out.push_str(&format!("{{\\f1 {}}}", rtf_escape(&c.literal)));
            }
            NodeValue::Emph => emit_inline(child, out, Style { italic: true, ..style }),
            NodeValue::Strong => emit_inline(child, out, Style { bold: true, ..style }),
            NodeValue::Strikethrough => emit_inline(child, out, Style { strike: true, ..style }),
            NodeValue::Link(link) => {
                let mut text = String::new();
                emit_inline(child, &mut text, style);
                let text = if text.trim().is_empty() { rtf_escape(&link.url) } else { text };
                let url = rtf_escape_attr(&link.url);
                out.push_str(&format!(
                    "{{\\field{{\\*\\fldinst HYPERLINK \"{url}\"}}{{\\fldrslt\\cf1 {text}}}}}"
                ));
            }
            NodeValue::Image(_) => {
                let alt = inline_collect(child);
                if !alt.trim().is_empty() {
                    out.push_str(&format!("[{}]", rtf_escape(&alt)));
                }
            }
            NodeValue::SoftBreak => out.push(' '),
            NodeValue::LineBreak => out.push_str("\\line "),
            _ => emit_inline(child, out, style),
        }
    }
}

fn inline_collect<'a>(node: &'a AstNode<'a>) -> String {
    let mut s = String::new();
    emit_inline(node, &mut s, Style::default());
    s
}

fn rtf_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\n' => out.push_str("\\line "),
            '\r' => {}
            c if (c as u32) < 0x80 => out.push(c),
            c => {
                let unit = c as u32;
                if unit <= 0xFFFF {
                    out.push_str(&format!("\\u{unit}?"));
                } else {
                    let pair = unit - 0x10000;
                    let hi = 0xD800 + (pair >> 10);
                    let lo = 0xDC00 + (pair & 0x3FF);
                    out.push_str(&format!("\\u{hi}?\\u{lo}?"));
                }
            }
        }
    }
    out
}

fn rtf_escape_attr(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_rtf_document_header() {
        let out = render("# Hi\n\nBody.");
        assert!(out.starts_with("{\\rtf1\\ansi"));
        assert!(out.contains("Times New Roman"));
        assert!(out.ends_with("}\n"));
    }

    #[test]
    fn heading_emitted_bold_and_sized() {
        let out = render("# Title");
        assert!(out.contains("\\fs40\\b"));
        assert!(out.contains("Title"));
    }

    #[test]
    fn bold_italic_inline() {
        let out = render("**b** *i*");
        assert!(out.contains("{\\b b"));
        assert!(out.contains("{\\i i"));
    }

    #[test]
    fn link_emits_hyperlink_field() {
        let out = render("[x](https://e.com)");
        assert!(out.contains("HYPERLINK"));
        assert!(out.contains("https://e.com"));
    }

    #[test]
    fn escapes_braces_and_backslash() {
        let out = render("`{ \\ }`");
        assert!(out.contains("\\{"));
        assert!(out.contains("\\\\"));
        assert!(out.contains("\\}"));
    }

    #[test]
    fn unicode_uses_utf16_units() {
        let out = render("é");
        assert!(out.contains("\\u233?"));
    }
}
