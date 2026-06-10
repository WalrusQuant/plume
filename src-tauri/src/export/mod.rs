pub mod docx;
pub mod html;
pub mod linkedin;

use serde::Serialize;

/// One comrak parse → AST → each target renders its own output format
/// (decision: one engine for preview, AI context, and export).
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportTarget {
    pub id: &'static str,
    pub label: &'static str,
    pub delivery: &'static str, // "clipboard" | "file"
    pub ext: Option<&'static str>,
}

pub const TARGETS: &[ExportTarget] = &[
    ExportTarget {
        id: "linkedin",
        label: "LinkedIn post",
        delivery: "clipboard",
        ext: None,
    },
    ExportTarget {
        id: "html",
        label: "HTML file",
        delivery: "file",
        ext: Some("html"),
    },
    ExportTarget {
        id: "docx",
        label: "Word document",
        delivery: "file",
        ext: Some("docx"),
    },
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ExportOutput {
    Clipboard { text: String },
    File { path: String },
    Cancelled,
}
