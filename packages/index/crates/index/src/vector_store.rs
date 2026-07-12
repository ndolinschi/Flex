//! Flat-file vector store: the simplest robust v1 for the embedding half of
//! hybrid retrieval.
//!
//! Chunk vectors are kept as a `Vec<(chunk_id, Vec<f32>)>`, bincode-encoded
//! to a single file alongside the manifest/symbol table (see [`crate::store`]).
//! Search is brute-force cosine similarity over every stored vector — fine
//! up to roughly 100k chunks (a few hundred ms at that scale on a modern
//! laptop CPU). Bigger corpora should upgrade to an ANN index (HNSW, e.g.
//! via `usearch` or `hnsw_rs`) behind the same `VectorStore` query surface;
//! nothing in `retrieve`/the tools depends on the brute-force scan itself.
//!
//! Keyed by [`EmbedderKey`] (provider id + dim): reopening a store whose
//! persisted key doesn't match the configured embedder wipes the vectors
//! and starts fresh, rather than mixing vectors from incompatible models.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::embed::EmbedderKey;

/// Bumped whenever the on-disk vector file's shape changes.
const VECTOR_STORE_VERSION: u32 = 1;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum VectorStoreError {
    #[error("failed to read/write vector store at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to (de)serialize vector store: {0}")]
    Codec(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Persisted {
    version: u32,
    embedder: EmbedderKey,
    /// chunk_id -> (path, vector). Path is kept alongside the vector so a
    /// whole-file removal doesn't require a second index structure.
    vectors: HashMap<String, (String, Vec<f32>)>,
}

/// One scored nearest-neighbor result from [`VectorStore::search`].
#[derive(Debug, Clone, PartialEq)]
pub struct VectorHit {
    pub chunk_id: String,
    pub path: String,
    /// Cosine similarity, in `[-1.0, 1.0]` (typically `[0.0, 1.0]` for
    /// embeddings from a model trained with normalized outputs).
    pub score: f32,
}

/// Per-repo store of chunk embedding vectors, persisted as a single bincode
/// file at `<index_dir>/vectors.bin`.
pub struct VectorStore {
    path: PathBuf,
    embedder: EmbedderKey,
    vectors: HashMap<String, (String, Vec<f32>)>,
}

impl VectorStore {
    /// Open (or create) the vector store at `index_dir/vectors.bin` for the
    /// given `embedder` identity. If a persisted store exists but was built
    /// with a different embedder id/dim, it's discarded — mixing vectors
    /// from two different models would make cosine similarity meaningless.
    pub fn open(index_dir: &Path, embedder: EmbedderKey) -> Result<Self, VectorStoreError> {
        let path = index_dir.join("vectors.bin");
        let persisted = load(&path)?;
        let (vectors, embedder) = match persisted {
            Some(p) if p.version == VECTOR_STORE_VERSION && p.embedder == embedder => {
                (p.vectors, p.embedder)
            }
            _ => (HashMap::new(), embedder),
        };
        Ok(Self {
            path,
            embedder,
            vectors,
        })
    }

    /// Whether the persisted store (before any writes this session) matched
    /// the requested embedder — `false` means it was invalidated and started
    /// empty (mismatched id/dim, or a version bump).
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    pub fn embedder(&self) -> &EmbedderKey {
        &self.embedder
    }

    /// Chunk ids already present in the store — used by callers to skip
    /// re-embedding chunks that haven't changed.
    pub fn has(&self, chunk_id: &str) -> bool {
        self.vectors.contains_key(chunk_id)
    }

    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Insert or overwrite a chunk's vector.
    pub fn upsert(&mut self, chunk_id: &str, path: &str, vector: Vec<f32>) {
        self.vectors
            .insert(chunk_id.to_owned(), (path.to_owned(), vector));
    }

    /// Drop every vector belonging to `path` (a file was deleted or
    /// re-chunked with different chunk ids). Returns the removed chunk ids.
    pub fn remove_path(&mut self, path: &str) -> Vec<String> {
        let removed: Vec<String> = self
            .vectors
            .iter()
            .filter(|(_, (p, _))| p == path)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &removed {
            self.vectors.remove(id);
        }
        removed
    }

    /// Persist the current contents to disk.
    pub fn save(&self) -> Result<(), VectorStoreError> {
        let persisted = Persisted {
            version: VECTOR_STORE_VERSION,
            embedder: self.embedder.clone(),
            vectors: self.vectors.clone(),
        };
        let bytes = bincode::serde::encode_to_vec(&persisted, bincode::config::standard())
            .map_err(|err| VectorStoreError::Codec(err.to_string()))?;
        fs::write(&self.path, bytes).map_err(|source| VectorStoreError::Io {
            path: self.path.display().to_string(),
            source,
        })
    }

    /// Brute-force cosine top-k over every stored vector.
    ///
    /// O(n) in the number of stored chunks — acceptable up to ~100k chunks;
    /// see the module doc for the ANN upgrade path once that stops being
    /// true for a given repo.
    pub fn search(&self, query: &[f32], k: usize) -> Vec<VectorHit> {
        if self.vectors.is_empty() || query.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<VectorHit> = self
            .vectors
            .iter()
            .map(|(chunk_id, (path, vector))| VectorHit {
                chunk_id: chunk_id.clone(),
                path: path.clone(),
                score: cosine_similarity(query, vector),
            })
            .collect();
        scored.sort_by(|a, b| b.score.total_cmp(&a.score));
        scored.truncate(k);
        scored
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a <= f32::EPSILON || norm_b <= f32::EPSILON {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

fn load(path: &Path) -> Result<Option<Persisted>, VectorStoreError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(|source| VectorStoreError::Io {
        path: path.display().to_string(),
        source,
    })?;
    match bincode::serde::decode_from_slice::<Persisted, _>(&bytes, bincode::config::standard()) {
        Ok((persisted, _)) => Ok(Some(persisted)),
        // Corrupt or foreign-format file: treat like "no store yet" rather
        // than a hard error, mirroring `store::load_manifest`'s version-drift
        // handling.
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(id: &str, dim: usize) -> EmbedderKey {
        EmbedderKey {
            id: id.to_owned(),
            dim,
        }
    }

    #[test]
    fn upsert_then_search_returns_nearest_by_cosine() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let mut store =
            VectorStore::open(dir.path(), key("mock", 3)).unwrap_or_else(|e| panic!("{e}"));
        store.upsert("a", "a.rs", vec![1.0, 0.0, 0.0]);
        store.upsert("b", "b.rs", vec![0.0, 1.0, 0.0]);

        let hits = store.search(&[1.0, 0.0, 0.0], 5);
        assert_eq!(hits[0].chunk_id, "a");
        assert!(hits[0].score > hits[1].score);
    }

    #[test]
    fn save_then_reopen_with_same_embedder_reloads_vectors() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        {
            let mut store =
                VectorStore::open(dir.path(), key("mock", 3)).unwrap_or_else(|e| panic!("{e}"));
            store.upsert("a", "a.rs", vec![1.0, 0.0, 0.0]);
            store.save().unwrap_or_else(|e| panic!("{e}"));
        }

        let reopened =
            VectorStore::open(dir.path(), key("mock", 3)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(reopened.len(), 1);
        assert!(reopened.has("a"));
    }

    #[test]
    fn reopen_with_mismatched_embedder_id_invalidates_store() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        {
            let mut store =
                VectorStore::open(dir.path(), key("mock-v1", 3)).unwrap_or_else(|e| panic!("{e}"));
            store.upsert("a", "a.rs", vec![1.0, 0.0, 0.0]);
            store.save().unwrap_or_else(|e| panic!("{e}"));
        }

        let reopened =
            VectorStore::open(dir.path(), key("mock-v2", 3)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(reopened.len(), 0, "different embedder id must invalidate");
    }

    #[test]
    fn reopen_with_mismatched_dim_invalidates_store() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        {
            let mut store =
                VectorStore::open(dir.path(), key("mock", 3)).unwrap_or_else(|e| panic!("{e}"));
            store.upsert("a", "a.rs", vec![1.0, 0.0, 0.0]);
            store.save().unwrap_or_else(|e| panic!("{e}"));
        }

        let reopened =
            VectorStore::open(dir.path(), key("mock", 384)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(reopened.len(), 0, "different dim must invalidate");
    }

    #[test]
    fn remove_path_drops_all_its_chunks() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let mut store =
            VectorStore::open(dir.path(), key("mock", 3)).unwrap_or_else(|e| panic!("{e}"));
        store.upsert("a1", "a.rs", vec![1.0, 0.0, 0.0]);
        store.upsert("a2", "a.rs", vec![0.0, 1.0, 0.0]);
        store.upsert("b1", "b.rs", vec![0.0, 0.0, 1.0]);

        let removed = store.remove_path("a.rs");
        assert_eq!(removed.len(), 2);
        assert_eq!(store.len(), 1);
        assert!(store.has("b1"));
    }
}
