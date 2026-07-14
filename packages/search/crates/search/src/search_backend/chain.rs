//! Cascading fallback chain and the default backend ordering.

use std::sync::Arc;

use async_trait::async_trait;

use super::{
    BraveSearchBackend, DuckDuckGoBackend, DuckDuckGoInstantBackend, SearchBackend, SearchError,
    SearchResult, SearxNGBackend, WikipediaBackend,
};

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
