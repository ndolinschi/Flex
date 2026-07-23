use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::chunker::{Chunk, chunk_file};
use crate::embed::{EmbedderKey, EmbeddingProvider};
use crate::lexical::{LexicalError, LexicalIndex};
use crate::scanner::scan_repo;
use crate::symbols::{Language, Symbol, extract_symbols};
use crate::vector_store::VectorStore;

const MANIFEST_VERSION: u32 = 2;

const MANIFEST_FILE: &str = "manifest.json";
const SYMBOLS_FILE: &str = "symbols.json";
const TANTIVY_DIR: &str = "tantivy";

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    #[error(transparent)]
    Lexical(#[from] LexicalError),
    #[error(transparent)]
    Vector(#[from] crate::vector_store::VectorStoreError),
    #[error("embedding failed: {0}")]
    Embed(#[from] crate::embed::EmbedError),
    #[error("failed to read/write index metadata at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to (de)serialize index metadata: {0}")]
    Serde(#[from] serde_json::Error),
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    files: HashMap<String, String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStats {
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
    pub unchanged: usize,
}

pub struct IndexStore {
    repo_root: PathBuf,
    index_dir: PathBuf,
    lexical: LexicalIndex,
    manifest: Manifest,
    symbols: Vec<Symbol>,
    embeddings: Option<Embeddings>,
}

struct Embeddings {
    provider: Arc<dyn EmbeddingProvider>,
    store: VectorStore,
}

impl IndexStore {
    pub fn open(
        repo_root: impl Into<PathBuf>,
        index_dir: impl Into<PathBuf>,
    ) -> Result<Self, StoreError> {
        Self::open_impl(repo_root.into(), index_dir.into(), None)
    }

    pub fn open_with_embeddings(
        repo_root: impl Into<PathBuf>,
        index_dir: impl Into<PathBuf>,
        embedder: Arc<dyn EmbeddingProvider>,
    ) -> Result<Self, StoreError> {
        Self::open_impl(repo_root.into(), index_dir.into(), Some(embedder))
    }

    fn open_impl(
        repo_root: PathBuf,
        index_dir: PathBuf,
        embedder: Option<Arc<dyn EmbeddingProvider>>,
    ) -> Result<Self, StoreError> {
        fs::create_dir_all(&index_dir).map_err(|source| StoreError::Io {
            path: index_dir.display().to_string(),
            source,
        })?;
        let lexical = LexicalIndex::open_or_create(&index_dir.join(TANTIVY_DIR))?;
        let manifest = load_manifest(&index_dir)?;
        let symbols = load_symbols(&index_dir)?;
        let embeddings = match embedder {
            Some(provider) => {
                let key = EmbedderKey::of(provider.as_ref());
                let store = VectorStore::open(&index_dir, key)?;
                Some(Embeddings { provider, store })
            }
            None => None,
        };
        Ok(Self {
            repo_root,
            index_dir,
            lexical,
            manifest,
            symbols,
            embeddings,
        })
    }

    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    pub fn index_dir(&self) -> &Path {
        &self.index_dir
    }

    pub fn indexed_paths(&self) -> impl Iterator<Item = &str> {
        self.manifest.files.keys().map(String::as_str)
    }

    pub(crate) fn vector_store(&self) -> Option<&VectorStore> {
        self.embeddings.as_ref().map(|e| &e.store)
    }

    pub(crate) fn embed_query(&self, query: &str) -> Result<Option<Vec<f32>>, StoreError> {
        let Some(embeddings) = &self.embeddings else {
            return Ok(None);
        };
        let vectors = embeddings.provider.embed(&[query])?;
        Ok(vectors.into_iter().next())
    }

    pub fn build_with_progress<F>(&mut self, mut on_progress: F) -> Result<UpdateStats, StoreError>
    where
        F: FnMut(usize, usize),
    {
        let scanned = scan_repo(&self.repo_root);
        let mut stats = UpdateStats::default();
        let mut seen = std::collections::HashSet::new();

        let work: Vec<_> = scanned
            .iter()
            .filter(|file| self.manifest.files.get(&file.rel_path) != Some(&file.content_hash))
            .collect();
        let total = work.len();
        let mut done = 0usize;

        for file in &scanned {
            seen.insert(file.rel_path.clone());
            let previous_hash = self.manifest.files.get(&file.rel_path);
            if previous_hash == Some(&file.content_hash) {
                stats.unchanged += 1;
                self.embed_if_missing(&file.rel_path)?;
                continue;
            }
            let is_new = previous_hash.is_none();
            self.reindex_file(&file.rel_path, &file.content_hash)?;
            if is_new {
                stats.added += 1;
            } else {
                stats.changed += 1;
            }
            done += 1;
            on_progress(done, total);
        }

        let removed_paths: Vec<String> = self
            .manifest
            .files
            .keys()
            .filter(|p| !seen.contains(*p))
            .cloned()
            .collect();
        for path in removed_paths {
            self.remove_file(&path)?;
            stats.removed += 1;
        }

        self.persist_manifest()?;
        self.persist_symbols()?;
        self.persist_vectors()?;
        Ok(stats)
    }

    pub fn build(&mut self) -> Result<UpdateStats, StoreError> {
        self.build_with_progress(|_, _| {})
    }

    pub fn update(&mut self, changed_paths: &[String]) -> Result<UpdateStats, StoreError> {
        let mut stats = UpdateStats::default();
        for rel_path in changed_paths {
            let abs = self.repo_root.join(rel_path);
            match fs::read(&abs) {
                Ok(bytes) => {
                    let hash = blake3::hash(&bytes).to_hex().to_string();
                    if self.manifest.files.get(rel_path) == Some(&hash) {
                        stats.unchanged += 1;
                        continue;
                    }
                    let is_new = !self.manifest.files.contains_key(rel_path);
                    self.reindex_file(rel_path, &hash)?;
                    if is_new {
                        stats.added += 1;
                    } else {
                        stats.changed += 1;
                    }
                }
                Err(_) => {
                    if self.manifest.files.contains_key(rel_path) {
                        self.remove_file(rel_path)?;
                        stats.removed += 1;
                    }
                }
            }
        }
        self.persist_manifest()?;
        self.persist_symbols()?;
        self.persist_vectors()?;
        Ok(stats)
    }

    fn reindex_file(&mut self, rel_path: &str, content_hash: &str) -> Result<(), StoreError> {
        let abs = self.repo_root.join(rel_path);
        let Ok(source) = fs::read_to_string(&abs) else {
            return Ok(());
        };
        let language = Language::from_path(Path::new(rel_path));
        let chunks = chunk_file(rel_path, &source);
        self.lexical
            .update_file(rel_path, language.tag(), &chunks)?;

        self.symbols.retain(|s| s.path != rel_path);
        self.symbols.extend(extract_symbols(rel_path, &source));

        self.embed_chunks(rel_path, &chunks)?;

        self.manifest
            .files
            .insert(rel_path.to_owned(), content_hash.to_owned());
        Ok(())
    }

    fn embed_chunks(&mut self, rel_path: &str, chunks: &[Chunk]) -> Result<(), StoreError> {
        let Some(embeddings) = &mut self.embeddings else {
            return Ok(());
        };
        embeddings.store.remove_path(rel_path);

        let to_embed: Vec<&Chunk> = chunks.iter().collect();
        if to_embed.is_empty() {
            return Ok(());
        }
        let texts: Vec<&str> = to_embed.iter().map(|c| c.text.as_str()).collect();
        let vectors = embeddings.provider.embed(&texts)?;
        for (chunk, vector) in to_embed.iter().zip(vectors) {
            embeddings.store.upsert(&chunk.chunk_id(), rel_path, vector);
        }
        Ok(())
    }

    fn embed_if_missing(&mut self, rel_path: &str) -> Result<(), StoreError> {
        if self.embeddings.is_none() {
            return Ok(());
        }
        let abs = self.repo_root.join(rel_path);
        let Ok(source) = fs::read_to_string(&abs) else {
            return Ok(());
        };
        let chunks = chunk_file(rel_path, &source);

        let missing_ids: Vec<usize> = {
            let Some(embeddings) = &self.embeddings else {
                return Ok(());
            };
            chunks
                .iter()
                .enumerate()
                .filter(|(_, c)| !embeddings.store.has(&c.chunk_id()))
                .map(|(idx, _)| idx)
                .collect()
        };
        if missing_ids.is_empty() {
            return Ok(());
        }

        let texts: Vec<&str> = missing_ids
            .iter()
            .map(|&i| chunks[i].text.as_str())
            .collect();
        let vectors = {
            let Some(embeddings) = &self.embeddings else {
                return Ok(());
            };
            embeddings.provider.embed(&texts)?
        };

        let Some(embeddings) = &mut self.embeddings else {
            return Ok(());
        };
        for (&idx, vector) in missing_ids.iter().zip(vectors) {
            embeddings
                .store
                .upsert(&chunks[idx].chunk_id(), rel_path, vector);
        }
        Ok(())
    }

    fn remove_file(&mut self, rel_path: &str) -> Result<(), StoreError> {
        self.lexical.remove_file(rel_path)?;
        self.symbols.retain(|s| s.path != rel_path);
        self.manifest.files.remove(rel_path);
        if let Some(embeddings) = &mut self.embeddings {
            embeddings.store.remove_path(rel_path);
        }
        Ok(())
    }

    fn persist_manifest(&self) -> Result<(), StoreError> {
        let path = self.index_dir.join(MANIFEST_FILE);
        let json = serde_json::to_vec_pretty(&self.manifest)?;
        fs::write(&path, json).map_err(|source| StoreError::Io {
            path: path.display().to_string(),
            source,
        })
    }

    fn persist_vectors(&self) -> Result<(), StoreError> {
        let Some(embeddings) = &self.embeddings else {
            return Ok(());
        };
        Ok(embeddings.store.save()?)
    }

    fn persist_symbols(&self) -> Result<(), StoreError> {
        let path = self.index_dir.join(SYMBOLS_FILE);
        let json = serde_json::to_vec(&self.symbols)?;
        fs::write(&path, json).map_err(|source| StoreError::Io {
            path: path.display().to_string(),
            source,
        })
    }

    pub fn lexical(&self) -> &LexicalIndex {
        &self.lexical
    }

    pub fn symbols(&self) -> &[Symbol] {
        &self.symbols
    }

    pub fn manifest_hash(&self, rel_path: &str) -> Option<&str> {
        self.manifest.files.get(rel_path).map(String::as_str)
    }

    pub fn indexed_file_count(&self) -> usize {
        self.manifest.files.len()
    }

    pub fn manifest_fingerprint(&self) -> String {
        let mut entries: Vec<(&str, &str)> = self
            .manifest
            .files
            .iter()
            .map(|(path, hash)| (path.as_str(), hash.as_str()))
            .collect();
        entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
        let mut hasher = blake3::Hasher::new();
        for (path, hash) in entries {
            hasher.update(path.as_bytes());
            hasher.update(b"\0");
            hasher.update(hash.as_bytes());
            hasher.update(b"\n");
        }
        hasher.finalize().to_hex().to_string()
    }

    pub fn embedded_chunk_count(&self) -> usize {
        self.embeddings.as_ref().map(|e| e.store.len()).unwrap_or(0)
    }

    pub fn status_counts(index_dir: &Path) -> Result<(usize, usize), StoreError> {
        let manifest = load_manifest(index_dir)?;
        let symbol_count = count_symbols_len(index_dir)?;
        Ok((manifest.files.len(), symbol_count))
    }
}

fn load_manifest(index_dir: &Path) -> Result<Manifest, StoreError> {
    let path = index_dir.join(MANIFEST_FILE);
    if !path.exists() {
        return Ok(Manifest {
            version: MANIFEST_VERSION,
            files: HashMap::new(),
        });
    }
    let bytes = fs::read(&path).map_err(|source| StoreError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    if manifest.version != MANIFEST_VERSION {
        return Ok(Manifest {
            version: MANIFEST_VERSION,
            files: HashMap::new(),
        });
    }
    Ok(manifest)
}

fn load_symbols(index_dir: &Path) -> Result<Vec<Symbol>, StoreError> {
    let path = index_dir.join(SYMBOLS_FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = fs::read(&path).map_err(|source| StoreError::Io {
        path: path.display().to_string(),
        source,
    })?;
    Ok(serde_json::from_slice(&bytes).unwrap_or_default())
}

fn count_symbols_len(index_dir: &Path) -> Result<usize, StoreError> {
    let path = index_dir.join(SYMBOLS_FILE);
    if !path.exists() {
        return Ok(0);
    }
    let bytes = fs::read(&path).map_err(|source| StoreError::Io {
        path: path.display().to_string(),
        source,
    })?;
    match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(serde_json::Value::Array(items)) => Ok(items.len()),
        Ok(_) | Err(_) => Ok(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|e| panic!("{e}"));
        }
        fs::write(path, content).unwrap_or_else(|e| panic!("{e}"));
    }

    #[test]
    fn build_indexes_all_files_then_incremental_update_only_touches_changed() {
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let index_dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        write(repo.path(), "a.rs", "fn a_fn() { 1; }");
        write(repo.path(), "b.rs", "fn b_fn() { 2; }");

        let mut store =
            IndexStore::open(repo.path(), index_dir.path()).unwrap_or_else(|e| panic!("{e}"));
        let stats = store.build().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(stats.added, 2);
        assert_eq!(stats.changed, 0);
        assert_eq!(store.indexed_file_count(), 2);

        let stats = store.build().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(stats.added, 0);
        assert_eq!(stats.changed, 0);
        assert_eq!(stats.unchanged, 2);

        let a_hash_before = store.manifest_hash("b.rs").map(str::to_owned);
        write(repo.path(), "a.rs", "fn a_fn() { 999; }");
        let stats = store.build().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(stats.changed, 1, "only a.rs should have changed");
        assert_eq!(stats.unchanged, 1, "b.rs should be untouched");
        assert_eq!(
            store.manifest_hash("b.rs").map(str::to_owned),
            a_hash_before
        );
    }

    #[test]
    fn build_drops_deleted_files_from_manifest_and_index() {
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let index_dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        write(repo.path(), "gone.rs", "fn will_be_deleted() {}");

        let mut store =
            IndexStore::open(repo.path(), index_dir.path()).unwrap_or_else(|e| panic!("{e}"));
        store.build().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(store.indexed_file_count(), 1);

        fs::remove_file(repo.path().join("gone.rs")).unwrap_or_else(|e| panic!("{e}"));
        let stats = store.build().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(stats.removed, 1);
        assert_eq!(store.indexed_file_count(), 0);
        assert!(store.manifest_hash("gone.rs").is_none());
    }

    #[test]
    fn reopening_store_reloads_manifest_and_symbols() {
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let index_dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        write(repo.path(), "a.rs", "fn persisted_fn() {}");

        {
            let mut store =
                IndexStore::open(repo.path(), index_dir.path()).unwrap_or_else(|e| panic!("{e}"));
            store.build().unwrap_or_else(|e| panic!("{e}"));
        }

        let reopened =
            IndexStore::open(repo.path(), index_dir.path()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(reopened.indexed_file_count(), 1);
        assert!(reopened.symbols().iter().any(|s| s.name == "persisted_fn"));
    }

    struct CountingEmbedder {
        inner: crate::embed::MockEmbedder,
        calls: std::sync::Mutex<Vec<String>>,
    }

    impl CountingEmbedder {
        fn new(dim: usize) -> Self {
            Self {
                inner: crate::embed::MockEmbedder::new(dim),
                calls: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn embedded_texts(&self) -> Vec<String> {
            self.calls.lock().unwrap_or_else(|e| panic!("{e}")).clone()
        }
    }

    impl crate::embed::EmbeddingProvider for CountingEmbedder {
        fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, crate::embed::EmbedError> {
            let mut calls = self.calls.lock().unwrap_or_else(|e| panic!("{e}"));
            calls.extend(texts.iter().map(|t| (*t).to_owned()));
            drop(calls);
            self.inner.embed(texts)
        }

        fn dim(&self) -> usize {
            self.inner.dim()
        }

        fn id(&self) -> &str {
            self.inner.id()
        }
    }

    #[test]
    fn build_embeds_only_new_or_changed_chunks_incrementally() {
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let index_dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        write(repo.path(), "a.rs", "fn alpha() { 1; }");
        write(repo.path(), "b.rs", "fn beta() { 2; }");

        let embedder = Arc::new(CountingEmbedder::new(8));
        let mut store =
            IndexStore::open_with_embeddings(repo.path(), index_dir.path(), embedder.clone())
                .unwrap_or_else(|e| panic!("{e}"));

        store.build().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(store.embedded_chunk_count(), 2, "both files embedded once");
        let first_pass = embedder.embedded_texts();
        assert!(first_pass.iter().any(|t| t.contains("alpha")));
        assert!(first_pass.iter().any(|t| t.contains("beta")));

        store.build().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            embedder.embedded_texts().len(),
            first_pass.len(),
            "no file changed, so no chunk should be re-embedded"
        );

        write(repo.path(), "a.rs", "fn alpha() { 999; }");
        store.build().unwrap_or_else(|e| panic!("{e}"));
        let after_touch = embedder.embedded_texts();
        assert_eq!(
            after_touch.len(),
            first_pass.len() + 1,
            "only a.rs's one chunk should have been re-embedded: {after_touch:?}"
        );
        assert!(after_touch.last().is_some_and(|t| t.contains("999")));
        assert_eq!(
            store.embedded_chunk_count(),
            2,
            "b.rs's vector is untouched"
        );
    }

    #[test]
    fn build_removes_vectors_for_deleted_files() {
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let index_dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        write(repo.path(), "gone.rs", "fn will_be_deleted() {}");

        let embedder: Arc<dyn crate::embed::EmbeddingProvider> =
            Arc::new(crate::embed::MockEmbedder::new(8));
        let mut store = IndexStore::open_with_embeddings(repo.path(), index_dir.path(), embedder)
            .unwrap_or_else(|e| panic!("{e}"));
        store.build().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(store.embedded_chunk_count(), 1);

        fs::remove_file(repo.path().join("gone.rs")).unwrap_or_else(|e| panic!("{e}"));
        store.build().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(store.embedded_chunk_count(), 0);
    }

    #[test]
    fn reopening_with_same_embedder_id_reuses_persisted_vectors_without_reembedding() {
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let index_dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        write(repo.path(), "a.rs", "fn alpha() { 1; }");

        {
            let embedder: Arc<dyn crate::embed::EmbeddingProvider> =
                Arc::new(crate::embed::MockEmbedder::new(8));
            let mut store =
                IndexStore::open_with_embeddings(repo.path(), index_dir.path(), embedder)
                    .unwrap_or_else(|e| panic!("{e}"));
            store.build().unwrap_or_else(|e| panic!("{e}"));
        }

        let embedder = Arc::new(CountingEmbedder::new(8));
        let mut reopened =
            IndexStore::open_with_embeddings(repo.path(), index_dir.path(), embedder.clone())
                .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            reopened.embedded_chunk_count(),
            1,
            "persisted vector should have been reloaded on open"
        );

        reopened.build().unwrap_or_else(|e| panic!("{e}"));
        assert!(
            embedder.embedded_texts().is_empty(),
            "unchanged file must not be re-embedded just because the store was reopened"
        );
    }

    #[test]
    fn reopening_with_mismatched_embedder_id_triggers_full_reembed() {
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let index_dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        write(repo.path(), "a.rs", "fn alpha() { 1; }");

        {
            let embedder: Arc<dyn crate::embed::EmbeddingProvider> =
                Arc::new(crate::embed::MockEmbedder::new(8).with_id("model-v1"));
            let mut store =
                IndexStore::open_with_embeddings(repo.path(), index_dir.path(), embedder)
                    .unwrap_or_else(|e| panic!("{e}"));
            store.build().unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(store.embedded_chunk_count(), 1);
        }

        let embedder = Arc::new(CountingEmbedder::new(8));
        let mut reopened =
            IndexStore::open_with_embeddings(repo.path(), index_dir.path(), embedder.clone())
                .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            reopened.embedded_chunk_count(),
            0,
            "mismatched embedder id must invalidate the persisted vectors on open"
        );

        reopened.build().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(embedder.embedded_texts().len(), 1);
        assert_eq!(reopened.embedded_chunk_count(), 1);
    }

    #[test]
    fn status_counts_reads_metadata_without_tantivy_dir() {
        let index_dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let manifest = Manifest {
            version: MANIFEST_VERSION,
            files: HashMap::from([
                ("a.rs".to_owned(), "hash-a".to_owned()),
                ("b.rs".to_owned(), "hash-b".to_owned()),
            ]),
        };
        fs::write(
            index_dir.path().join(MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap_or_else(|e| panic!("{e}")),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        fs::write(
            index_dir.path().join(SYMBOLS_FILE),
            br#"[{"name":"a"},{"name":"b"},{"name":"c"}]"#,
        )
        .unwrap_or_else(|e| panic!("{e}"));

        let (files, symbols) =
            IndexStore::status_counts(index_dir.path()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(files, 2);
        assert_eq!(symbols, 3);
        assert!(
            !index_dir.path().join(TANTIVY_DIR).exists(),
            "status_counts must not create the tantivy directory"
        );
    }
}
