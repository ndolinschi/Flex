//! CI gate: hybrid retrieval recall@10 ≥ 0.8 on the offline golden set.
//!
//! Uses [`agentloop_index::eval`] with [`MockEmbedder`] overrides — no
//! network, no API keys, no live LLM.

use agentloop_index::eval::{
    RECALL_THRESHOLD, materialize_golden_fixture, open_golden_store, run_hybrid_retrieval_eval,
};

#[test]
fn hybrid_retrieval_recall_at_10_meets_threshold() {
    let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let index_dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    materialize_golden_fixture(repo.path()).unwrap_or_else(|e| panic!("{e}"));
    let store = open_golden_store(repo.path(), index_dir.path()).unwrap_or_else(|e| panic!("{e}"));
    let report = run_hybrid_retrieval_eval(&store).unwrap_or_else(|e| panic!("{e}"));

    report.assert_gate().unwrap_or_else(|e| {
        panic!(
            "{e}; agent_ab={:?}; per_query={:?}",
            report.agent_ab, report.per_query
        )
    });
    assert!(
        report.recall_at_k >= RECALL_THRESHOLD,
        "recall@{} = {:.3} ({}/{})",
        report.k,
        report.recall_at_k,
        report.hits,
        report.total
    );
}
