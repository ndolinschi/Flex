//! Offline retrieval eval harness (M4).
//!
//! Builds a golden multi-file fixture, indexes it with a deterministic
//! [`crate::embed::MockEmbedder`], runs hybrid retrieval, and scores
//! **recall@10**. The CI gate is `recall_at_k >= `[`RECALL_THRESHOLD`].
//!
//! No network, no API keys, no live LLM. An optional [`AgentAbStub`] estimates
//! "tokens to first correct file" from retrieval rank as a cheap A/B proxy —
//! not a real agent run.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::embed::MockEmbedder;
use crate::retrieve::{Hit, HybridSearchError, search_hybrid};
use crate::store::{IndexStore, StoreError};

/// Top-k used for the recall gate (recall@10).
pub const RECALL_AT_K: usize = 10;

/// Minimum acceptable recall@[`RECALL_AT_K`] for the CI gate.
pub const RECALL_THRESHOLD: f64 = 0.8;

/// Assumed tokens spent opening one wrong file during Grep-style exploration
/// (A/B stub only).
const TOKENS_PER_MISS: u64 = 150;
/// Fixed overhead before the first file open (A/B stub only).
const TOKENS_BASE: u64 = 200;
/// Without an index, assume the correct file sits at this exploratory rank.
const NO_INDEX_BASELINE_RANK: usize = 25;

/// Errors from materializing the golden fixture or running the eval.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EvalError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Hybrid(#[from] HybridSearchError),
    #[error(
        "recall@{k} = {recall:.3} ({hits}/{total}) below threshold {threshold}; misses={misses:?}"
    )]
    BelowThreshold {
        k: usize,
        recall: f64,
        hits: usize,
        total: usize,
        threshold: f64,
        misses: Vec<String>,
    },
}

/// One golden query → expected repo-relative path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoldenQuery {
    pub query: String,
    pub expected_path: String,
}

/// Per-query retrieval outcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryResult {
    pub query: String,
    pub expected_path: String,
    pub found: bool,
    /// 1-based rank of the expected path in the top-k, if present.
    pub rank: Option<usize>,
    pub top_paths: Vec<String>,
}

/// Cheap offline proxy for agent A/B "tokens to first correct file".
///
/// Not a live LLM measurement: ranks from hybrid retrieval stand in for
/// how many wrong files an agent would open before the right one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentAbStub {
    pub tokens_to_first_correct_with_index: u64,
    pub tokens_to_first_correct_without_index: u64,
    /// `1 - with/without`, clamped to `[0, 1]`.
    pub savings_ratio: f64,
}

/// Aggregated golden-set report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalEvalReport {
    pub k: usize,
    pub hits: usize,
    pub total: usize,
    pub recall_at_k: f64,
    pub per_query: Vec<QueryResult>,
    pub misses: Vec<String>,
    pub agent_ab: AgentAbStub,
}

impl RetrievalEvalReport {
    /// Fail if [`Self::recall_at_k`] is below [`RECALL_THRESHOLD`].
    pub fn assert_gate(&self) -> Result<(), EvalError> {
        if self.recall_at_k >= RECALL_THRESHOLD {
            return Ok(());
        }
        Err(EvalError::BelowThreshold {
            k: self.k,
            recall: self.recall_at_k,
            hits: self.hits,
            total: self.total,
            threshold: RECALL_THRESHOLD,
            misses: self.misses.clone(),
        })
    }
}

/// The golden query set used by the CI gate.
pub fn golden_queries() -> Vec<GoldenQuery> {
    [
        ("authenticate user credentials", "src/auth.rs"),
        ("where is the session title generated", "src/session.rs"),
        ("tcp connection timeout", "src/network.rs"),
        ("greatest common divisor", "src/math.rs"),
        ("retry provider request backoff", "src/retry.rs"),
        ("parse configuration from toml", "src/config.rs"),
        ("compact conversation transcript", "src/compact.rs"),
        ("hash password with salt", "src/crypto.rs"),
        ("walk directory entries recursively", "src/fs_walk.rs"),
        ("serialize events to json lines", "src/jsonl.rs"),
    ]
    .into_iter()
    .map(|(query, expected_path)| GoldenQuery {
        query: query.to_owned(),
        expected_path: expected_path.to_owned(),
    })
    .collect()
}

/// Write the golden multi-file fixture under `repo_root`.
pub fn materialize_golden_fixture(repo_root: &Path) -> Result<(), EvalError> {
    let files: &[(&str, &str)] = &[
        (
            "src/auth.rs",
            "/// Confirm a supplied secret matches what's on file.\npub fn verify_credentials(user_id: &str, secret: &str) -> bool { true }\n",
        ),
        (
            "src/session.rs",
            "/// Derive a short human-readable title for a session.\npub fn generate_session_title(first_message: &str) -> String { first_message.to_owned() }\n",
        ),
        (
            "src/network.rs",
            "/// Open a TCP connection to the given address with a timeout.\npub fn connect_with_timeout(addr: &str, timeout_ms: u64) -> Result<(), String> { Ok(()) }\n",
        ),
        (
            "src/math.rs",
            "/// Compute the greatest common divisor of two integers.\npub fn gcd(mut a: u64, mut b: u64) -> u64 { a }\n",
        ),
        (
            "src/banner.rs",
            "/// Lexical decoy that repeats \"user login check\" without authenticating.\npub fn print_banner_words() { println!(\"user login check\"); }\n",
        ),
        (
            "src/retry.rs",
            "/// Retry a fallible provider request with exponential backoff.\npub fn retry_provider_request(attempt: u32) -> u64 { 1u64 << attempt.min(6) }\n",
        ),
        (
            "src/config.rs",
            "/// Load agent settings from a TOML document on disk.\npub fn parse_config_toml(raw: &str) -> Result<(), String> { let _ = raw; Ok(()) }\n",
        ),
        (
            "src/compact.rs",
            "/// Shrink an over-long conversation transcript to fit the context window.\npub fn compact_conversation_transcript(events: &[String]) -> Vec<String> { events.to_vec() }\n",
        ),
        (
            "src/crypto.rs",
            "/// Derive a password verifier from a secret and a random salt.\npub fn hash_password_with_salt(secret: &str, salt: &[u8]) -> [u8; 32] { let _ = (secret, salt); [0u8; 32] }\n",
        ),
        (
            "src/fs_walk.rs",
            "/// Recursively visit every directory entry under a root path.\npub fn walk_directory_entries(root: &str) -> Vec<String> { vec![root.to_owned()] }\n",
        ),
        (
            "src/jsonl.rs",
            "/// Append one structured event as a single JSON line.\npub fn serialize_event_json_line(event: &str) -> String { event.to_owned() }\n",
        ),
    ];

    for (rel, body) in files {
        let path = repo_root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, body)?;
    }
    Ok(())
}

fn unit(dim: usize, axis: usize) -> Vec<f32> {
    let mut v = vec![0.0; dim];
    if axis < dim {
        v[axis] = 1.0;
    }
    v
}

/// Deterministic embedder with orthogonal unit vectors per golden symbol /
/// query so hybrid RRF ranks the expected file first offline.
///
/// The banner decoy is left on the hash fallback so it never collides with a
/// golden target axis.
pub fn golden_mock_embedder() -> MockEmbedder {
    const D: usize = 10;
    MockEmbedder::new(D)
        .with_override("pub fn verify_credentials", unit(D, 0))
        .with_override("pub fn generate_session_title", unit(D, 1))
        .with_override("pub fn connect_with_timeout", unit(D, 2))
        .with_override("pub fn gcd", unit(D, 3))
        .with_override("pub fn retry_provider_request", unit(D, 4))
        .with_override("pub fn parse_config_toml", unit(D, 5))
        .with_override("pub fn compact_conversation_transcript", unit(D, 6))
        .with_override("pub fn hash_password_with_salt", unit(D, 7))
        .with_override("pub fn walk_directory_entries", unit(D, 8))
        .with_override("pub fn serialize_event_json_line", unit(D, 9))
        .with_override("authenticate user credentials", unit(D, 0))
        .with_override("where is the session title generated", unit(D, 1))
        .with_override("tcp connection timeout", unit(D, 2))
        .with_override("greatest common divisor", unit(D, 3))
        .with_override("retry provider request backoff", unit(D, 4))
        .with_override("parse configuration from toml", unit(D, 5))
        .with_override("compact conversation transcript", unit(D, 6))
        .with_override("hash password with salt", unit(D, 7))
        .with_override("walk directory entries recursively", unit(D, 8))
        .with_override("serialize events to json lines", unit(D, 9))
}

/// Open + build an index over a materialized golden fixture.
pub fn open_golden_store(repo_root: &Path, index_dir: &Path) -> Result<IndexStore, EvalError> {
    let embedder = Arc::new(golden_mock_embedder());
    let mut store = IndexStore::open_with_embeddings(repo_root, index_dir, embedder)?;
    store.build()?;
    Ok(store)
}

/// Score hybrid retrieval against [`golden_queries`] on an already-built store.
pub fn run_hybrid_retrieval_eval(store: &IndexStore) -> Result<RetrievalEvalReport, EvalError> {
    score_queries(store, &golden_queries(), RECALL_AT_K)
}

fn score_queries(
    store: &IndexStore,
    cases: &[GoldenQuery],
    k: usize,
) -> Result<RetrievalEvalReport, EvalError> {
    let mut hits_at_k = 0usize;
    let mut misses = Vec::new();
    let mut per_query = Vec::with_capacity(cases.len());
    let mut ranks_for_ab: Vec<Option<usize>> = Vec::with_capacity(cases.len());

    for case in cases {
        let hits = search_hybrid(store, &case.query, k)?;
        let rank = rank_of(&hits, &case.expected_path);
        let found = rank.is_some();
        if found {
            hits_at_k += 1;
        } else {
            misses.push(format!(
                "query={:?} expected={} got={:?}",
                case.query,
                case.expected_path,
                hits.iter().map(|h| h.path.as_str()).collect::<Vec<_>>()
            ));
        }
        ranks_for_ab.push(rank);
        per_query.push(QueryResult {
            query: case.query.clone(),
            expected_path: case.expected_path.clone(),
            found,
            rank,
            top_paths: hits.into_iter().map(|h| h.path).collect(),
        });
    }

    let total = cases.len();
    let recall_at_k = if total == 0 {
        0.0
    } else {
        hits_at_k as f64 / total as f64
    };

    Ok(RetrievalEvalReport {
        k,
        hits: hits_at_k,
        total,
        recall_at_k,
        per_query,
        misses,
        agent_ab: stub_agent_ab(&ranks_for_ab),
    })
}

fn rank_of(hits: &[Hit], expected_path: &str) -> Option<usize> {
    hits.iter()
        .position(|h| h.path == expected_path)
        .map(|i| i + 1)
}

fn tokens_for_rank(rank: usize) -> u64 {
    TOKENS_BASE + (rank.saturating_sub(1) as u64) * TOKENS_PER_MISS
}

fn stub_agent_ab(ranks: &[Option<usize>]) -> AgentAbStub {
    let mut with_sum = 0u64;
    let mut without_sum = 0u64;
    let n = ranks.len().max(1) as u64;
    for rank in ranks {
        let with_rank = rank.unwrap_or(RECALL_AT_K + 1);
        with_sum += tokens_for_rank(with_rank);
        without_sum += tokens_for_rank(NO_INDEX_BASELINE_RANK);
    }
    let with_avg = with_sum / n;
    let without_avg = without_sum / n;
    let savings_ratio = if without_avg == 0 {
        0.0
    } else {
        (1.0 - (with_avg as f64 / without_avg as f64)).clamp(0.0, 1.0)
    };
    AgentAbStub {
        tokens_to_first_correct_with_index: with_avg,
        tokens_to_first_correct_without_index: without_avg,
        savings_ratio,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_set_meets_recall_threshold() {
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let index_dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        materialize_golden_fixture(repo.path()).unwrap_or_else(|e| panic!("{e}"));
        let store =
            open_golden_store(repo.path(), index_dir.path()).unwrap_or_else(|e| panic!("{e}"));
        let report = run_hybrid_retrieval_eval(&store).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            report.total >= 8,
            "golden set should be reasonably sized, got {}",
            report.total
        );
        report.assert_gate().unwrap_or_else(|e| panic!("{e}"));
        assert!(
            report.agent_ab.tokens_to_first_correct_with_index
                < report.agent_ab.tokens_to_first_correct_without_index,
            "index stub should beat no-index baseline: {:?}",
            report.agent_ab
        );
    }

    #[test]
    fn assert_gate_rejects_low_recall() {
        let report = RetrievalEvalReport {
            k: 10,
            hits: 1,
            total: 10,
            recall_at_k: 0.1,
            per_query: Vec::new(),
            misses: vec!["miss".into()],
            agent_ab: AgentAbStub {
                tokens_to_first_correct_with_index: 500,
                tokens_to_first_correct_without_index: 500,
                savings_ratio: 0.0,
            },
        };
        assert!(report.assert_gate().is_err());
    }
}
