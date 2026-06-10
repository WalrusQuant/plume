//! Markdown AST → .docx via docx-rs (real document structure, not the
//! HTML-import shortcut — decision 4).

use std::io::Cursor;

use comrak::nodes::{AstNode, ListType, NodeValue};
use docx_rs::{Docx, LineSpacing, Paragraph, Run, RunFonts, Table, TableCell, TableRow};

use crate::error::{Error, Result};

const BODY_FONT: &str = "Calibri";
const MONO_FONT: &str = "Courier New";
const BODY_SIZE: usize = 22; // half-points: 11pt

/// Body paragraph with normal spacing after (units: 1/20 pt; 160 = 8pt).
fn body_paragraph() -> Paragraph {
    Paragraph::new().line_spacing(LineSpacing::new().after(160))
}

fn heading_paragraph() -> Paragraph {
    Paragraph::new().line_spacing(LineSpacing::new().before(280).after(140))
}

#[derive(Debug, Clone, Copy, Default)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
}

pub fn render(content: &str) -> Result<Vec<u8>> {
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, content, &crate::preview::options());

    let mut docx = Docx::new()
        .default_fonts(RunFonts::new().ascii(BODY_FONT))
        .default_size(BODY_SIZE);
    for child in root.children() {
        docx = render_block(docx, child, 0);
    }

    let mut cursor = Cursor::new(Vec::new());
    docx.build()
        .pack(&mut cursor)
        .map_err(|e| Error::InvalidInput(format!("failed to build .docx: {e}")))?;
    Ok(cursor.into_inner())
}

fn heading_size(level: u8) -> usize {
    match level {
        1 => 48, // 24pt
        2 => 36, // 18pt
        3 => 30, // 15pt
        _ => 26, // 13pt
    }
}

fn render_block<'a>(mut docx: Docx, node: &'a AstNode<'a>, indent: usize) -> Docx {
    match &node.data.borrow().value {
        NodeValue::Heading(h) => {
            let mut para = heading_paragraph();
            for run in inline_runs(node, Style { bold: true, ..Style::default() }, heading_size(h.level)) {
                para = para.add_run(run);
            }
            docx.add_paragraph(para)
        }
        NodeValue::Paragraph => {
            let mut para = body_paragraph();
            if indent > 0 {
                para = para.indent(Some((indent * 360) as i32), None, None, None);
            }
            for run in inline_runs(node, Style::default(), BODY_SIZE) {
                para = para.add_run(run);
            }
            docx.add_paragraph(para)
        }
        NodeValue::List(list) => render_list(docx, node, list.list_type, list.start, indent),
        NodeValue::CodeBlock(cb) => {
            for line in cb.literal.trim_end().lines() {
                docx = docx.add_paragraph(
                    Paragraph::new().add_run(
                        Run::new()
                            .add_text(line)
                            .fonts(RunFonts::new().ascii(MONO_FONT))
                            .size(20),
                    ),
                );
            }
            docx.add_paragraph(Paragraph::new())
        }
        NodeValue::BlockQuote => {
            for child in node.children() {
                docx = render_quote_block(docx, child, indent + 1);
            }
            docx
        }
        NodeValue::ThematicBreak => {
            docx.add_paragraph(Paragraph::new().add_run(Run::new().add_text("———").size(BODY_SIZE)))
        }
        NodeValue::Table(_) => {
            let mut rows = Vec::new();
            for row_node in node.children() {
                let header = matches!(&row_node.data.borrow().value, NodeValue::TableRow(true));
                let style = Style { bold: header, ..Style::default() };
                let cells: Vec<TableCell> = row_node
                    .children()
                    .map(|cell| {
                        let mut para = Paragraph::new();
                        for run in inline_runs(cell, style, BODY_SIZE) {
                            para = para.add_run(run);
                        }
                        TableCell::new().add_paragraph(para)
                    })
                    .collect();
                rows.push(TableRow::new(cells));
            }
            docx = docx.add_table(Table::new(rows));
            docx.add_paragraph(body_paragraph())
        }
        _ => {
            for child in node.children() {
                docx = render_block(docx, child, indent);
            }
            docx
        }
    }
}

fn render_quote_block<'a>(docx: Docx, node: &'a AstNode<'a>, indent: usize) -> Docx {
    if matches!(node.data.borrow().value, NodeValue::Paragraph) {
        let mut para = body_paragraph().indent(Some((indent * 360) as i32), None, None, None);
        for run in inline_runs(node, Style { italic: true, ..Style::default() }, BODY_SIZE) {
            para = para.add_run(run);
        }
        docx.add_paragraph(para)
    } else {
        render_block(docx, node, indent)
    }
}

fn render_list<'a>(
    mut docx: Docx,
    list_node: &'a AstNode<'a>,
    list_type: ListType,
    start: usize,
    indent: usize,
) -> Docx {
    let mut number = start;
    for item in list_node.children() {
        let marker = match list_type {
            ListType::Bullet => "•  ".to_string(),
            ListType::Ordered => {
                let m = format!("{number}.  ");
                number += 1;
                m
            }
        };
        let mut first = true;
        for child in item.children() {
            match &child.data.borrow().value {
                NodeValue::List(inner) => {
                    docx = render_list(docx, child, inner.list_type, inner.start, indent + 1);
                }
                NodeValue::Paragraph => {
                    let mut para = Paragraph::new()
                        .line_spacing(LineSpacing::new().after(60))
                        .indent(Some(((indent + 1) * 360) as i32), None, None, None);
                    let mut prefix = Run::new().add_text(if first { marker.clone() } else { "   ".into() });
                    prefix = prefix.size(BODY_SIZE);
                    para = para.add_run(prefix);
                    for run in inline_runs(child, Style::default(), BODY_SIZE) {
                        para = para.add_run(run);
                    }
                    docx = docx.add_paragraph(para);
                    first = false;
                }
                _ => {
                    docx = render_block(docx, child, indent + 1);
                }
            }
        }
    }
    docx
}

fn inline_runs<'a>(node: &'a AstNode<'a>, style: Style, size: usize) -> Vec<Run> {
    let mut runs = Vec::new();
    collect_runs(node, style, size, &mut runs);
    runs
}

fn collect_runs<'a>(node: &'a AstNode<'a>, style: Style, size: usize, runs: &mut Vec<Run>) {
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Text(t) => runs.push(make_run(t, style, size)),
            NodeValue::Code(c) => runs.push(make_run(&c.literal, Style { code: true, ..style }, size)),
            NodeValue::Emph => collect_runs(child, Style { italic: true, ..style }, size, runs),
            NodeValue::Strong => collect_runs(child, Style { bold: true, ..style }, size, runs),
            NodeValue::Strikethrough => {
                collect_runs(child, Style { strike: true, ..style }, size, runs)
            }
            NodeValue::Link(link) => {
                collect_runs(child, style, size, runs);
                runs.push(make_run(&format!(" ({})", link.url), style, size));
            }
            NodeValue::SoftBreak => runs.push(make_run(" ", style, size)),
            NodeValue::LineBreak => runs.push(Run::new().add_break(docx_rs::BreakType::TextWrapping)),
            _ => collect_runs(child, style, size, runs),
        }
    }
}

fn make_run(text: &str, style: Style, size: usize) -> Run {
    let mut run = Run::new().add_text(text).size(size);
    if style.bold {
        run = run.bold();
    }
    if style.italic {
        run = run.italic();
    }
    if style.strike {
        run = run.strike();
    }
    if style.code {
        run = run.fonts(RunFonts::new().ascii(MONO_FONT));
    }
    run
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_valid_zip_bytes() {
        let bytes = render(
            "# Title\n\nSome **bold** and *italic* and `code`.\n\n- a\n- b\n\n> quote\n\n```\nlet x = 1;\n```\n\n| Team | League |\n| --- | --- |\n| Brewers | MLB |",
        )
        .unwrap();
        // .docx is a zip: PK magic
        assert_eq!(&bytes[..2], b"PK");
        assert!(bytes.len() > 1000);
    }
}
