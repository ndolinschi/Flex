//! SearXNG — opt-in via `SEARXNG_BASE_URL`.

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use super::{SearchBackend, SearchError, SearchResult, http_client, map_status_error, urlencoding};

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
