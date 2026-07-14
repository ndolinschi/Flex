//! DuckDuckGo HTML scrape — last-ditch; often blocked from datacenters.

use async_trait::async_trait;
use reqwest::Client;

use super::html_parse::{looks_like_ddg_block_page, parse_duckduckgo_html};
use super::{SearchBackend, SearchError, SearchResult, http_client, map_status_error, urlencoding};

/// Searches DuckDuckGo's HTML endpoint and parses result blocks.
///
/// Uses `https://html.duckduckgo.com/html/` — no API key is required, but
/// datacenter IPs are frequently blocked (400/captcha). Prefer
/// [`super::DuckDuckGoInstantBackend`] in the default chain.
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
