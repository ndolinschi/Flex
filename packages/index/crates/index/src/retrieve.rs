use std::collections::HashMap;

use crate::chunker::chunk_id_of;
use crate::store::IndexStore;

const EXACT_SYMBOL_BOOST: f32 = 10.0;
const PREFIX_SYMBOL_BOOST: f32 = 4.0;
const RRF_K: f32 = 60.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub snippet: String,
    pub score: f32,
    pub symbol: Option<String>,
}

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

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HybridSearchError {
    #[error(transparent)]
    Lexical(#[from] crate::lexical::LexicalError),
    #[error(transparent)]
    Store(#[from] crate::store::StoreError),
}

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

    let mut fused: HashMap<String, (Hit, f32)> = HashMap::new();

    for (rank, hit) in lexical_hits.into_iter().enumerate() {
        let chunk_id = chunk_id_of(&hit.path, hit.start_line, hit.end_line);
        let rrf = 1.0 / (RRF_K + rank as f32 + 1.0);
        fused
            .entry(chunk_id)
            .and_modify(|(_, score)| *score += rrf)
            .or_insert((hit, rrf));
    }

    let mut resolved_paths: HashMap<String, Vec<crate::lexical::RawHit>> = HashMap::new();

    for (rank, vhit) in vector_hits.into_iter().enumerate() {
        let rrf = 1.0 / (RRF_K + rank as f32 + 1.0);
        if let Some((_, score)) = fused.get_mut(&vhit.chunk_id) {
            *score += rrf;
            continue;
        }
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
            continue;
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
