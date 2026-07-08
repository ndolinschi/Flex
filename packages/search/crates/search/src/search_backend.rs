//! Pluggable search backends.
//!
//! The `SearchBackend` trait lets the `search_web` tool query different search
//! engines. The default implementation uses DuckDuckGo's HTML endpoint (no API
//! key required), but consumers can swap in any backend.

use async_trait::async_trait;
use reqwest::Client;

/// A single search result from a backend.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Errors that can occur during a search.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SearchError {
    #[error("search request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("search backend returned no parseable results")]
    NoResults,
    #[error("search backend rate-limited; retry later")]
    RateLimited,
}

/// A pluggable search backend.
#[async_trait]
pub trait SearchBackend: Send + Sync {
    /// Execute a web search and return parsed results.
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError>;
}

// ---------------------------------------------------------------------------
// DuckDuckGo HTML backend
// ---------------------------------------------------------------------------

/// Searches DuckDuckGo's HTML endpoint and parses result blocks.
///
/// Uses `https://html.duckduckgo.com/html/` — no API key is required.
/// The HTML structure is parsed with simple string matching (no regex),
/// extracting title, URL, and snippet from each result block.
pub struct DuckDuckGoBackend {
    client: Client,
}

impl DuckDuckGoBackend {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

impl Default for DuckDuckGoBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SearchBackend for DuckDuckGoBackend {
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        let url = format!("https://html.duckduckgo.com/html/?q={}", urlencoding(query));
        let response = self
            .client
            .get(&url)
            .header(
                reqwest::header::USER_AGENT,
                "Mozilla/5.0 (compatible; DuckDuckGoBot/1.0; +https://duckduckgo.com/duckduckbot)",
            )
            .send()
            .await?;

        // Normalize HTTP errors through reqwest's own error type so we get
        // a consistent error message that includes the status code and URL.
        let response = match response.error_for_status() {
            Ok(r) => r,
            Err(err) => {
                if err.status() == Some(reqwest::StatusCode::TOO_MANY_REQUESTS) {
                    return Err(SearchError::RateLimited);
                }
                return Err(SearchError::Request(err));
            }
        };

        let html = response.text().await?;
        let results = parse_duckduckgo_html(&html);
        if results.is_empty() {
            return Err(SearchError::NoResults);
        }
        Ok(results)
    }
}

/// Percent-encode a query string for a URL.
fn urlencoding(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len() * 3);
    for byte in s.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char);
            }
            b' ' => encoded.push('+'),
            _ => {
                encoded.push('%');
                encoded.push(hex_char(byte >> 4));
                encoded.push(hex_char(byte & 0x0F));
            }
        }
    }
    encoded
}

fn hex_char(nibble: u8) -> char {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    HEX[nibble as usize] as char
}

/// Parse DuckDuckGo HTML results page into `SearchResult` items.
///
/// Result blocks are delimited by `class="result__body"`. Within each block:
/// - Title and URL come from `<a class="result__a" href="URL">TITLE</a>`
/// - Snippet comes from `<a class="result__snippet">...</a>` or
///   `<td class="result-snippet">...</td>`
fn parse_duckduckgo_html(html: &str) -> Vec<SearchResult> {
    let mut results = Vec::new();
    let marker = "result__body";
    let mut search_pos = 0usize;

    while let Some(rel_start) = html[search_pos..].find(marker) {
        let block_start = search_pos + rel_start;

        // Advance past the marker to avoid re-matching.
        let content_start = block_start + marker.len();

        // Find the end of this block: next `class="result "` or end of HTML.
        // The trailing space avoids matching partial class names like `result__a`.
        let block_end = html[content_start..]
            .find("class=\"result \"")
            .map(|p| content_start + p)
            .unwrap_or(html.len());

        let block = &html[block_start..block_end];

        if let Some((title, url)) = extract_result_link(block) {
            // Don't include ad results (they have a different URL pattern
            // and no search-engine value).
            if !is_ad_result(&url) {
                let snippet = extract_snippet(block).unwrap_or_default();
                results.push(SearchResult {
                    title,
                    url,
                    snippet,
                });
            }
        }

        search_pos = block_end;
    }

    results
}

/// Extract title and URL from a `<a class="result__a" href="URL">TITLE</a>` tag.
fn extract_result_link(html: &str) -> Option<(String, String)> {
    let link_marker = "class=\"result__a\"";
    let pos = html.find(link_marker)?;
    let after_marker = &html[pos + link_marker.len()..];

    // Extract href="..."
    let href_start = after_marker.find("href=\"")? + 6;
    let href_end = after_marker[href_start..].find('"')?;
    let url = html_entity_decode(&after_marker[href_start..href_start + href_end]);

    // Find the closing `>` after the `<a ...>` tag and extract inner text.
    let tag_close = after_marker[href_start + href_end..].find('>')?;
    let text_start = href_start + href_end + tag_close + 1;
    let text_end = after_marker[text_start..]
        .find("</a>")
        .unwrap_or(after_marker.len() - text_start);
    let title = strip_html_tags(&after_marker[text_start..text_start + text_end])
        .trim()
        .to_owned();

    if title.is_empty() || url.is_empty() {
        return None;
    }
    Some((title, url))
}

/// Extract the snippet text from a result block.
fn extract_snippet(html: &str) -> Option<String> {
    // Try `class="result__snippet"` first (DuckDuckGo HTML endpoint).
    if let Some(snippet) = extract_tag_content(html, "class=\"result__snippet\"") {
        return Some(snippet);
    }
    // Fall back to the legacy `class="result-snippet"` table-cell format.
    if let Some(snippet) = extract_tag_content(html, "class=\"result-snippet\"") {
        return Some(snippet);
    }
    None
}

/// Extract the text content after a marker attribute until the next `<`.
fn extract_tag_content(html: &str, marker: &str) -> Option<String> {
    let pos = html.find(marker)?;
    let after_marker = &html[pos + marker.len()..];
    let tag_close = after_marker.find('>')?;
    let content_start = tag_close + 1;
    let content_end = after_marker[content_start..]
        .find('<')
        .unwrap_or(after_marker.len() - content_start);
    let raw = &after_marker[content_start..content_start + content_end];
    let decoded = html_entity_decode(raw);
    let cleaned = decoded.trim().to_owned();
    if cleaned.is_empty() {
        return None;
    }
    Some(cleaned)
}

/// Decode common HTML entities in a string.
fn html_entity_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&apos;", "'")
}

/// Strip HTML tags from a string (naive: removes anything between `<` and `>`).
fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    html_entity_decode(&out)
}

/// DuckDuckGo ad results have URLs that start with the ad-redirect domain.
fn is_ad_result(url: &str) -> bool {
    url.contains("duckduckgo.com/y.js") || url.contains("duckduckgo.com/ac.js")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencoding_spaces_and_special_chars() {
        assert_eq!(urlencoding("hello world"), "hello+world");
        assert_eq!(urlencoding("rust&go"), "rust%26go");
        assert_eq!(urlencoding("c++"), "c%2B%2B");
    }

    #[test]
    fn parse_extracts_result_blocks() {
        let html = r#"
        <div class="result results_links results_links_deep web-result">
          <div class="links_main links_deep result__body">
            <a class="result__a" href="https://example.com/rust">The Rust Programming Language</a>
            <div class="result__extras"><div class="result__extras__url"><a class="result__url" href="https://example.com/rust">example.com/rust</a></div></div>
            <a class="result__snippet">A language empowering everyone to build reliable and efficient software.</a>
          </div>
        </div>
        "#;
        let results = parse_duckduckgo_html(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "The Rust Programming Language");
        assert_eq!(results[0].url, "https://example.com/rust");
        assert!(
            results[0].snippet.contains("reliable and efficient"),
            "{}",
            results[0].snippet
        );
    }

    #[test]
    fn parse_skips_ad_results() {
        let html = r#"
        <div class="result results_links results_links_deep web-result">
          <div class="links_main links_deep result__body">
            <a class="result__a" href="https://duckduckgo.com/y.js?u3=...">Ad Title</a>
            <a class="result__snippet">Ad snippet</a>
          </div>
        </div>
        "#;
        let results = parse_duckduckgo_html(html);
        assert!(results.is_empty());
    }

    #[test]
    fn html_entities_are_decoded() {
        assert_eq!(html_entity_decode("hello &amp; world"), "hello & world");
        assert_eq!(html_entity_decode("a &lt; b"), "a < b");
    }
}
