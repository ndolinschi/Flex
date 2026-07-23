use std::sync::Arc;

use async_trait::async_trait;

use super::{
    BraveSearchBackend, DuckDuckGoBackend, DuckDuckGoInstantBackend, SearchBackend, SearchError,
    SearchResult, SearxNGBackend, WikipediaBackend,
};

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

pub struct FallbackSearchBackend {
    backends: Vec<Arc<dyn SearchBackend>>,
}

impl FallbackSearchBackend {
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
            return Err(SearchError::NoResults);
        }
        Err(last_err.unwrap_or(SearchError::NoResults))
    }
}
