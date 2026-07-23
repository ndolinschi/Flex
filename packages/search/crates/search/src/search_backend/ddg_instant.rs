use async_trait::async_trait;
use reqwest::Client;

use super::ddg_instant_parse::{DdgIaResponse, ddg_ia_results};
use super::{SearchBackend, SearchError, SearchResult, http_client, map_status_error};

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
        let parsed: DdgIaResponse = serde_json::from_str(&body).map_err(|err| {
            SearchError::ParseError(format!("DuckDuckGo IA JSON parse error: {err}"))
        })?;

        let results = ddg_ia_results(parsed);
        if results.is_empty() {
            return Err(SearchError::NoResults);
        }
        Ok(results)
    }
}
