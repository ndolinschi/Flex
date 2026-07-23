pub mod auto_context;
pub mod chunker;
pub mod embed;
pub mod eval;
pub mod lexical;
pub mod plugin;
pub mod repomap;
pub mod retrieve;
pub mod scanner;
pub mod store;
pub mod symbols;
mod tools;
pub mod vector_store;

pub use auto_context::{
    AUTO_CONTEXT_ENV, AutoContextHook, IndexStatus, env_auto_context_enabled, rebuild_with_stats,
    status_for,
};
pub use embed::{
    EmbedError, EmbedderKey, EmbeddingProvider, MockEmbedder, RemoteEmbedder, resolve_embedder,
};
pub use eval::{
    AgentAbStub, EvalError, GoldenQuery, QueryResult, RECALL_AT_K, RECALL_THRESHOLD,
    RetrievalEvalReport, golden_mock_embedder, golden_queries, materialize_golden_fixture,
    open_golden_store, run_hybrid_retrieval_eval,
};
pub use plugin::IndexPlugin;
pub use repomap::{build_repo_map, build_repo_map_cached};
pub use retrieve::{Hit, HybridSearchError, search, search_hybrid};
pub use store::{IndexStore, StoreError, UpdateStats};
pub use tools::shared::{
    AUTO_UPDATE_ENV, IndexOpenMode, env_auto_update_enabled, index_dir_for, index_root_base,
    open_and_build, open_and_build_with_events, open_and_build_with_events_mode,
    open_and_build_with_mode,
};
pub use tools::{FindSymbolTool, RepoMapTool, SearchCodeTool};
pub use vector_store::{VectorHit, VectorStore, VectorStoreError};
