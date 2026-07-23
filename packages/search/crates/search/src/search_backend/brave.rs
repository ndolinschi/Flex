use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use super::{SearchBackend, SearchError, SearchResult, http_client, map_status_error};

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
