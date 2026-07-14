//! Deep-search plugin: web search + page scraping + researcher role.
//!
//! ## Quick start
//!
//! ```no_run
//! use agentloop_search::SearchPlugin;
//!
//! let plugin = SearchPlugin::default();
//! ```
//!
//! Swap the search backend:
//!
//! ```no_run
//! use std::sync::Arc;
//! use agentloop_search::{SearchPlugin, DuckDuckGoBackend};
//!
//! let plugin = SearchPlugin::new(Arc::new(DuckDuckGoBackend::new()));
//! ```

pub mod plugin;
pub mod rerank;
pub mod scrape_page;
pub mod search_backend;
pub mod search_web;

pub use plugin::SearchPlugin;
pub use rerank::{KeywordReranker, SearchReranker};
pub use scrape_page::ScrapePageTool;
pub use search_backend::{
    BraveSearchBackend, DuckDuckGoBackend, DuckDuckGoInstantBackend, FallbackSearchBackend,
    SearchBackend, SearchError, SearchResult, SearxNGBackend, WikipediaBackend,
    default_search_backends,
};
pub use search_web::SearchWebTool;
