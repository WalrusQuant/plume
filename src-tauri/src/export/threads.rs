//! Instagram Threads export — numbered posts under the 500-char limit.

use crate::export::social;

/// Threads' per-post limit.
const LIMIT: usize = 500;

pub fn render_thread_text(content: &str) -> String {
    social::render_thread_text(content, LIMIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_under_500() {
        let content = "lorem ipsum dolor sit amet ".repeat(80);
        for post in social::render_thread(&content, LIMIT) {
            assert!(post.chars().count() <= 500, "over limit");
        }
    }
}
