//! Clean, semantic, self-contained HTML document — paste-ready for CMSes or
//! usable as a standalone page. Same comrak options as the preview.

pub fn render(content: &str, title: &str) -> String {
    let body = crate::preview::render_html(content);
    let title = html_escape(title);
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
{STYLE}
</style>
</head>
<body>
<article>
{body}</article>
</body>
</html>
"#
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

const STYLE: &str = r#"  body {
    margin: 0 auto;
    max-width: 42rem;
    padding: 2rem 1.25rem 4rem;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
    font-size: 1.0625rem;
    line-height: 1.65;
    color: #1f2328;
    background: #ffffff;
  }
  h1, h2, h3, h4, h5, h6 { line-height: 1.25; margin: 1.8em 0 0.6em; }
  h1 { font-size: 2rem; margin-top: 0; }
  h2 { font-size: 1.5rem; }
  h3 { font-size: 1.2rem; }
  p, ul, ol { margin: 0 0 1em; }
  a { color: #0969da; }
  blockquote {
    margin: 0 0 1em;
    padding: 0.1em 1.25em;
    border-left: 4px solid #d0d7de;
    color: #57606a;
  }
  code {
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
    font-size: 0.9em;
    background: #f6f8fa;
    border-radius: 4px;
    padding: 0.15em 0.35em;
  }
  pre {
    background: #f6f8fa;
    border-radius: 8px;
    padding: 1em;
    overflow-x: auto;
    margin: 0 0 1em;
  }
  pre code { background: none; padding: 0; font-size: 0.875em; }
  table { border-collapse: collapse; margin: 0 0 1em; width: 100%; }
  th, td { border: 1px solid #d0d7de; padding: 0.4em 0.75em; text-align: left; }
  th { background: #f6f8fa; }
  img { max-width: 100%; }
  hr { border: none; border-top: 1px solid #d0d7de; margin: 2em 0; }
  @media (prefers-color-scheme: dark) {
    body { color: #e6edf3; background: #0d1117; }
    a { color: #4493f8; }
    blockquote { border-color: #30363d; color: #8d96a0; }
    code, pre, th { background: #161b22; }
    th, td { border-color: #30363d; }
    hr { border-color: #30363d; }
  }"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_document_with_gfm() {
        let html = render("# Hi\n\n- [x] done\n\n| a |\n| - |\n| 1 |", "My Doc");
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("<title>My Doc</title>"));
        assert!(html.contains("<h1>Hi</h1>"));
        assert!(html.contains("<table>"));
    }

    #[test]
    fn title_is_escaped() {
        let html = render("x", "<script>");
        assert!(html.contains("<title>&lt;script&gt;</title>"));
    }
}
