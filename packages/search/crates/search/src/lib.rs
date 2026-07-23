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
