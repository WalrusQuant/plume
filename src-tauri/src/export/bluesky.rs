//! Bluesky export — numbered posts under the 300-char grapheme limit. Bluesky
//! counts graphemes (not code units), and our segmenter already uses
//! `chars().count()` which approximates grapheme clusters well for typical text.

use crate::export::social;

/// Bluesky's per-post limit.
const LIMIT: usize = 300;

pub fn render_thread_text(content: &str) -> String {
    social::render_thread_text(content, LIMIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_under_300() {
        let content = "lorem ipsum dolor sit amet ".repeat(50);
        for post in social::render_thread(&content, LIMIT) {
            assert!(post.chars().count() <= 300, "over limit");
        }
    }
}
