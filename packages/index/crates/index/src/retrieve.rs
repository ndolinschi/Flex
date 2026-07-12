//! Query-time retrieval: BM25 search merged with a symbol-name exact/prefix
//! boost, so a query naming a symbol ranks its definition first even when
//! the surrounding prose wouldn't otherwise win on term frequency alone.
//!
//! [`search_hybrid`] additionally fuses in a cosine-similarity vector rank
//! (Reciprocal Rank Fusion, k=60) when the store has an embedder configured;
//! with no vectors present it degrades silently to exactly [`search`]'s
//! BM25 + symbol-boost behavior, so callers (the tools) can call
//! `search_hybrid` unconditionally.

use std::collections::HashMap;

use crate::chunker::chunk_id_of;
use crate::store::IndexStore;

/// Additive score boost when the query matches a chunk's symbol name
/// exactly (case-insensitive).
const EXACT_SYMBOL_BOOST: f32 = 10.0;
/// Additive score boost when the query is a prefix of the symbol name, or
/// vice versa.
const PREFIX_SYMBOL_BOOST: f32 = 4.0;
/// RRF's smoothing constant: `1 / (k + rank)`. 60 is the standard value from
/// the original Cormack/Clarke/Buettcher RRF paper, chosen so no single
/// ranker's #1 slot can completely dominate the fused score.
const RRF_K: f32 = 60.0;

/// One ranked retrieval result.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub snippet: String,
    pub score: f32,
    pub symbol: Option<String>,
}

/// Search `store` for `query`, returning up to `k` hits ranked by BM25 score
/// with a symbol-name boost merged in.
///
/// Pulls a larger candidate pool from tantivy (`k * 4`, capped) than the
/// final result count so re-ranking by the boost can actually change the
/// top-k order rather than just re-sorting whatever BM25 happened to return.
pub fn search(
    store: &IndexStore,
    query: &str,
    k: usize,
) -> Result<Vec<Hit>, crate::lexical::LexicalError> {
    let pool_size = (k.saturating_mul(4)).max(k).min(200);
    let raw_hits = store.lexical().search(query, pool_size)?;
    let query_lower = query.trim().to_lowercase();

    let mut hits: Vec<Hit> = raw_hits
        .into_iter()
        .map(|raw| {
            let boost = symbol_boost(&query_lower, raw.symbol.as_deref());
            Hit {
                path: raw.path,
                start_line: raw.start_line,
                end_line: raw.end_line,
                snippet: snippet_of(&raw.chunk),
                score: raw.score + boost,
                symbol: raw.symbol,
            }
        })
        .collect();

    hits.sort_by(|a, b| b.score.total_cmp(&a.score));
    hits.truncate(k);
    Ok(hits)
}

/// Errors from [`search_hybrid`]: either half of the fusion (lexical search,
/// or embedding the query for the vector half) can fail independently.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HybridSearchError {
    #[error(transparent)]
    Lexical(#[from] crate::lexical::LexicalError),
    #[error(transparent)]
    Store(#[from] crate::store::StoreError),
}

/// Hybrid search: Reciprocal Rank Fusion (`k=60`) of the BM25+symbol-boost
/// ranking (from [`search`]) with a cosine-similarity vector ranking, when
/// `store` has an embedder configured.
///
/// If the store has no vector store populated (embeddings disabled, or
/// simply not built yet), this degrades silently to exactly [`search`]'s
/// result — the tools call this unconditionally and never need to branch on
/// whether embeddings are enabled.
pub fn search_hybrid(
    store: &IndexStore,
    query: &str,
    k: usize,
) -> Result<Vec<Hit>, HybridSearchError> {
    let pool_size = (k.saturating_mul(4)).max(k).min(200);
    let lexical_hits = search(store, query, pool_size)?;

    let Some(vector_store) = store.vector_store() else {
        let mut hits = lexical_hits;
        hits.truncate(k);
        return Ok(hits);
    };
    if vector_store.is_empty() {
        let mut hits = lexical_hits;
        hits.truncate(k);
        return Ok(hits);
    }
    let Some(query_vector) = store.embed_query(query)? else {
        let mut hits = lexical_hits;
        hits.truncate(k);
        return Ok(hits);
    };

    let vector_hits = vector_store.search(&query_vector, pool_size);

    // chunk_id -> (best-known Hit fields, rrf score accumulator).
    let mut fused: HashMap<String, (Hit, f32)> = HashMap::new();

    for (rank, hit) in lexical_hits.into_iter().enumerate() {
        let chunk_id = chunk_id_of(&hit.path, hit.start_line, hit.end_line);
        let rrf = 1.0 / (RRF_K + rank as f32 + 1.0);
        fused
            .entry(chunk_id)
            .and_modify(|(_, score)| *score += rrf)
            .or_insert((hit, rrf));
    }

    // Paths already resolved via a `chunks_for_path` lookup this call, so a
    // query with several vector-only hits in the same file doesn't re-fetch
    // it once per chunk.
    let mut resolved_paths: HashMap<String, Vec<crate::lexical::RawHit>> = HashMap::new();

    for (rank, vhit) in vector_hits.into_iter().enumerate() {
        let rrf = 1.0 / (RRF_K + rank as f32 + 1.0);
        if let Some((_, score)) = fused.get_mut(&vhit.chunk_id) {
            *score += rrf;
            continue;
        }
        // A vector-only hit: this chunk's embedding is close to the query
        // but it shared too few (or no) literal terms to be in the BM25
        // pool at all — exactly the case hybrid retrieval exists to catch.
        // Resolve its full chunk data (line range, snippet, symbol) from
        // the lexical index by chunk id, since the vector store only kept
        // `(chunk_id, path, vector)`.
        let candidates = match resolved_paths.get(&vhit.path) {
            Some(cached) => cached,
            None => {
                let fetched = store.lexical().chunks_for_path(&vhit.path)?;
                resolved_paths.entry(vhit.path.clone()).or_insert(fetched)
            }
        };
        let Some(raw) = candidates
            .iter()
            .find(|raw| chunk_id_of(&raw.path, raw.start_line, raw.end_line) == vhit.chunk_id)
        else {
            continue; // Stale vector (file changed since last embed); skip.
        };
        let hit = Hit {
            path: raw.path.clone(),
            start_line: raw.start_line,
            end_line: raw.end_line,
            snippet: snippet_of(&raw.chunk),
            score: 0.0,
            symbol: raw.symbol.clone(),
        };
        fused.insert(vhit.chunk_id, (hit, rrf));
    }

    let mut results: Vec<Hit> = fused
        .into_values()
        .map(|(mut hit, score)| {
            hit.score = score;
            hit
        })
        .collect();
    // Tie-break deterministically on (path, start_line): `fused` is a
    // `HashMap`, so its iteration order (and thus the order of exact score
    // ties) isn't stable across runs otherwise.
    results.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.start_line.cmp(&b.start_line))
    });
    results.truncate(k);
    Ok(results)
}

fn symbol_boost(query_lower: &str, symbol: Option<&str>) -> f32 {
    let Some(symbol) = symbol else {
        return 0.0;
    };
    let symbol_lower = symbol.to_lowercase();
    if query_lower == symbol_lower {
        return EXACT_SYMBOL_BOOST;
    }
    if query_lower.contains(&symbol_lower) || symbol_lower.contains(query_lower) {
        return PREFIX_SYMBOL_BOOST;
    }
    0.0
}

/// First couple of lines of a chunk, for a compact preview.
fn snippet_of(chunk: &str) -> String {
    let mut lines = chunk.lines();
    let first = lines.next().unwrap_or("").trim();
    match lines.next() {
        Some(second) if !second.trim().is_empty() => format!("{first} {}", second.trim()),
        _ => first.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::IndexStore;
    use std::fs;
    use std::path::Path;

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|e| panic!("{e}"));
        }
        fs::write(path, content).unwrap_or_else(|e| panic!("{e}"));
    }

    /// A synthetic mini-repo with a handful of files, one of which is
    /// unambiguously "where the session title is generated".
    fn build_mini_repo() -> (tempfile::TempDir, tempfile::TempDir, IndexStore) {
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let index_dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));

        write(
            repo.path(),
            "src/session.rs",
            r#"
/// Derive a short human-readable title for a session from its first user
/// message, truncating to a reasonable length.
pub fn generate_session_title(first_message: &str) -> String {
    let trimmed = first_message.trim();
    trimmed.chars().take(60).collect()
}
"#,
        );
        write(
            repo.path(),
            "src/network.rs",
            r#"
/// Open a TCP connection to the given address with a timeout.
pub fn connect_with_timeout(addr: &str, timeout_ms: u64) -> Result<(), String> {
    let _ = (addr, timeout_ms);
    Ok(())
}
"#,
        );
        write(
            repo.path(),
            "src/math.rs",
            r#"
/// Compute the greatest common divisor of two integers.
pub fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}
"#,
        );
        write(
            repo.path(),
            "README.md",
            "# Demo Repo\n\n## Overview\n\nThis is an unrelated demo repository used for tests.\n",
        );

        let mut store =
            IndexStore::open(repo.path(), index_dir.path()).unwrap_or_else(|e| panic!("{e}"));
        store.build().unwrap_or_else(|e| panic!("{e}"));
        (repo, index_dir, store)
    }

    #[test]
    fn natural_language_query_ranks_right_file_top_1() {
        let (_repo, _index_dir, store) = build_mini_repo();
        let hits = search(&store, "where is the session title generated", 5)
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(!hits.is_empty(), "expected at least one hit");
        assert_eq!(
            hits[0].path,
            "src/session.rs",
            "expected src/session.rs top-1, got {:?}",
            hits.iter().map(|h| h.path.as_str()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn exact_symbol_name_query_boosts_its_definition_to_top() {
        let (_repo, _index_dir, store) = build_mini_repo();
        let hits = search(&store, "generate_session_title", 5).unwrap_or_else(|e| panic!("{e}"));
        assert!(!hits.is_empty());
        assert_eq!(hits[0].path, "src/session.rs");
        assert_eq!(hits[0].symbol.as_deref(), Some("generate_session_title"));
    }

    #[test]
    fn unrelated_query_does_not_rank_math_file_first() {
        let (_repo, _index_dir, store) = build_mini_repo();
        let hits =
            search(&store, "network timeout connection", 5).unwrap_or_else(|e| panic!("{e}"));
        assert!(!hits.is_empty());
        assert_eq!(hits[0].path, "src/network.rs");
    }

    /// Fixture for the hybrid-vs-BM25 tests below: a query whose literal
    /// words appear (out of context) in a lexically-loud but semantically
    /// irrelevant chunk, while the *actually* relevant chunk
    /// (`verify_credentials`) shares only one incidental word with the query
    /// (enough to land somewhere in BM25's candidate pool, never at the
    /// top) — exactly the case where BM25 alone still picks the lexical
    /// decoy, but RRF-fusing in a strong vector-rank signal flips the order.
    /// A couple of unrelated filler files widen the candidate pool so the
    /// fusion isn't just a two-item tiebreak.
    const QUERY: &str = "user login check";
    const AUTH_CHUNK: &str = "\
/// Confirm a supplied secret matches what's on file for this account.
pub fn verify_credentials(user_id: &str, secret: &str) -> bool {
    user_id.len() > 0 && secret.len() > 0
}
";
    const DISTRACTOR_CHUNK: &str = "\
/// Prints the words user, login, and check into the terminal banner --
/// user login check user login check -- purely a lexical decoy, has
/// nothing to do with authenticating anyone.
pub fn print_banner_words() {
    println!(\"user login check\");
}
";
    const FILLER_ONE: &str = "\
/// Compute the greatest common divisor of two integers.
pub fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}
";
    const FILLER_TWO: &str = "\
/// Open a TCP connection to the given address with a timeout.
pub fn connect_with_timeout(addr: &str, timeout_ms: u64) -> Result<(), String> {
    let _ = (addr, timeout_ms);
    Ok(())
}
";

    fn build_semantic_fixture() -> (tempfile::TempDir, tempfile::TempDir) {
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let index_dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        write(repo.path(), "src/auth.rs", AUTH_CHUNK);
        write(repo.path(), "src/banner.rs", DISTRACTOR_CHUNK);
        write(repo.path(), "src/math.rs", FILLER_ONE);
        write(repo.path(), "src/network.rs", FILLER_TWO);
        (repo, index_dir)
    }

    /// An embedder engineered so the query vector is close to the auth
    /// chunk's vector and far from the distractor's — simulating what a
    /// real semantic embedding model would give "for free", deterministically.
    ///
    /// Needles target each chunk's *code* (function signature), not the doc
    /// comment above it — [`crate::symbols::extract_symbols`] anchors a
    /// symbol's chunk at the `pub fn` line itself, so the doc comment isn't
    /// part of the text actually handed to `embed`. `QUERY`'s exact words
    /// also appear inside `DISTRACTOR_CHUNK`'s `println!` call, so the query
    /// override must be checked as a *whole-string* match, and the
    /// distractor's needle must be specific enough (its unique function
    /// signature) not to also match the query text.
    fn semantic_embedder() -> crate::embed::MockEmbedder {
        crate::embed::MockEmbedder::new(4)
            .with_override("pub fn verify_credentials", vec![0.9, 0.1, 0.0, 0.0])
            .with_override("pub fn print_banner_words", vec![0.0, 0.0, 0.0, 1.0])
            .with_override("pub fn gcd", vec![0.0, 1.0, 0.0, 0.0])
            .with_override("pub fn connect_with_timeout", vec![0.0, 0.0, 1.0, 0.0])
            .with_override(QUERY, vec![1.0, 0.0, 0.0, 0.0])
    }

    #[test]
    fn plain_bm25_search_ranks_lexical_distractor_over_semantic_match() {
        let (repo, index_dir) = build_semantic_fixture();
        let mut store =
            IndexStore::open(repo.path(), index_dir.path()).unwrap_or_else(|e| panic!("{e}"));
        store.build().unwrap_or_else(|e| panic!("{e}"));

        let hits = search(&store, QUERY, 5).unwrap_or_else(|e| panic!("{e}"));
        assert!(!hits.is_empty());
        assert_eq!(
            hits[0].path, "src/banner.rs",
            "BM25 alone should be fooled by the lexical decoy: {hits:?}"
        );
    }

    #[test]
    fn hybrid_search_ranks_semantically_relevant_chunk_above_lexical_decoy() {
        let (repo, index_dir) = build_semantic_fixture();
        let embedder = std::sync::Arc::new(semantic_embedder());
        let mut store = IndexStore::open_with_embeddings(repo.path(), index_dir.path(), embedder)
            .unwrap_or_else(|e| panic!("{e}"));
        store.build().unwrap_or_else(|e| panic!("{e}"));

        let hits = search_hybrid(&store, QUERY, 5).unwrap_or_else(|e| panic!("{e}"));
        assert!(!hits.is_empty());
        assert_eq!(
            hits[0].path, "src/auth.rs",
            "hybrid retrieval should rank the semantically relevant chunk first: {hits:?}"
        );
    }

    #[test]
    fn search_hybrid_degrades_to_bm25_when_no_embedder_configured() {
        let (repo, index_dir) = build_semantic_fixture();
        let mut store =
            IndexStore::open(repo.path(), index_dir.path()).unwrap_or_else(|e| panic!("{e}"));
        store.build().unwrap_or_else(|e| panic!("{e}"));

        let plain = search(&store, QUERY, 5).unwrap_or_else(|e| panic!("{e}"));
        let hybrid = search_hybrid(&store, QUERY, 5).unwrap_or_else(|e| panic!("{e}"));
        let plain_paths: Vec<&str> = plain.iter().map(|h| h.path.as_str()).collect();
        let hybrid_paths: Vec<&str> = hybrid.iter().map(|h| h.path.as_str()).collect();
        assert_eq!(
            plain_paths, hybrid_paths,
            "with no vector store, search_hybrid must match plain search's ranking"
        );
    }

    #[test]
    fn search_hybrid_degrades_to_bm25_when_vector_store_empty() {
        // An embedder is configured, but `build()` is never called, so the
        // vector store stays empty — the "not built yet" half of the
        // degrade-silently contract, distinct from "no embedder at all".
        let (repo, index_dir) = build_semantic_fixture();
        let embedder = std::sync::Arc::new(semantic_embedder());
        let store = IndexStore::open_with_embeddings(repo.path(), index_dir.path(), embedder)
            .unwrap_or_else(|e| panic!("{e}"));

        let hits = search_hybrid(&store, QUERY, 5).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            hits.is_empty(),
            "nothing indexed yet, hybrid should behave like plain search: {hits:?}"
        );
    }

    /// Golden recall@10 gate lives in [`crate::eval`] (M4). Kept here as a
    /// thin unit-level pointer so retrieve-module changes still trip the gate
    /// without needing the integration-test binary.
    #[test]
    fn hybrid_golden_recall_at_10_meets_threshold() {
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let index_dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        crate::eval::materialize_golden_fixture(repo.path()).unwrap_or_else(|e| panic!("{e}"));
        let store = crate::eval::open_golden_store(repo.path(), index_dir.path())
            .unwrap_or_else(|e| panic!("{e}"));
        let report =
            crate::eval::run_hybrid_retrieval_eval(&store).unwrap_or_else(|e| panic!("{e}"));
        report.assert_gate().unwrap_or_else(|e| panic!("{e}"));
    }
}
