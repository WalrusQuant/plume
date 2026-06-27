pub mod docx;
pub mod html;
pub mod linkedin;
pub mod markdown;
pub mod mastodon;
pub mod bluesky;
pub mod threads;
pub mod plaintext;
pub mod reddit;
pub mod discord;
pub mod richhtml;
pub mod rtf;
pub mod social;
pub mod telegram;
pub mod x;

use comrak::nodes::{AstNode, ListType, NodeValue};

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
    // Clipboard — plain text, platform-specific renderers.
    ExportTarget {
        id: "linkedin",
        label: "LinkedIn post",
        delivery: "clipboard",
        ext: None,
    },
    ExportTarget {
        id: "x-thread",
        label: "X thread",
        delivery: "clipboard",
        ext: None,
    },
    ExportTarget {
        id: "x-article",
        label: "X article (rich)",
        delivery: "clipboard",
        ext: None,
    },
    ExportTarget {
        id: "mastodon",
        label: "Mastodon thread",
        delivery: "clipboard",
        ext: None,
    },
    ExportTarget {
        id: "bluesky",
        label: "Bluesky thread",
        delivery: "clipboard",
        ext: None,
    },
    ExportTarget {
        id: "threads",
        label: "Threads post",
        delivery: "clipboard",
        ext: None,
    },
    ExportTarget {
        id: "reddit",
        label: "Reddit (markdown)",
        delivery: "clipboard",
        ext: None,
    },
    ExportTarget {
        id: "discord",
        label: "Discord (markdown)",
        delivery: "clipboard",
        ext: None,
    },
    ExportTarget {
        id: "telegram",
        label: "Telegram (HTML)",
        delivery: "clipboard",
        ext: None,
    },
    // Clipboard — rich HTML paste into contenteditable composers.
    ExportTarget {
        id: "google-docs",
        label: "Google Docs (rich paste)",
        delivery: "clipboard",
        ext: None,
    },
    ExportTarget {
        id: "newsletter",
        label: "Newsletter / Substack (rich paste)",
        delivery: "clipboard",
        ext: None,
    },
    // File exports.
    ExportTarget {
        id: "markdown",
        label: "Markdown file",
        delivery: "file",
        ext: Some("md"),
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
    ExportTarget {
        id: "rtf",
        label: "Rich text (RTF)",
        delivery: "file",
        ext: Some("rtf"),
    },
    ExportTarget {
        id: "plaintext",
        label: "Plain text file",
        delivery: "file",
        ext: Some("txt"),
    },
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ExportOutput {
    Clipboard { text: String },
    /// Rich paste: `html` is the primary flavor, `plain` the fallback.
    ClipboardHtml { html: String, plain: String },
    File { path: String },
    Cancelled,
}

/// Shared plain-text list renderer for clipboard-text targets (LinkedIn, X).
/// Walks a comrak list node and produces indented `"• "` / `"1. "` lines,
/// recursing into nested sub-lists. The `inline` fn maps each item's child AST
/// node to a text string (each target has its own styling rules).
/// `indent_unit` is repeated per depth level (3 spaces for LinkedIn, 2 for X).
pub(crate) fn render_plain_list<'a>(
    list_node: &'a AstNode<'a>,
    list_type: ListType,
    start: usize,
    depth: usize,
    indent_unit: &str,
    inline: fn(&'a AstNode<'a>) -> String,
) -> String {
    let indent = indent_unit.repeat(depth);
    let cont_indent = " ".repeat(indent_unit.len() + 2); // align under marker text
    let mut number = start;
    let mut out = String::new();
    for item in list_node.children() {
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
                    out.push_str(&render_plain_list(
                        child,
                        inner.list_type,
                        inner.start,
                        depth + 1,
                        indent_unit,
                        inline,
                    ));
                    out.push('\n');
                }
                _ => {
                    let prefix = if first {
                        format!("{indent}{marker}")
                    } else {
                        format!("{indent}{cont_indent}")
                    };
                    out.push_str(&prefix);
                    out.push_str(&inline(child));
                    out.push('\n');
                    first = false;
                }
            }
        }
    }
    out.trim_end().to_string()
}
