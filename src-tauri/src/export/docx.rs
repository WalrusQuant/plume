//! Markdown AST → .docx via docx-rs (real document structure, not the
//! HTML-import shortcut — decision 4).
//!
//! Round 2 (2026-06-11) adds: embedded images (data-URI / local files),
//! real hyperlinks, Word-native list numbering, task-list checkboxes,
//! real footnotes, table column alignment + borders + header shading,
//! built-in Heading styles (outline/TOC), and code/inline-code shading.
//! Every element comes from the shared comrak AST — fixes live here in the
//! renderer, never by toggling `preview::options()` (shared with live preview).

use std::collections::HashMap;
use std::io::Cursor;

use comrak::nodes::{AstNode, ListType, NodeValue, TableAlignment};
use docx_rs::*;
use image::GenericImageView;

use crate::error::{Error, Result};

const BODY_FONT: &str = "Calibri";
const MONO_FONT: &str = "Consolas";
const BODY_SIZE: usize = 22; // half-points: 11pt
const CODE_SIZE: usize = 20; // 10pt
const LINK_COLOR: &str = "0563C1"; // Word's default hyperlink blue
const CODE_FILL: &str = "F2F2F2"; // light grey behind code
const HEADER_FILL: &str = "D9D9D9"; // table header shading
const RULE_COLOR: &str = "BFBFBF"; // thematic-break line
const EMU_PER_PX: u32 = 9525; // 1px @ 96dpi
const MAX_IMG_PX: u32 = 600; // cap embedded image width (~6.25in)
const CONTENT_WIDTH_DXA: usize = 9350; // ≈6.5in content width (Letter, 1in margins)

/// Inline character formatting accumulated while walking the AST. (Named `Fmt`
/// so it doesn't collide with docx-rs's own `Style` from the glob import.)
#[derive(Debug, Clone, Copy, Default)]
struct Fmt {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
}

/// Footnote definitions, keyed by name, resolved at reference sites instead of
/// being rendered as trailing body text.
type Footnotes<'a> = HashMap<String, &'a AstNode<'a>>;

fn body_fonts() -> RunFonts {
    RunFonts::new().ascii(BODY_FONT).hi_ansi(BODY_FONT).cs(BODY_FONT)
}

fn mono_fonts() -> RunFonts {
    RunFonts::new().ascii(MONO_FONT).hi_ansi(MONO_FONT).cs(MONO_FONT)
}

/// Body paragraph with normal spacing after (units: 1/20 pt; 160 = 8pt).
fn body_paragraph() -> Paragraph {
    Paragraph::new().line_spacing(LineSpacing::new().after(160))
}

fn heading_paragraph() -> Paragraph {
    Paragraph::new().line_spacing(LineSpacing::new().before(280).after(140))
}

/// half-points (24pt = 48).
fn heading_size(level: u8) -> usize {
    match level {
        1 => 48, // 24pt
        2 => 36, // 18pt
        3 => 30, // 15pt
        4 => 26, // 13pt
        5 => 24, // 12pt
        _ => 22, // 11pt
    }
}

pub fn render(content: &str) -> Result<Vec<u8>> {
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, content, &crate::preview::options());

    // Collect footnote definitions up front; they render at their reference
    // sites (real Word footnotes), not as body text at the end of the doc.
    let mut footnotes: Footnotes = HashMap::new();
    for node in root.children() {
        if let NodeValue::FootnoteDefinition(def) = &node.data.borrow().value {
            footnotes.insert(def.name.clone(), node);
        }
    }

    let mut docx = Docx::new().default_fonts(body_fonts()).default_size(BODY_SIZE);
    docx = register_heading_styles(docx);

    // Word numbering definitions are allocated lazily, one per list node.
    let mut next_num_id: usize = 1;
    for child in root.children() {
        docx = render_block(docx, child, 0, &mut next_num_id, &footnotes);
    }

    let mut cursor = Cursor::new(Vec::new());
    docx.build()
        .pack(&mut cursor)
        .map_err(|e| Error::InvalidInput(format!("failed to build .docx: {e}")))?;
    Ok(cursor.into_inner())
}

/// Register Heading1..6 as real Word styles so they map to the outline / TOC.
fn register_heading_styles(mut docx: Docx) -> Docx {
    for level in 1u8..=6 {
        let style = Style::new(format!("Heading{level}"), StyleType::Paragraph)
            .name(format!("heading {level}"))
            .bold()
            .size(heading_size(level))
            .outline_lvl((level - 1) as usize);
        docx = docx.add_style(style);
    }
    docx
}

fn render_block<'a>(
    mut docx: Docx,
    node: &'a AstNode<'a>,
    indent: usize,
    num_id: &mut usize,
    fns: &Footnotes<'a>,
) -> Docx {
    match &node.data.borrow().value {
        NodeValue::Heading(h) => {
            let level = h.level.min(6);
            let para = heading_paragraph().style(&format!("Heading{level}"));
            let para = add_inlines(para, node, Fmt::default(), heading_size(level), fns);
            docx.add_paragraph(para)
        }
        NodeValue::Paragraph => {
            let mut para = body_paragraph();
            if indent > 0 {
                para = para.indent(Some((indent * 360) as i32), None, None, None);
            }
            let para = add_inlines(para, node, Fmt::default(), BODY_SIZE, fns);
            docx.add_paragraph(para)
        }
        NodeValue::List(list) => {
            render_list(docx, node, list.list_type, list.start, indent, num_id, fns)
        }
        NodeValue::CodeBlock(cb) => render_code_block(docx, &cb.literal),
        NodeValue::BlockQuote => {
            for child in node.children() {
                docx = render_quote_block(docx, child, indent + 1, num_id, fns);
            }
            docx
        }
        NodeValue::ThematicBreak => render_hr(docx),
        NodeValue::Table(t) => render_table(docx, node, &t.alignments, fns),
        // Definitions are rendered at their reference sites, not as body text.
        NodeValue::FootnoteDefinition(_) => docx,
        _ => {
            for child in node.children() {
                docx = render_block(docx, child, indent, num_id, fns);
            }
            docx
        }
    }
}

fn render_quote_block<'a>(
    docx: Docx,
    node: &'a AstNode<'a>,
    indent: usize,
    num_id: &mut usize,
    fns: &Footnotes<'a>,
) -> Docx {
    if matches!(node.data.borrow().value, NodeValue::Paragraph) {
        let para = body_paragraph().indent(Some((indent * 360) as i32), None, None, None);
        let para = add_inlines(para, node, Fmt { italic: true, ..Fmt::default() }, BODY_SIZE, fns);
        docx.add_paragraph(para)
    } else {
        render_block(docx, node, indent, num_id, fns)
    }
}

// ---- Lists --------------------------------------------------------------

fn bullet_glyph(depth: usize) -> &'static str {
    match depth % 3 {
        0 => "•",
        1 => "◦",
        _ => "▪",
    }
}

/// One abstract-numbering definition per list node: a single level whose
/// indent encodes the nesting depth. Each list gets its own id so ordered
/// lists restart at their own `start` and nested lists count independently.
fn build_abstract_numbering(
    id: usize,
    list_type: ListType,
    start: usize,
    depth: usize,
) -> AbstractNumbering {
    let left = ((depth + 1) * 720) as i32; // 0.5in per level
    let (fmt, text) = match list_type {
        ListType::Bullet => ("bullet", bullet_glyph(depth).to_string()),
        ListType::Ordered => ("decimal", "%1.".to_string()),
    };
    AbstractNumbering::new(id).add_level(
        Level::new(
            0,
            Start::new(start),
            NumberFormat::new(fmt),
            LevelText::new(text),
            LevelJc::new("left"),
        )
        .indent(Some(left), Some(SpecialIndentType::Hanging(360)), None, None),
    )
}

fn render_list<'a>(
    mut docx: Docx,
    list_node: &'a AstNode<'a>,
    list_type: ListType,
    start: usize,
    depth: usize,
    num_id: &mut usize,
    fns: &Footnotes<'a>,
) -> Docx {
    // A GFM task list is a bullet list whose items are checkbox items; render
    // checkboxes instead of bullets.
    let is_task = list_node
        .children()
        .any(|item| matches!(&item.data.borrow().value, NodeValue::TaskItem(_)));
    if is_task {
        return render_task_list(docx, list_node, depth, num_id, fns);
    }

    let id = *num_id;
    *num_id += 1;
    docx = docx
        .add_abstract_numbering(build_abstract_numbering(id, list_type, start, depth))
        .add_numbering(Numbering::new(id, id));

    for item in list_node.children() {
        let mut first = true;
        for child in item.children() {
            match &child.data.borrow().value {
                NodeValue::List(inner) => {
                    docx = render_list(docx, child, inner.list_type, inner.start, depth + 1, num_id, fns);
                }
                NodeValue::Paragraph => {
                    let mut para = Paragraph::new().line_spacing(LineSpacing::new().after(60));
                    if first {
                        para = para.numbering(NumberingId::new(id), IndentLevel::new(0));
                    } else {
                        para = para.indent(Some(((depth + 1) * 720) as i32), None, None, None);
                    }
                    let para = add_inlines(para, child, Fmt::default(), BODY_SIZE, fns);
                    docx = docx.add_paragraph(para);
                    first = false;
                }
                _ => docx = render_block(docx, child, depth + 1, num_id, fns),
            }
        }
    }
    docx
}

fn render_task_list<'a>(
    mut docx: Docx,
    list_node: &'a AstNode<'a>,
    depth: usize,
    num_id: &mut usize,
    fns: &Footnotes<'a>,
) -> Docx {
    for item in list_node.children() {
        let checked = matches!(
            &item.data.borrow().value,
            NodeValue::TaskItem(t) if t.symbol.is_some()
        );
        let mut first = true;
        for child in item.children() {
            match &child.data.borrow().value {
                NodeValue::List(inner) => {
                    docx = render_list(docx, child, inner.list_type, inner.start, depth + 1, num_id, fns);
                }
                NodeValue::Paragraph => {
                    let mut para = Paragraph::new()
                        .line_spacing(LineSpacing::new().after(60))
                        .indent(
                            Some(((depth + 1) * 720) as i32),
                            Some(SpecialIndentType::Hanging(360)),
                            None,
                            None,
                        );
                    if first {
                        let glyph = if checked { "☑  " } else { "☐  " };
                        para = para.add_run(
                            Run::new().add_text(glyph).fonts(body_fonts()).size(BODY_SIZE),
                        );
                    }
                    let para = add_inlines(para, child, Fmt::default(), BODY_SIZE, fns);
                    docx = docx.add_paragraph(para);
                    first = false;
                }
                _ => docx = render_block(docx, child, depth + 1, num_id, fns),
            }
        }
    }
    docx
}

// ---- Block elements rendered as single-cell tables ----------------------

fn render_code_block(docx: Docx, literal: &str) -> Docx {
    let mut cell = TableCell::new()
        .set_borders(TableCellBorders::with_empty())
        .shading(Shading::new().shd_type(ShdType::Clear).color("auto").fill(CODE_FILL));
    let mut any = false;
    for line in literal.trim_end_matches('\n').split('\n') {
        // Word collapses an empty paragraph's height; a space keeps blank lines.
        let text = if line.is_empty() { " " } else { line };
        cell = cell.add_paragraph(
            Paragraph::new()
                .line_spacing(LineSpacing::new().after(0))
                .add_run(Run::new().add_text(text).fonts(mono_fonts()).size(CODE_SIZE)),
        );
        any = true;
    }
    if !any {
        cell = cell.add_paragraph(Paragraph::new());
    }
    let table = Table::new(vec![TableRow::new(vec![cell])])
        .set_borders(TableBorders::with_empty())
        .width(CONTENT_WIDTH_DXA, WidthType::Dxa);
    docx.add_table(table).add_paragraph(body_paragraph())
}

fn render_hr(docx: Docx) -> Docx {
    let cell = TableCell::new()
        .set_borders(
            TableCellBorders::with_empty().set(
                TableCellBorder::new(TableCellBorderPosition::Bottom)
                    .color(RULE_COLOR)
                    .size(4),
            ),
        )
        .add_paragraph(Paragraph::new().line_spacing(LineSpacing::new().after(0)));
    let table = Table::new(vec![TableRow::new(vec![cell])])
        .set_borders(TableBorders::with_empty())
        .width(CONTENT_WIDTH_DXA, WidthType::Dxa);
    docx.add_table(table).add_paragraph(body_paragraph())
}

// ---- Tables -------------------------------------------------------------

fn alignment_type(a: TableAlignment) -> Option<AlignmentType> {
    match a {
        TableAlignment::Left => Some(AlignmentType::Left),
        TableAlignment::Center => Some(AlignmentType::Center),
        TableAlignment::Right => Some(AlignmentType::Right),
        TableAlignment::None => None,
    }
}

fn render_table<'a>(
    docx: Docx,
    table_node: &'a AstNode<'a>,
    alignments: &[TableAlignment],
    fns: &Footnotes<'a>,
) -> Docx {
    let mut rows = Vec::new();
    for row_node in table_node.children() {
        let header = matches!(&row_node.data.borrow().value, NodeValue::TableRow(true));
        let fmt = Fmt { bold: header, ..Fmt::default() };
        let mut cells = Vec::new();
        for (col, cell_node) in row_node.children().enumerate() {
            let mut para = Paragraph::new();
            if let Some(a) = alignments.get(col).copied().and_then(alignment_type) {
                para = para.align(a);
            }
            let para = add_inlines(para, cell_node, fmt, BODY_SIZE, fns);
            let mut cell = TableCell::new().add_paragraph(para);
            if header {
                cell = cell.shading(
                    Shading::new().shd_type(ShdType::Clear).color("auto").fill(HEADER_FILL),
                );
            }
            cells.push(cell);
        }
        rows.push(TableRow::new(cells));
    }
    let table = Table::new(rows)
        .set_borders(TableBorders::new())
        .width(CONTENT_WIDTH_DXA, WidthType::Dxa);
    docx.add_table(table).add_paragraph(body_paragraph())
}

// ---- Inline runs --------------------------------------------------------

/// Append a node's inline children to `para` as runs / hyperlinks / images /
/// footnote references, in document order.
fn add_inlines<'a>(
    mut para: Paragraph,
    node: &'a AstNode<'a>,
    fmt: Fmt,
    size: usize,
    fns: &Footnotes<'a>,
) -> Paragraph {
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Text(t) => para = para.add_run(make_run(t, fmt, size)),
            NodeValue::Code(c) => {
                para = para.add_run(make_run(&c.literal, Fmt { code: true, ..fmt }, size))
            }
            NodeValue::Emph => {
                para = add_inlines(para, child, Fmt { italic: true, ..fmt }, size, fns)
            }
            NodeValue::Strong => {
                para = add_inlines(para, child, Fmt { bold: true, ..fmt }, size, fns)
            }
            NodeValue::Strikethrough => {
                para = add_inlines(para, child, Fmt { strike: true, ..fmt }, size, fns)
            }
            NodeValue::Link(link) => para = add_link(para, child, &link.url, fmt, size),
            NodeValue::Image(link) => para = add_image(para, child, &link.url, fmt, size),
            NodeValue::FootnoteReference(r) => para = add_footnote(para, &r.name, fmt, size, fns),
            NodeValue::SoftBreak => para = para.add_run(make_run(" ", fmt, size)),
            NodeValue::LineBreak => {
                para = para.add_run(Run::new().add_break(BreakType::TextWrapping))
            }
            _ => para = add_inlines(para, child, fmt, size, fns),
        }
    }
    para
}

fn add_link<'a>(para: Paragraph, node: &'a AstNode<'a>, url: &str, fmt: Fmt, size: usize) -> Paragraph {
    if url.is_empty() {
        // No destination — just emit the link text.
        let mut runs = Vec::new();
        collect_link_runs(node, fmt, size, &mut runs);
        return runs.into_iter().fold(para, |p, r| p.add_run(r));
    }
    let mut hyperlink = Hyperlink::new(url, HyperlinkType::External);
    let mut runs = Vec::new();
    collect_link_runs(node, fmt, size, &mut runs);
    if runs.is_empty() {
        runs.push(make_link_run(url, fmt, size));
    }
    for r in runs {
        hyperlink = hyperlink.add_run(r);
    }
    para.add_hyperlink(hyperlink)
}

/// Flatten a link's inline children into styled (blue + underline) runs.
fn collect_link_runs<'a>(node: &'a AstNode<'a>, fmt: Fmt, size: usize, runs: &mut Vec<Run>) {
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Text(t) => runs.push(make_link_run(t, fmt, size)),
            NodeValue::Code(c) => runs.push(make_link_run(&c.literal, Fmt { code: true, ..fmt }, size)),
            NodeValue::Emph => collect_link_runs(child, Fmt { italic: true, ..fmt }, size, runs),
            NodeValue::Strong => collect_link_runs(child, Fmt { bold: true, ..fmt }, size, runs),
            NodeValue::Strikethrough => collect_link_runs(child, Fmt { strike: true, ..fmt }, size, runs),
            NodeValue::SoftBreak => runs.push(make_link_run(" ", fmt, size)),
            _ => collect_link_runs(child, fmt, size, runs),
        }
    }
}

fn add_image<'a>(para: Paragraph, node: &'a AstNode<'a>, url: &str, fmt: Fmt, size: usize) -> Paragraph {
    // Embed data-URI and local images; remote/unresolvable sources fall back to
    // a real hyperlink so the reference is preserved (a synchronous export must
    // never block on the network — remote fetch is a deferred follow-up).
    if let Some(bytes) = load_image_bytes(url) {
        if let Some((w, h)) = image_dims(&bytes) {
            let (sw, sh) = scaled_emu(w, h);
            let pic = Pic::new(&bytes).size(sw, sh);
            return para.add_run(Run::new().add_image(pic));
        }
    }
    let alt = node_text(node);
    let label = if alt.trim().is_empty() { url.to_string() } else { alt };
    if url.is_empty() || url.starts_with("data:") {
        return para.add_run(make_run(&label, fmt, size));
    }
    let hyperlink =
        Hyperlink::new(url, HyperlinkType::External).add_run(make_link_run(&label, fmt, size));
    para.add_hyperlink(hyperlink)
}

fn add_footnote<'a>(
    para: Paragraph,
    name: &str,
    fmt: Fmt,
    size: usize,
    fns: &Footnotes<'a>,
) -> Paragraph {
    let Some(def) = fns.get(name) else {
        // Unknown reference — keep the marker visible rather than dropping it.
        return para.add_run(make_run(&format!("[^{name}]"), fmt, size));
    };
    let mut fnote = Footnote::new();
    let mut added = false;
    for block in def.children() {
        let is_para = matches!(&block.data.borrow().value, NodeValue::Paragraph);
        let p = if is_para {
            add_inlines(Paragraph::new(), block, Fmt::default(), BODY_SIZE, fns)
        } else {
            // Word footnotes take paragraphs; a list/code/etc. block would be
            // dropped, so flatten it to text rather than lose the content.
            let text = node_text(block);
            if text.trim().is_empty() {
                continue;
            }
            Paragraph::new().add_run(make_run(&text, Fmt::default(), BODY_SIZE))
        };
        fnote = fnote.add_content(p);
        added = true;
    }
    if !added {
        fnote = fnote.add_content(Paragraph::new());
    }
    para.add_run(Run::new().add_footnote_reference(fnote))
}

// ---- Image helpers ------------------------------------------------------

fn load_image_bytes(url: &str) -> Option<Vec<u8>> {
    if let Some(rest) = url.strip_prefix("data:") {
        // data:[<mediatype>][;base64],<data>
        let comma = rest.find(',')?;
        let meta = &rest[..comma];
        let data = &rest[comma + 1..];
        if meta.contains("base64") {
            use base64::Engine;
            return base64::engine::general_purpose::STANDARD.decode(data).ok();
        }
        return None; // non-base64 (percent-encoded) data URLs aren't supported
    }
    let path = url.strip_prefix("file://").unwrap_or(url);
    // Only absolute local paths resolve — the doc has no on-disk base for
    // relative paths (it lives in SQLite).
    if path.starts_with('/') {
        return std::fs::read(path).ok();
    }
    None
}

/// Validate the bytes are a decodable image and return its pixel dimensions.
/// (Also guards `Pic::new`, which panics on a non-image buffer.)
fn image_dims(bytes: &[u8]) -> Option<(u32, u32)> {
    image::load_from_memory(bytes).ok().map(|img| img.dimensions())
}

fn scaled_emu(w: u32, h: u32) -> (u32, u32) {
    let (w, h) = (w.max(1), h.max(1));
    if w <= MAX_IMG_PX {
        (w * EMU_PER_PX, h * EMU_PER_PX)
    } else {
        let nh = ((h as u64 * MAX_IMG_PX as u64) / w as u64).max(1) as u32;
        (MAX_IMG_PX * EMU_PER_PX, nh * EMU_PER_PX)
    }
}

fn node_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut s = String::new();
    collect_text(node, &mut s);
    s
}

fn collect_text<'a>(node: &'a AstNode<'a>, s: &mut String) {
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Text(t) => s.push_str(t),
            NodeValue::Code(c) => s.push_str(&c.literal),
            _ => collect_text(child, s),
        }
    }
}

// ---- Run construction ---------------------------------------------------

fn make_run(text: &str, fmt: Fmt, size: usize) -> Run {
    let mut run = Run::new().add_text(text).size(size);
    if fmt.bold {
        run = run.bold();
    }
    if fmt.italic {
        run = run.italic();
    }
    if fmt.strike {
        run = run.strike();
    }
    if fmt.code {
        run = run
            .fonts(mono_fonts())
            .shading(Shading::new().shd_type(ShdType::Clear).color("auto").fill(CODE_FILL));
    }
    run
}

fn make_link_run(text: &str, fmt: Fmt, size: usize) -> Run {
    make_run(text, fmt, size).color(LINK_COLOR).underline("single")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// Inflate one entry from the produced .docx (a zip) to a UTF-8 string.
    fn entry(bytes: &[u8], name: &str) -> Option<String> {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).unwrap();
        let mut f = zip.by_name(name).ok()?;
        let mut s = String::new();
        f.read_to_string(&mut s).unwrap();
        Some(s)
    }

    fn document(bytes: &[u8]) -> String {
        entry(bytes, "word/document.xml").expect("document.xml present")
    }

    #[test]
    fn produces_valid_zip_bytes() {
        let bytes = render(
            "# Title\n\nSome **bold** and *italic* and `code`.\n\n- a\n- b\n\n> quote\n\n```\nlet x = 1;\n```\n\n| Team | League |\n| --- | --- |\n| Brewers | MLB |",
        )
        .unwrap();
        assert_eq!(&bytes[..2], b"PK");
        assert!(bytes.len() > 1000);
    }

    #[test]
    fn headings_use_built_in_styles() {
        let bytes = render("# H1\n\n## H2").unwrap();
        let doc = document(&bytes);
        assert!(doc.contains(r#"w:val="Heading1""#));
        assert!(doc.contains(r#"w:val="Heading2""#));
        // the style definitions carry an outline level (TOC/navigation)
        let styles = entry(&bytes, "word/styles.xml").unwrap();
        assert!(styles.contains("Heading1"));
        assert!(styles.contains("outlineLvl"));
    }

    #[test]
    fn ordered_and_bullet_lists_use_word_numbering() {
        let bytes = render("1. one\n2. two\n\n- a\n- b").unwrap();
        let doc = document(&bytes);
        assert!(doc.contains("<w:numPr>"), "paragraphs reference numbering");
        let numbering = entry(&bytes, "word/numbering.xml").expect("numbering.xml present");
        assert!(numbering.contains("decimal"));
        assert!(numbering.contains("bullet"));
        // no literal text markers left behind
        assert!(!doc.contains("1.  "));
        assert!(!doc.contains("•  "));
    }

    #[test]
    fn ordered_list_respects_start() {
        let bytes = render("3. three\n4. four").unwrap();
        let numbering = entry(&bytes, "word/numbering.xml").unwrap();
        assert!(numbering.contains(r#"w:val="3""#), "start value 3 carried into numbering");
    }

    #[test]
    fn task_list_renders_checkboxes() {
        let bytes = render("- [x] done\n- [ ] todo").unwrap();
        let doc = document(&bytes);
        assert!(doc.contains('☑'), "checked box");
        assert!(doc.contains('☐'), "unchecked box");
        // task lists are not numbered
        assert!(!doc.contains("<w:numPr>"));
    }

    #[test]
    fn links_become_real_hyperlinks() {
        let bytes = render("see [Anthropic](https://anthropic.com) now").unwrap();
        let doc = document(&bytes);
        assert!(doc.contains("<w:hyperlink"));
        assert!(!doc.contains("(https://anthropic.com)"), "no literal url fallback");
        let rels = entry(&bytes, "word/_rels/document.xml.rels").unwrap();
        assert!(rels.contains("https://anthropic.com"), "url stored in relationships");
    }

    #[test]
    fn footnotes_are_real_word_footnotes() {
        let bytes = render("Claim.[^1]\n\n[^1]: the evidence").unwrap();
        let doc = document(&bytes);
        assert!(doc.contains("footnoteReference"));
        let notes = entry(&bytes, "word/footnotes.xml").expect("footnotes.xml present");
        assert!(notes.contains("the evidence"));
        // the definition is not duplicated as trailing body text
        assert_eq!(doc.matches("the evidence").count(), 0);
    }

    #[test]
    fn table_alignment_and_header_shading() {
        let bytes = render("| L | R |\n| :--- | ---: |\n| a | b |").unwrap();
        let doc = document(&bytes);
        assert!(doc.contains(r#"w:val="right""#), "right-aligned column");
        assert!(doc.contains(HEADER_FILL), "header row shaded");
    }

    #[test]
    fn code_block_is_shaded() {
        let bytes = render("```\nlet x = 1;\n```").unwrap();
        let doc = document(&bytes);
        assert!(doc.contains(CODE_FILL), "code block has grey fill");
    }

    #[test]
    fn embeds_data_uri_image() {
        // 1x1 transparent PNG
        let png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==";
        let bytes = render(&format!("![pixel]({png})")).unwrap();
        let doc = document(&bytes);
        assert!(doc.contains("<w:drawing>"), "image embedded as a drawing");
        // media part written
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes.clone())).unwrap();
        let has_media = (0..zip.len()).any(|i| zip.by_index(i).unwrap().name().contains("media"));
        assert!(has_media, "image bytes packed into word/media");
    }

    #[test]
    fn remote_image_falls_back_to_hyperlink() {
        let bytes = render("![diagram](https://example.com/a.png)").unwrap();
        let doc = document(&bytes);
        assert!(doc.contains("<w:hyperlink"), "remote image becomes a link");
        let rels = entry(&bytes, "word/_rels/document.xml.rels").unwrap();
        assert!(rels.contains("https://example.com/a.png"));
    }

    #[test]
    fn inline_code_uses_mono_and_shading() {
        let bytes = render("call `foo()` here").unwrap();
        let doc = document(&bytes);
        assert!(doc.contains(MONO_FONT));
        assert!(doc.contains(CODE_FILL));
    }
}

