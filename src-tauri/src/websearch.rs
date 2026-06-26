//! Tavily web search — the shared executor behind the assistant's `web_search`
//! tool. Provider-agnostic: both the Anthropic and OpenRouter tool loops in
//! `ai.rs` call [`search`], format the results with [`format_results`], and feed
//! that text back to the model as a tool result so it can ground (and cite) its
//! reply.
//!
//! BYOK: the Tavily key is stored alongside the AI provider keys (keychain in
//! release, `dev-keys.json` in debug) — see `ai.rs`.

use serde::Deserialize;
use serde_json::json;

use crate::error::{Error, Result};

const TAVILY_URL: &str = "https://api.tavily.com/search";
/// Results per query. Five keeps the tool result compact while giving the model
/// enough sources to cross-check and cite.
pub const DEFAULT_MAX_RESULTS: u32 = 5;
/// "advanced" is the highest-relevance depth (2 Tavily credits). Worth it for an
/// LLM grounding its answer; the free tier is 1,000 credits/month.
pub const DEFAULT_DEPTH: &str = "advanced";

/// One web result, trimmed to what the model needs to reason and cite.
#[derive(Debug, Clone, Deserialize)]
pub struct TavilyResult {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct TavilyResponse {
    #[serde(default)]
    results: Vec<TavilyResult>,
}

/// Run a Tavily search. `api_key` is the user's Tavily key; failures (network,
/// auth, quota) surface as `Error::InvalidInput` so the assistant stream can
/// report them. The key is sent in the JSON body — Tavily's REST API accepts it
/// there, which avoids a separate auth-header code path.
pub async fn search(
    api_key: &str,
    query: &str,
    max_results: u32,
    depth: &str,
) -> Result<Vec<TavilyResult>> {
    let body = json!({
        "api_key": api_key,
        "query": query,
        "search_depth": depth,
        "max_results": max_results,
        "include_answer": false,
    });
    let response = crate::ai::http_client()
        .post(TAVILY_URL)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::InvalidInput(format!("web search request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                v["error"]
                    .as_str()
                    .or_else(|| v["detail"]["error"].as_str())
                    .map(String::from)
            })
            .unwrap_or(text);
        return Err(Error::InvalidInput(format!("Tavily error {status}: {detail}")));
    }

    let parsed: TavilyResponse = response
        .json()
        .await
        .map_err(|e| Error::InvalidInput(format!("could not parse Tavily response: {e}")))?;
    Ok(parsed.results)
}

/// Render results into the text block handed back to the model as a tool result.
/// Each entry leads with `title — url` so the model has the source link to cite
/// inline, followed by the snippet. Empty results return an explicit message so
/// the model knows the search ran but found nothing (rather than hallucinating).
pub fn format_results(query: &str, results: &[TavilyResult]) -> String {
    if results.is_empty() {
        return format!("No web results found for \"{query}\".");
    }
    let mut out = format!("Web search results for \"{query}\":\n");
    for (i, r) in results.iter().enumerate() {
        out.push_str(&format!(
            "\n[{n}] {title} — {url}\n{content}\n",
            n = i + 1,
            title = r.title,
            url = r.url,
            content = r.content,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_includes_titles_and_urls() {
        let results = vec![
            TavilyResult {
                title: "Rust 2.0 announced".into(),
                url: "https://example.com/rust2".into(),
                content: "The Rust team announced…".into(),
            },
            TavilyResult {
                title: "Reactions".into(),
                url: "https://example.com/reactions".into(),
                content: "Developers responded…".into(),
            },
        ];
        let text = format_results("rust 2.0", &results);
        // both titles and both URLs must be present so the model can cite them
        assert!(text.contains("Rust 2.0 announced"));
        assert!(text.contains("https://example.com/rust2"));
        assert!(text.contains("https://example.com/reactions"));
        assert!(text.contains("rust 2.0"));
    }

    #[test]
    fn format_empty_results_is_explicit() {
        let text = format_results("obscure query", &[]);
        assert!(text.contains("No web results"));
        assert!(text.contains("obscure query"));
    }
}
