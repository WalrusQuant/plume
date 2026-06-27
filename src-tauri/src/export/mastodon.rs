//! Mastodon export — split into numbered posts under the standard 500-char
//! per-post limit. Most instances use 500; a few raise it. We target the
//! default so the paste works everywhere.

use crate::export::social;

/// Default per-post limit across most Mastodon instances (mastodon.social).
const LIMIT: usize = 500;

pub fn render_thread_text(content: &str) -> String {
    social::render_thread_text(content, LIMIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_post_unnumbered() {
        assert_eq!(social::render_thread("hi", LIMIT), vec!["hi".to_string()]);
    }

    #[test]
    fn long_post_splits_under_500() {
        let content = "lorem ipsum dolor sit amet ".repeat(80); // ~2160 chars
        for post in social::render_thread(&content, LIMIT) {
            assert!(post.chars().count() <= 500, "over limit");
        }
    }
}
