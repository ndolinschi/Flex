use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::embed::EmbedderKey;

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
    vectors: HashMap<String, (String, Vec<f32>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VectorHit {
    pub chunk_id: String,
    pub path: String,
    pub score: f32,
}

pub struct VectorStore {
    path: PathBuf,
    embedder: EmbedderKey,
    vectors: HashMap<String, (String, Vec<f32>)>,
}

impl VectorStore {
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

    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    pub fn embedder(&self) -> &EmbedderKey {
        &self.embedder
    }

    pub fn has(&self, chunk_id: &str) -> bool {
        self.vectors.contains_key(chunk_id)
    }

    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    pub fn upsert(&mut self, chunk_id: &str, path: &str, vector: Vec<f32>) {
        self.vectors
            .insert(chunk_id.to_owned(), (path.to_owned(), vector));
    }

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
