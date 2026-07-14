//! External document import: read a file from disk, extract its plain text, and
//! (in `commands::import_documents`) turn it into a Plume document that the embed
//! worker then indexes. Supports markdown/plain text (verbatim), PDF
//! (`pdf-extract`), and DOCX (unzip `word/document.xml`, pull `<w:t>` runs).
//!
//! Extraction is best-effort and lossy for rich formats — tables, images, and
//! layout collapse to plain text, which is exactly what retrieval wants.

use std::io::Read;
use std::path::Path;

use crate::error::{Error, Result};

/// Extract plain text from a supported file, dispatching by extension. Errors
/// (unsupported type, unreadable/enc­rypted file) are returned per-file so the
/// caller can skip one bad file without aborting a multi-file import.
pub fn extract_text(path: &Path) -> Result<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "md" | "markdown" | "txt" | "text" => std::fs::read_to_string(path).map_err(Into::into),
        "pdf" => extract_pdf(path),
        "docx" => extract_docx(path),
        other => Err(Error::InvalidInput(format!(
            "unsupported file type: .{other}"
        ))),
    }
}

/// Extract a PDF's text layer. Wrapped in `catch_unwind` because the parser can
/// panic on malformed/encrypted PDFs — a bad file must fail this one import, not
/// crash the app. Scanned/image-only PDFs have no text layer and yield "" (the
/// caller treats empty output as a failure with a helpful message).
fn extract_pdf(path: &Path) -> Result<String> {
    let owned = path.to_path_buf();
    std::panic::catch_unwind(move || pdf_extract::extract_text(&owned))
        .map_err(|_| {
            Error::InvalidInput("couldn't read PDF (it may be malformed or encrypted)".into())
        })?
        .map_err(|e| Error::InvalidInput(format!("couldn't read PDF: {e}")))
}

/// Extract a DOCX by unzipping `word/document.xml` and pulling its text.
fn extract_docx(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| Error::InvalidInput(format!("not a valid .docx: {e}")))?;
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .map_err(|_| Error::InvalidInput("not a valid .docx (missing document.xml)".into()))?
        .read_to_string(&mut xml)?;
    Ok(docx_xml_to_text(&xml))
}

/// Pull readable text out of a WordprocessingML body: `<w:t>` runs become text,
/// paragraph ends (`</w:p>`) and breaks (`<w:br>`/`<w:cr>`) become newlines, tabs
/// become tabs. Everything else (styles, tables markup, images) is ignored.
/// Best-effort: a parse error just returns whatever was collected so far.
fn docx_xml_to_text(xml: &str) -> String {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    let mut out = String::new();
    let mut in_text = false;
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == b"w:t" => in_text = true,
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"w:t" => in_text = false,
                b"w:p" => out.push('\n'),
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"w:br" | b"w:cr" => out.push('\n'),
                b"w:tab" => out.push('\t'),
                _ => {}
            },
            Ok(Event::Text(t)) if in_text => {
                out.push_str(&t.unescape().unwrap_or_default());
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docx_xml_extracts_runs_and_paragraph_breaks() {
        let xml = r#"<w:document><w:body>
            <w:p><w:r><w:t>Hello</w:t></w:r><w:r><w:t xml:space="preserve"> world</w:t></w:r></w:p>
            <w:p><w:r><w:t>Second &amp; last</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let text = docx_xml_to_text(xml);
        assert!(text.contains("Hello world"), "runs join within a paragraph: {text:?}");
        assert!(text.contains("Second & last"), "entities are decoded: {text:?}");
        // Two paragraphs → a newline between them.
        assert!(text.contains("world\nSecond"), "paragraph break inserted: {text:?}");
    }

    #[test]
    fn docx_xml_ignores_non_text_markup() {
        let xml = "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>Title</w:t></w:r></w:p>";
        assert_eq!(docx_xml_to_text(xml), "Title");
    }

    #[test]
    fn unsupported_extension_errors() {
        assert!(extract_text(Path::new("/tmp/whatever.pptx")).is_err());
        assert!(extract_text(Path::new("/tmp/noext")).is_err());
    }

    #[test]
    fn markdown_and_text_files_read_verbatim() {
        let dir = std::env::temp_dir().join(format!("plume-import-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let md = dir.join("note.md");
        std::fs::write(&md, "# Title\n\nSome **body** text.").unwrap();
        assert_eq!(extract_text(&md).unwrap(), "# Title\n\nSome **body** text.");
        let txt = dir.join("plain.txt");
        std::fs::write(&txt, "just words").unwrap();
        assert_eq!(extract_text(&txt).unwrap(), "just words");
        std::fs::remove_dir_all(&dir).ok();
    }
}
