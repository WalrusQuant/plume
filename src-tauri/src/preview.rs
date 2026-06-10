use comrak::Options;

/// One comrak configuration for the whole app: preview now, AI context and
/// export renderers later (decision 7 — single engine).
/// GFM extensions match the old remark-gfm preview. Raw HTML in the source is
/// escaped (comrak default) — the preview pane renders with full IPC access,
/// so we don't execute author-supplied HTML.
pub fn options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.autolink = true;
    options.extension.footnotes = true;
    // strip YAML frontmatter (skill/agent docs lead with it) instead of
    // rendering it as body text
    options.extension.front_matter_delimiter = Some("---".to_string());
    options
}

pub fn render_html(content: &str) -> String {
    comrak::markdown_to_html(content, &options())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_gfm() {
        let html = render_html("# Hi\n\n- [x] done\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n~~gone~~");
        assert!(html.contains("<h1>Hi</h1>"));
        assert!(html.contains("checkbox"));
        assert!(html.contains("<table>"));
        assert!(html.contains("<del>gone</del>"));
    }

    #[test]
    fn strips_yaml_frontmatter() {
        let html = render_html("---\nname: sports\ndescription: test\n---\n\n# Body");
        assert!(!html.contains("name: sports"));
        assert!(html.contains("<h1>Body</h1>"));
    }

    #[test]
    fn frontmatter_survives_indented_dashes_in_block_scalars() {
        // a block scalar's lines are indented, so an inner "---" must not
        // terminate the frontmatter (a flush-left "---" legitimately does —
        // that's YAML's document separator)
        let html = render_html("---\ndescription: |\n  ---\n  use dashes\nname: x\n---\n\n# Body");
        assert!(!html.contains("name: x"));
        assert!(!html.contains("use dashes"));
        assert!(html.contains("<h1>Body</h1>"));
    }

    #[test]
    fn escapes_raw_html() {
        let html = render_html("<script>alert(1)</script>");
        assert!(!html.contains("<script>"));
    }
}
