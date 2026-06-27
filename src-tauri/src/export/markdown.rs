//! Raw markdown source — for the `.md` file export and "copy as markdown"
//! clipboard targets (Reddit, Discord). Returns the source verbatim; the app's
//! single-engine invariant means "markdown export" is literally the source
//! text, not a re-rendering.

pub fn render(content: &str) -> &str {
    content
}
