//! Pluggable search backends.
//!
//! The `SearchBackend` trait lets the `search_web` tool query different search
//! engines. The default chain (see [`default_search_backends`]) prefers free,
//! non-scraping APIs that do not rate-limit datacenter IPs the way DuckDuckGo
//! HTML and public SearXNG instances do.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

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
    #[error("search response parse error: {0}")]
    ParseError(String),
}

/// A pluggable search backend.
#[async_trait]
pub trait SearchBackend: Send + Sync {
    /// Execute a web search and return parsed results.
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError>;
}

const BROWSER_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) \
     Chrome/124.0.0.0 Safari/537.36";

fn http_client() -> Client {
    Client::builder()
        .user_agent(BROWSER_UA)
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .unwrap_or_else(|_| Client::new())
}

fn map_status_error(err: reqwest::Error) -> SearchError {
    match err.status() {
        Some(reqwest::StatusCode::TOO_MANY_REQUESTS) => SearchError::RateLimited,
        // HTML scrapers often get 400/403/202 challenge pages from DDG —
        // treat as "no usable results" so the fallback chain can continue
        // instead of surfacing a permanent-feeling rate-limit to the model.
        Some(
            reqwest::StatusCode::BAD_REQUEST
            | reqwest::StatusCode::FORBIDDEN
            | reqwest::StatusCode::ACCEPTED,
        ) => SearchError::NoResults,
        _ => SearchError::Request(err),
    }
}

/// Build the default fallback chain used by [`crate::SearchPlugin`].
///
/// Order:
/// 1. Brave Search — when `BRAVE_SEARCH_API_KEY` is set (best general web results)
/// 2. DuckDuckGo Instant Answer JSON — free, no scrape, rarely rate-limited
/// 3. Wikipedia OpenSearch — free encyclopedia hits
/// 4. DuckDuckGo HTML — last-ditch scrape (often blocked from datacenters)
/// 5. SearXNG — only when `SEARXNG_BASE_URL` is set (public instances 429 constantly)
pub fn default_search_backends() -> Vec<Arc<dyn SearchBackend>> {
    let mut backends: Vec<Arc<dyn SearchBackend>> = Vec::new();

    if let Ok(key) = std::env::var("BRAVE_SEARCH_API_KEY") {
        let key = key.trim();
        if !key.is_empty() {
            backends.push(Arc::new(BraveSearchBackend::new(key.to_owned())));
        }
    }

    backends.push(Arc::new(DuckDuckGoInstantBackend::new()));
    backends.push(Arc::new(WikipediaBackend::new()));
    backends.push(Arc::new(DuckDuckGoBackend::new()));

    if let Ok(url) = std::env::var("SEARXNG_BASE_URL") {
        let url = url.trim();
        if !url.is_empty() {
            backends.push(Arc::new(SearxNGBackend::new(url.to_owned())));
        }
    }

    backends
}

/// Cascading fallback search backend.
///
/// Wraps an ordered list of backends and tries each in sequence until one
/// returns non-empty results. Rate-limits on one backend never short-circuit
/// the chain — only if *every* backend rate-limits do we surface
/// [`SearchError::RateLimited`].
pub struct FallbackSearchBackend {
    backends: Vec<Arc<dyn SearchBackend>>,
}

impl FallbackSearchBackend {
    /// Create a fallback chain from the given backends.
    ///
    /// Backends are tried in order; the first to return non-empty results wins.
    pub fn new(backends: Vec<Arc<dyn SearchBackend>>) -> Self {
        Self { backends }
    }
}

#[async_trait]
impl SearchBackend for FallbackSearchBackend {
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        let mut saw_rate_limit = false;
        let mut last_err: Option<SearchError> = None;
        for backend in self.backends.iter() {
            match backend.search(query).await {
                Ok(results) if !results.is_empty() => {
                    return Ok(results);
                }
                Ok(_) => {
                    last_err = Some(SearchError::NoResults);
                }
                Err(SearchError::RateLimited) => {
                    saw_rate_limit = true;
                    last_err = Some(SearchError::RateLimited);
                }
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }
        if matches!(last_err, Some(SearchError::NoResults)) && saw_rate_limit {
            // Prefer "no results" from Instant Answer / Wikipedia over a
            // misleading rate-limit from a scraped HTML / public SearXNG hop.
            return Err(SearchError::NoResults);
        }
        Err(last_err.unwrap_or(SearchError::NoResults))
    }
}

// ---------------------------------------------------------------------------
// Brave Search (optional, env-gated)
// ---------------------------------------------------------------------------

/// Brave Search API — set `BRAVE_SEARCH_API_KEY` to enable.
///
/// Docs: <https://api.search.brave.com/app/documentation/web-search/get-started>
pub struct BraveSearchBackend {
    client: Client,
    api_key: String,
}

impl BraveSearchBackend {
    pub fn new(api_key: String) -> Self {
        Self {
            client: http_client(),
            api_key,
        }
    }
}

#[derive(Debug, Deserialize)]
struct BraveResponse {
    web: Option<BraveWeb>,
}

#[derive(Debug, Deserialize)]
struct BraveWeb {
    results: Vec<BraveResult>,
}

#[derive(Debug, Deserialize)]
struct BraveResult {
    title: String,
    url: String,
    #[serde(default)]
    description: String,
}

#[async_trait]
impl SearchBackend for BraveSearchBackend {
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        let response = self
            .client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("Accept", "application/json")
            .header("X-Subscription-Token", &self.api_key)
            .query(&[("q", query), ("count", "20")])
            .send()
            .await?;

        let response = match response.error_for_status() {
            Ok(r) => r,
            Err(err) => return Err(map_status_error(err)),
        };

        let parsed: BraveResponse = response
            .json()
            .await
            .map_err(|err| SearchError::ParseError(format!("Brave JSON parse error: {err}")))?;

        let results: Vec<SearchResult> = parsed
            .web
            .map(|w| w.results)
            .unwrap_or_default()
            .into_iter()
            .map(|r| SearchResult {
                title: r.title,
                url: r.url,
                snippet: r.description,
            })
            .filter(|r| !r.url.is_empty())
            .collect();

        if results.is_empty() {
            return Err(SearchError::NoResults);
        }
        Ok(results)
    }
}

// ---------------------------------------------------------------------------
// DuckDuckGo Instant Answer (primary free default)
// ---------------------------------------------------------------------------

/// DuckDuckGo Instant Answer JSON API — no scrape, no API key.
///
/// `https://api.duckduckgo.com/?q=…&format=json` returns abstracts, related
/// topics, and official-site links. It is far more reliable from datacenter
/// IPs than the HTML endpoint (which is usually blocked / 400 / captcha).
pub struct DuckDuckGoInstantBackend {
    client: Client,
}

impl DuckDuckGoInstantBackend {
    pub fn new() -> Self {
        Self {
            client: http_client(),
        }
    }
}

impl Default for DuckDuckGoInstantBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct DdgIaResponse {
    #[serde(default, rename = "Heading")]
    heading: String,
    #[serde(default, rename = "Abstract")]
    abstract_text: String,
    #[serde(default, rename = "AbstractText")]
    abstract_text_alt: String,
    #[serde(default, rename = "AbstractURL")]
    abstract_url: String,
    #[serde(default, rename = "AbstractSource")]
    abstract_source: String,
    #[serde(default, rename = "Results")]
    results: Vec<DdgIaLink>,
    #[serde(default, rename = "RelatedTopics")]
    related_topics: Vec<DdgIaTopic>,
}

#[derive(Debug, Deserialize)]
struct DdgIaLink {
    #[serde(default, rename = "FirstURL")]
    first_url: String,
    #[serde(default, rename = "Text")]
    text: String,
}

#[derive(Debug, Deserialize)]
struct DdgIaTopic {
    #[serde(default, rename = "FirstURL")]
    first_url: Option<String>,
    #[serde(default, rename = "Text")]
    text: Option<String>,
    #[serde(default, rename = "Topics")]
    topics: Option<Vec<DdgIaTopic>>,
    #[serde(default, rename = "Name")]
    #[allow(dead_code)]
    name: Option<String>,
}

fn ddg_ia_results(parsed: DdgIaResponse) -> Vec<SearchResult> {
    let mut out = Vec::new();
    let abstract_body = if !parsed.abstract_text.is_empty() {
        parsed.abstract_text
    } else {
        parsed.abstract_text_alt
    };
    if !parsed.abstract_url.is_empty() && !abstract_body.is_empty() {
        let title = if parsed.heading.is_empty() {
            if parsed.abstract_source.is_empty() {
                parsed.abstract_url.clone()
            } else {
                parsed.abstract_source.clone()
            }
        } else if parsed.abstract_source.is_empty() {
            parsed.heading.clone()
        } else {
            format!("{} ({})", parsed.heading, parsed.abstract_source)
        };
        out.push(SearchResult {
            title,
            url: parsed.abstract_url,
            snippet: abstract_body,
        });
    }
    for link in parsed.results {
        push_ddg_link(&mut out, &link.first_url, &link.text);
    }
    flatten_ddg_topics(&mut out, &parsed.related_topics);
    out
}

fn flatten_ddg_topics(out: &mut Vec<SearchResult>, topics: &[DdgIaTopic]) {
    for topic in topics {
        if let Some(nested) = &topic.topics {
            flatten_ddg_topics(out, nested);
            continue;
        }
        let url = topic.first_url.as_deref().unwrap_or("");
        let text = topic.text.as_deref().unwrap_or("");
        push_ddg_link(out, url, text);
    }
}

fn push_ddg_link(out: &mut Vec<SearchResult>, url: &str, text: &str) {
    let url = url.trim();
    let text = text.trim();
    if url.is_empty() || text.is_empty() {
        return;
    }
    // Instant Answer related topics often point at DDG category pages that
    // are useless to scrape — keep external / wikipedia-style destinations.
    if url.contains("duckduckgo.com/c/") || url.contains("duckduckgo.com/?") {
        return;
    }
    if out.iter().any(|r| r.url == url) {
        return;
    }
    out.push(SearchResult {
        title: text.chars().take(120).collect(),
        url: url.to_owned(),
        snippet: text.to_owned(),
    });
}

#[async_trait]
impl SearchBackend for DuckDuckGoInstantBackend {
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        let response = self
            .client
            .get("https://api.duckduckgo.com/")
            .query(&[
                ("q", query),
                ("format", "json"),
                ("no_html", "1"),
                ("skip_disambig", "1"),
            ])
            .send()
            .await?;

        let response = match response.error_for_status() {
            Ok(r) => r,
            Err(err) => return Err(map_status_error(err)),
        };

        let body = response.text().await?;
        let parsed: DdgIaResponse = serde_json::from_str(&body)
            .map_err(|err| SearchError::ParseError(format!("DuckDuckGo IA JSON parse error: {err}")))?;

        let results = ddg_ia_results(parsed);
        if results.is_empty() {
            return Err(SearchError::NoResults);
        }
        Ok(results)
    }
}

// ---------------------------------------------------------------------------
// Wikipedia OpenSearch
// ---------------------------------------------------------------------------

/// Wikipedia OpenSearch API — free, no key, rarely rate-limited.
pub struct WikipediaBackend {
    client: Client,
}

impl WikipediaBackend {
    pub fn new() -> Self {
        Self {
            client: http_client(),
        }
    }
}

impl Default for WikipediaBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SearchBackend for WikipediaBackend {
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        let response = self
            .client
            .get("https://en.wikipedia.org/w/api.php")
            .query(&[
                ("action", "opensearch"),
                ("search", query),
                ("limit", "8"),
                ("namespace", "0"),
                ("format", "json"),
            ])
            .send()
            .await?;

        let response = match response.error_for_status() {
            Ok(r) => r,
            Err(err) => return Err(map_status_error(err)),
        };

        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|err| SearchError::ParseError(format!("Wikipedia JSON parse error: {err}")))?;

        let results = parse_wikipedia_opensearch(&value)?;
        if results.is_empty() {
            return Err(SearchError::NoResults);
        }
        Ok(results)
    }
}

fn parse_wikipedia_opensearch(value: &serde_json::Value) -> Result<Vec<SearchResult>, SearchError> {
    let arr = value
        .as_array()
        .ok_or_else(|| SearchError::ParseError("Wikipedia opensearch: expected array".into()))?;
    if arr.len() < 4 {
        return Err(SearchError::ParseError(
            "Wikipedia opensearch: expected [query, titles, descriptions, urls]".into(),
        ));
    }
    let titles = arr[1].as_array().cloned().unwrap_or_default();
    let descriptions = arr[2].as_array().cloned().unwrap_or_default();
    let urls = arr[3].as_array().cloned().unwrap_or_default();

    let mut results = Vec::new();
    for i in 0..titles.len().min(urls.len()) {
        let title = titles[i].as_str().unwrap_or("").trim();
        let url = urls[i].as_str().unwrap_or("").trim();
        if title.is_empty() || url.is_empty() {
            continue;
        }
        let snippet = descriptions
            .get(i)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        results.push(SearchResult {
            title: title.to_owned(),
            url: url.to_owned(),
            snippet: if snippet.is_empty() {
                format!("Wikipedia article: {title}")
            } else {
                snippet.to_owned()
            },
        });
    }
    Ok(results)
}

// ---------------------------------------------------------------------------
// DuckDuckGo HTML (legacy scrape — often blocked)
// ---------------------------------------------------------------------------

/// Searches DuckDuckGo's HTML endpoint and parses result blocks.
///
/// Uses `https://html.duckduckgo.com/html/` — no API key is required, but
/// datacenter IPs are frequently blocked (400/captcha). Prefer
/// [`DuckDuckGoInstantBackend`] in the default chain.
pub struct DuckDuckGoBackend {
    client: Client,
}

impl DuckDuckGoBackend {
    pub fn new() -> Self {
        Self {
            client: http_client(),
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
            .header(reqwest::header::ACCEPT, "text/html,application/xhtml+xml")
            .header(reqwest::header::REFERER, "https://duckduckgo.com/")
            .send()
            .await?;

        let response = match response.error_for_status() {
            Ok(r) => r,
            Err(err) => return Err(map_status_error(err)),
        };

        let html = response.text().await?;
        if looks_like_ddg_block_page(&html) {
            return Err(SearchError::NoResults);
        }
        let results = parse_duckduckgo_html(&html);
        if results.is_empty() {
            return Err(SearchError::NoResults);
        }
        Ok(results)
    }
}

fn looks_like_ddg_block_page(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    lower.contains("anomaly-modal")
        || lower.contains("please complete the following challenge")
        || lower.contains("bots use duckduckgo")
        || (!lower.contains("result__body") && lower.contains("challenge"))
}

// ---------------------------------------------------------------------------
// SearXNG (opt-in via SEARXNG_BASE_URL)
// ---------------------------------------------------------------------------

/// A SearXNG backend that queries a public or self-hosted instance.
///
/// Queries the instance at the configured base URL using the JSON API
/// (`/search?format=json&q=...`). Public instances are frequently rate-limited;
/// set `SEARXNG_BASE_URL` to your own instance when you need this hop.
pub struct SearxNGBackend {
    client: Client,
    base_url: String,
}

impl SearxNGBackend {
    /// Create a backend that queries the given SearXNG instance.
    ///
    /// The `base_url` should be the root of the instance (e.g.
    /// `https://searx.example.com`).
    pub fn new(base_url: String) -> Self {
        Self {
            client: http_client(),
            base_url,
        }
    }
}

/// Minimal SearXNG JSON response for parsing.
#[derive(Debug, Deserialize)]
struct SearxNGResponse {
    results: Vec<SearxNGResult>,
}

#[derive(Debug, Deserialize)]
struct SearxNGResult {
    title: String,
    url: String,
    content: Option<String>,
}

#[async_trait]
impl SearchBackend for SearxNGBackend {
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        let url = format!(
            "{}/search?format=json&q={}",
            self.base_url.trim_end_matches('/'),
            urlencoding(query)
        );

        let response = self.client.get(&url).send().await?;

        let response = match response.error_for_status() {
            Ok(r) => r,
            Err(err) => return Err(map_status_error(err)),
        };

        let body = response.text().await?;
        let parsed: SearxNGResponse = serde_json::from_str(&body)
            .map_err(|err| SearchError::ParseError(format!("SearXNG JSON parse error: {err}")))?;

        let results: Vec<SearchResult> = parsed
            .results
            .into_iter()
            .map(|r| SearchResult {
                title: r.title,
                url: r.url,
                snippet: r.content.unwrap_or_default(),
            })
            .collect();

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

        let content_start = block_start + marker.len();

        let block_end = html[content_start..]
            .find("class=\"result \"")
            .map(|p| content_start + p)
            .unwrap_or(html.len());

        let block = &html[block_start..block_end];

        if let Some((title, url)) = extract_result_link(block) {
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

    let href_start = after_marker.find("href=\"")? + 6;
    let href_end = after_marker[href_start..].find('"')?;
    let url = html_entity_decode(&after_marker[href_start..href_start + href_end]);

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
    if let Some(snippet) = extract_tag_content(html, "class=\"result__snippet\"") {
        return Some(snippet);
    }
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

    #[test]
    fn ddg_ia_parses_abstract_and_filters_category_links() {
        let parsed = DdgIaResponse {
            heading: "Rust".into(),
            abstract_text: "A systems language.".into(),
            abstract_text_alt: String::new(),
            abstract_url: "https://en.wikipedia.org/wiki/Rust_(programming_language)".into(),
            abstract_source: "Wikipedia".into(),
            results: vec![DdgIaLink {
                first_url: "https://www.rust-lang.org/".into(),
                text: "Official site".into(),
            }],
            related_topics: vec![
                DdgIaTopic {
                    first_url: Some("https://duckduckgo.com/c/Rust_(programming_language)".into()),
                    text: Some("Category".into()),
                    topics: None,
                    name: None,
                },
                DdgIaTopic {
                    first_url: Some("https://doc.rust-lang.org/book/".into()),
                    text: Some("The Rust Book".into()),
                    topics: None,
                    name: None,
                },
            ],
        };
        let results = ddg_ia_results(parsed);
        assert_eq!(results.len(), 3);
        assert!(results[0].url.contains("wikipedia.org"));
        assert_eq!(results[1].url, "https://www.rust-lang.org/");
        assert_eq!(results[2].url, "https://doc.rust-lang.org/book/");
    }

    #[test]
    fn wikipedia_opensearch_parses_tuple() {
        let value = serde_json::json!([
            "Rust",
            ["Rust (programming language)", "Rust"],
            ["", "Corrosion"],
            [
                "https://en.wikipedia.org/wiki/Rust_(programming_language)",
                "https://en.wikipedia.org/wiki/Rust"
            ]
        ]);
        let results = parse_wikipedia_opensearch(&value).expect("parse");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Rust (programming language)");
        assert!(results[0].snippet.contains("Wikipedia article"));
        assert_eq!(results[1].snippet, "Corrosion");
    }

    #[test]
    fn block_page_detector() {
        assert!(looks_like_ddg_block_page(
            "<html><body>Please complete the following challenge</body></html>"
        ));
        assert!(!looks_like_ddg_block_page(
            "<div class=\"result__body\"><a class=\"result__a\" href=\"https://x\">x</a></div>"
        ));
    }
}
