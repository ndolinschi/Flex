//! Agentic code-index plugin: lexical (BM25) + symbol + embedding search
//! over a repo, exposed as `SearchCode`/`FindSymbol`/`RepoMap` tools.
//!
//! The pipeline: [`scanner`] walks the repo (gitignore-aware, binary/size
//! filtered, content-hashed) → [`chunker`] splits files into symbol-aware
//! chunks (~40–120 lines on symbol boundaries, via [`symbols`]) →
//! [`lexical`] indexes chunks with `tantivy`, while [`embed`] +
//! [`vector_store`] optionally embed and store chunk vectors → [`store`]
//! ties it together with an incremental manifest → [`retrieve`] runs
//! [`retrieve::search_hybrid`] (BM25 + symbol boost, fused with cosine
//! vector rank via Reciprocal Rank Fusion when embeddings are enabled;
//! degrades silently to BM25 + symbol boost otherwise).
//!
//! [`repomap`] builds a PageRank/import-graph compact map for agent
//! orientation. [`auto_context`] optionally injects top-k chunks into the
//! first user message of a turn (`AGENTLOOP_AUTO_CONTEXT`, default off).
//! Index refresh on tool use is opt-in (`AGENTLOOP_INDEX_AUTO_UPDATE` /
//! desktop prefs `autoUpdateIndex`, default off — reuse a warm index).
//!
//! ## Embeddings (architecture)
//!
//! Default is **BM25-only** (`AGENTLOOP_EMBEDDINGS` unset/`off`): offline,
//! CI-safe, no model download. Opt in:
//! - `AGENTLOOP_EMBEDDINGS=remote` → [`embed::RemoteEmbedder`]
//!   (OpenAI-compatible; needs an API key). Plan-risk fallback when ONNX
//!   binaries are too heavy to ship.
//! - `AGENTLOOP_EMBEDDINGS=local` → [`embed::FastembedProvider`]
//!   (bge-small-en-v1.5, 384d) when the `local-embeddings` cargo feature is
//!   compiled in; otherwise a warning and BM25 degrade. Local ONNX is a
//!   follow-up for composition roots (desktop/sdk) once packaging size is
//!   acceptable.
//!
//! [`embed::MockEmbedder`] is used by every unit test (deterministic, no
//! network).
//!
//! ## Retrieval eval (M4)
//!
//! [`eval`] ships an offline golden set + hybrid recall@10 gate
//! ([`eval::RECALL_THRESHOLD`] ≥ 0.8). Run via
//! `cargo test -p agentloop-index retrieval_eval` — CI-safe, no API keys.
//!
//! ## Quick start
//!
//! ```no_run
//! use agentloop_index::IndexPlugin;
//!
//! let plugin = IndexPlugin::new().with_auto_context(false).with_auto_update(false);
//! ```

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
pub use repomap::build_repo_map;
pub use retrieve::{Hit, HybridSearchError, search, search_hybrid};
pub use store::{IndexStore, StoreError, UpdateStats};
pub use tools::shared::{
    AUTO_UPDATE_ENV, IndexOpenMode, env_auto_update_enabled, index_dir_for, index_root_base,
    open_and_build, open_and_build_with_events, open_and_build_with_events_mode,
    open_and_build_with_mode,
};
pub use tools::{FindSymbolTool, RepoMapTool, SearchCodeTool};
pub use vector_store::{VectorHit, VectorStore, VectorStoreError};
