use comrak::Options;
use regex::Regex;
use std::sync::OnceLock;

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

/// Cached options — `render_html` runs per keystroke in the preview.
static OPTIONS: OnceLock<Options<'static>> = OnceLock::new();

/// Matches `href="..."` / `src="..."` (single or double quoted), case-insensitive.
/// Used to sanitize URL schemes so that `javascript:`/`data:` links authored in
/// markdown cannot execute in the preview pane (which has full IPC access).
static URL_ATTR: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r#"(?i)(href|src)\s*=\s*("[^"]*"|'[^']*')"#).unwrap()
});

/// A URL is safe if it has no scheme, or an allowlisted scheme. Browsers strip
/// leading control/whitespace before resolving the scheme, so we mirror that
/// to avoid a `\njava\nscript:` style bypass.
fn url_scheme_is_safe(raw: &str) -> bool {
    let mut s = String::with_capacity(raw.len());
    for c in raw.chars() {
        if c.is_ascii_control() || c == ' ' || c == '\u{a0}' {
            continue;
        }
        s.push(c.to_ascii_lowercase());
    }
    if s.is_empty() || s.starts_with('/') || s.starts_with('#') || s.starts_with('?') {
        return true; // relative URL / fragment
    }
    // data:image/* is acceptable for inline images; other data: (e.g. text/html) is not.
    if s.starts_with("data:image/") {
        return true;
    }
    match s.find(':') {
        None => true, // relative (scheme-less)
        Some(idx) => {
            let scheme = &s[..idx];
            matches!(scheme, "http" | "https" | "mailto" | "tel" | "xmpp")
        }
    }
}

/// Strip dangerous URL schemes from `href`/`src` attributes in rendered HTML.
/// Unsafe URLs are replaced with `#` so the element still renders but
/// navigates nowhere.
fn sanitize_urls(html: &str) -> String {
    URL_ATTR
        .replace_all(html, |caps: &regex::Captures| {
            let attr = caps.get(1).unwrap().as_str();
            let quoted = caps.get(2).unwrap().as_str(); // includes surrounding quotes
            // regex guarantees at least two chars (the quote pair)
            let url = &quoted[1..quoted.len() - 1];
            if url_scheme_is_safe(url) {
                caps.get(0).unwrap().as_str().to_string()
            } else {
                format!("{}=\"#\"", attr)
            }
        })
        .into_owned()
}

pub fn render_html(content: &str) -> String {
    let options = OPTIONS.get_or_init(options);
    let html = comrak::markdown_to_html(content, options);
    sanitize_urls(&html)
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

    #[test]
    fn neutralizes_javascript_link_href() {
        // comrak itself strips dangerous schemes to empty hrefs; our sanitizer
        // is defense-in-depth. Either way the output must be safe.
        let html = render_html("[click](javascript:alert(1))");
        assert!(!html.contains("javascript:"));
        let data = render_html("[x](data:text/html,<b>y</b>)");
        assert!(!data.contains("data:text/html"));
        let vbs = render_html("[x](vbscript:msgbox)");
        assert!(!vbs.contains("vbscript:"));
    }

    #[test]
    fn sanitizer_scheme_check() {
        assert!(!url_scheme_is_safe("javascript:alert(1)"));
        assert!(!url_scheme_is_safe("JavaScript:alert(1)"));
        assert!(!url_scheme_is_safe("java\nscript:alert(1)"));
        assert!(!url_scheme_is_safe("data:text/html,x"));
        assert!(!url_scheme_is_safe("vbscript:x"));
        assert!(!url_scheme_is_safe("file:///etc/passwd"));
        assert!(url_scheme_is_safe("https://example.com/javascript-guide"));
        assert!(url_scheme_is_safe("http://e.com"));
        assert!(url_scheme_is_safe("mailto:a@b.com"));
        assert!(url_scheme_is_safe("tel:+1234"));
        assert!(url_scheme_is_safe("#frag"));
        assert!(url_scheme_is_safe("/rel/path"));
        assert!(url_scheme_is_safe("relative.html"));
        assert!(url_scheme_is_safe("data:image/png;base64,iVBOR="));
        assert!(url_scheme_is_safe(""));
    }

    #[test]
    fn sanitizer_replaces_unsafe_href() {
        let out = sanitize_urls(r#"<a href="javascript:alert(1)">x</a>"#);
        assert!(out.contains("href=\"#\""));
        assert!(!out.contains("javascript"));
        let kept = sanitize_urls(r#"<a href="https://e.com">x</a>"#);
        assert!(kept.contains("https://e.com"));
    }
}
