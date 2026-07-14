//! Pluggable search backends.
//!
//! The `SearchBackend` trait lets the `search_web` tool query different search
//! engines. The default chain (see [`default_search_backends`]) prefers free,
//! non-scraping APIs that do not rate-limit datacenter IPs the way DuckDuckGo
//! HTML and public SearXNG instances do.

mod brave;
mod chain;
mod ddg_html;
mod ddg_instant;
mod ddg_instant_parse;
mod html_parse;
mod searxng;
mod wikipedia;

use reqwest::Client;

pub use brave::BraveSearchBackend;
pub use chain::{FallbackSearchBackend, default_search_backends};
pub use ddg_html::DuckDuckGoBackend;
pub use ddg_instant::DuckDuckGoInstantBackend;
pub use searxng::SearxNGBackend;
pub use wikipedia::WikipediaBackend;

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
#[async_trait::async_trait]
pub trait SearchBackend: Send + Sync {
    /// Execute a web search and return parsed results.
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError>;
}

const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) \
     Chrome/124.0.0.0 Safari/537.36";

pub(crate) fn http_client() -> Client {
    Client::builder()
        .user_agent(BROWSER_UA)
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .unwrap_or_else(|_| Client::new())
}

pub(crate) fn map_status_error(err: reqwest::Error) -> SearchError {
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

/// Percent-encode a query string for a URL.
pub(crate) fn urlencoding(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencoding_spaces_and_special_chars() {
        assert_eq!(urlencoding("hello world"), "hello+world");
        assert_eq!(urlencoding("rust&go"), "rust%26go");
        assert_eq!(urlencoding("c++"), "c%2B%2B");
    }
}
