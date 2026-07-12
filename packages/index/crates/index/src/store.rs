//! `IndexStore`: owns the on-disk lexical index plus a manifest of
//! `path -> content_hash` so re-running a build only re-processes files that
//! actually changed. The manifest and the symbol table are persisted as JSON
//! next to the tantivy directory.
//!
//! Embeddings are optional and additive: [`IndexStore::open`] never touches
//! vectors (pure BM25 + symbols, as in M1). [`IndexStore::open_with_embeddings`]
//! also opens a [`VectorStore`] and embeds new/changed chunks incrementally
//! as part of `build`/`update`, reusing the same per-file manifest diff that
//! already drives lexical incrementality — a file that didn't change never
//! gets re-chunked or re-embedded.

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

/// Bumped whenever the manifest or on-disk layout changes shape. v2 adds the
/// optional embeddings vector store alongside the manifest; older (v1)
/// manifests are not migrated in place — they trigger a clean rebuild (every
/// file is re-scanned as "new", which also naturally (re-)populates
/// embeddings for a store that now has an embedder configured).
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

/// Persisted manifest: schema version + per-file content hash, so a rebuild
/// can diff against the last run and only touch changed files.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    /// repo-relative path -> blake3 hex content hash at last index time.
    files: HashMap<String, String>,
}

/// Result of a build/update pass, useful for tests and progress logging.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStats {
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
    pub unchanged: usize,
}

/// Owns a lexical index + symbol table + manifest rooted at `index_dir`,
/// indexing source from `repo_root`. Optionally also owns a vector store and
/// embedding provider, for hybrid (BM25 + cosine) retrieval.
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
    /// Open an existing index at `index_dir` or create a fresh (empty) one.
    /// Does not scan `repo_root` — call [`Self::build`] or [`Self::update`]
    /// to populate it.
    ///
    /// No embedding provider is configured, so retrieval degrades to
    /// BM25 + symbol boost only (see [`crate::retrieve::search_hybrid`]).
    /// Use [`Self::open_with_embeddings`] to also embed chunks.
    pub fn open(
        repo_root: impl Into<PathBuf>,
        index_dir: impl Into<PathBuf>,
    ) -> Result<Self, StoreError> {
        Self::open_impl(repo_root.into(), index_dir.into(), None)
    }

    /// Like [`Self::open`], but also opens a per-repo vector store keyed by
    /// `embedder`'s id/dim, so `build`/`update` embed new/changed chunks and
    /// [`crate::retrieve::search_hybrid`] fuses in cosine ranks.
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

    /// Absolute path of the on-disk index directory (app-data, never inside
    /// the repo).
    pub fn index_dir(&self) -> &Path {
        &self.index_dir
    }

    /// Repo-relative paths currently tracked in the manifest, unsorted.
    pub fn indexed_paths(&self) -> impl Iterator<Item = &str> {
        self.manifest.files.keys().map(String::as_str)
    }

    /// The vector store, if an embedding provider was configured. Used by
    /// [`crate::retrieve::search_hybrid`]; empty/`None` means "degrade to
    /// BM25 + symbol boost".
    pub(crate) fn vector_store(&self) -> Option<&VectorStore> {
        self.embeddings.as_ref().map(|e| &e.store)
    }

    /// Embed `query` with the configured provider, if any.
    pub(crate) fn embed_query(&self, query: &str) -> Result<Option<Vec<f32>>, StoreError> {
        let Some(embeddings) = &self.embeddings else {
            return Ok(None);
        };
        let vectors = embeddings.provider.embed(&[query])?;
        Ok(vectors.into_iter().next())
    }

    /// Full (re)build: scan the repo, diff against the manifest, and
    /// re-index only files whose content hash changed (or are new).
    /// Files that vanished since the last manifest are dropped from the
    /// lexical index and the symbol table.
    ///
    /// A file whose *content* is unchanged but that has no vectors yet
    /// (e.g. right after an embedder switch invalidated the vector store,
    /// or embeddings were just enabled for a previously BM25-only index) is
    /// still re-embedded, even though its lexical/symbol data is left
    /// alone — this is what lets the index "self-heal" its vectors on the
    /// very next `build()` after invalidation, rather than requiring every
    /// file to change first.
    pub fn build(&mut self) -> Result<UpdateStats, StoreError> {
        let scanned = scan_repo(&self.repo_root);
        let mut stats = UpdateStats::default();
        let mut seen = std::collections::HashSet::new();

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
        }

        // Drop files that disappeared since the last manifest.
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

    /// Incremental update over an explicit set of repo-relative paths
    /// (e.g. from a file-watcher). Paths that no longer exist on disk are
    /// treated as deletions; everything else is re-hashed and re-indexed
    /// only if the hash actually changed.
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
            // Unreadable as UTF-8 text after all (race, or a sniff false
            // negative) — treat as untouchable rather than erroring the
            // whole build.
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

    /// Embed only the chunks of `rel_path` that aren't already in the vector
    /// store under their (stable, line-range-derived) chunk id — a no-op
    /// when no embedder is configured. Stale vectors for chunk ids that no
    /// longer exist in `chunks` (e.g. the file shrank) are dropped first, so
    /// a changed file never leaves orphaned vectors behind.
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

    /// For a file whose manifest hash is unchanged (so `reindex_file` was
    /// skipped): if an embedder is configured and any of its chunks are
    /// missing from the vector store, (re-)chunk and embed just the missing
    /// ones. A no-op — no file read, no chunking — when no embedder is
    /// configured, or when every chunk already has a vector.
    fn embed_if_missing(&mut self, rel_path: &str) -> Result<(), StoreError> {
        if self.embeddings.is_none() {
            return Ok(());
        }
        let abs = self.repo_root.join(rel_path);
        let Ok(source) = fs::read_to_string(&abs) else {
            return Ok(());
        };
        let chunks = chunk_file(rel_path, &source);

        // Two short, non-overlapping borrows of `self.embeddings` (read to
        // find what's missing, then write the new vectors) rather than one
        // long-lived mutable borrow across the `embed` call.
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

    /// Manifest content-hash for `rel_path`, if it has been indexed.
    pub fn manifest_hash(&self, rel_path: &str) -> Option<&str> {
        self.manifest.files.get(rel_path).map(String::as_str)
    }

    /// Number of files currently tracked in the manifest.
    pub fn indexed_file_count(&self) -> usize {
        self.manifest.files.len()
    }

    /// Number of chunk vectors currently in the vector store (`0` if no
    /// embedder is configured). Exposed mainly for tests asserting
    /// incremental-embedding behavior.
    pub fn embedded_chunk_count(&self) -> usize {
        self.embeddings.as_ref().map(|e| e.store.len()).unwrap_or(0)
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
        // Schema drift: start fresh rather than trust stale hashes.
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

        // Re-build with no changes: everything should be "unchanged".
        let stats = store.build().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(stats.added, 0);
        assert_eq!(stats.changed, 0);
        assert_eq!(stats.unchanged, 2);

        // Touch only `a.rs`.
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

    /// Wraps [`crate::embed::MockEmbedder`], recording every text handed to
    /// `embed` (in call order) — lets a test assert *which* chunks were
    /// (re-)embedded on a given `build`/`update` call, distinct from merely
    /// observing the resulting vector count.
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

        /// All texts embedded so far, across every `embed` call.
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

        // Rebuild with nothing changed: no new embed calls at all.
        store.build().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            embedder.embedded_texts().len(),
            first_pass.len(),
            "no file changed, so no chunk should be re-embedded"
        );

        // Touch only a.rs.
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

        // Reopen with a *fresh* embedder instance sharing the same id/dim —
        // build() should not need to re-embed anything (the manifest still
        // says "unchanged" for a.rs, so `reindex_file`/`embed_chunks` never
        // runs), and the persisted vectors should already be there.
        let embedder = Arc::new(CountingEmbedder::new(8)); // id: "mock", dim: 8 — matches.
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

        // Reopen with a different embedder id (a "model switch"): the vector
        // store must invalidate wholesale rather than silently mixing
        // vectors from two different models.
        let embedder = Arc::new(CountingEmbedder::new(8)); // id: "mock" != "model-v1".
        let mut reopened =
            IndexStore::open_with_embeddings(repo.path(), index_dir.path(), embedder.clone())
                .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            reopened.embedded_chunk_count(),
            0,
            "mismatched embedder id must invalidate the persisted vectors on open"
        );

        // The file-content manifest still says "unchanged", so `reindex_file`
        // is skipped for it — but `build()` separately self-heals any
        // manifest-tracked file that has no vector yet (see
        // `embed_if_missing`), so the very next build repopulates it under
        // the new embedder without requiring the file itself to change.
        reopened.build().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(embedder.embedded_texts().len(), 1);
        assert_eq!(reopened.embedded_chunk_count(), 1);
    }
}
