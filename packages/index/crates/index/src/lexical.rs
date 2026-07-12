//! BM25 full-text index over code chunks, backed by `tantivy`.
//!
//! Schema: `path`, `language`, `chunk` (the indexed+stored text), `start_line`,
//! `end_line`, and an optional `symbol` name field (boosted at query time by
//! [`crate::retrieve`]).

use std::path::Path;

use tantivy::directory::MmapDirectory;
use tantivy::query::QueryParser;
use tantivy::schema::{FAST, STORED, STRING, Schema, TEXT, Value};
use tantivy::{Index, IndexReader, IndexWriter, TantivyDocument, doc};

use crate::chunker::Chunk;

/// Writer heap size for tantivy segment building. Small, since this indexes
/// one repo's worth of code chunks, not a web-scale corpus.
const WRITER_HEAP_BYTES: usize = 50_000_000;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LexicalError {
    #[error("failed to open or create the tantivy index directory: {0}")]
    Directory(String),
    #[error("tantivy index error: {0}")]
    Tantivy(String),
}

impl From<tantivy::TantivyError> for LexicalError {
    fn from(err: tantivy::TantivyError) -> Self {
        LexicalError::Tantivy(err.to_string())
    }
}

/// Field handles for the fixed schema, resolved once at open time.
#[derive(Clone, Copy)]
struct Fields {
    path: tantivy::schema::Field,
    language: tantivy::schema::Field,
    chunk: tantivy::schema::Field,
    start_line: tantivy::schema::Field,
    end_line: tantivy::schema::Field,
    symbol: tantivy::schema::Field,
}

fn build_schema() -> (Schema, Fields) {
    let mut builder = Schema::builder();
    let path = builder.add_text_field("path", STRING | STORED | FAST);
    let language = builder.add_text_field("language", STRING | STORED);
    let chunk = builder.add_text_field("chunk", TEXT | STORED);
    let start_line = builder.add_u64_field("start_line", STORED | FAST);
    let end_line = builder.add_u64_field("end_line", STORED | FAST);
    let symbol = builder.add_text_field("symbol", TEXT | STORED);
    let schema = builder.build();
    (
        schema,
        Fields {
            path,
            language,
            chunk,
            start_line,
            end_line,
            symbol,
        },
    )
}

/// One BM25 hit straight from tantivy, before symbol-boost re-ranking.
#[derive(Debug, Clone)]
pub struct RawHit {
    pub path: String,
    pub language: String,
    pub chunk: String,
    pub start_line: usize,
    pub end_line: usize,
    pub symbol: Option<String>,
    pub score: f32,
}

/// A lexical (BM25) index over code chunks, persisted at a caller-chosen
/// directory.
pub struct LexicalIndex {
    index: Index,
    reader: IndexReader,
    fields: Fields,
}

impl LexicalIndex {
    /// Open the index at `dir`, creating it (and the directory) if absent.
    pub fn open_or_create(dir: &Path) -> Result<Self, LexicalError> {
        std::fs::create_dir_all(dir)
            .map_err(|err| LexicalError::Directory(format!("{}: {err}", dir.display())))?;
        let (schema, fields) = build_schema();
        let mmap_dir = MmapDirectory::open(dir)
            .map_err(|err| LexicalError::Directory(format!("{}: {err}", dir.display())))?;
        let index = Index::open_or_create(mmap_dir, schema)?;
        let reader = index.reader()?;
        Ok(Self {
            index,
            reader,
            fields,
        })
    }

    /// Replace the entire index contents with `chunks` (full rebuild).
    pub fn rebuild(
        &self,
        language_by_path: impl Fn(&str) -> &'static str,
        chunks: &[Chunk],
    ) -> Result<(), LexicalError> {
        let mut writer: IndexWriter = self.index.writer(WRITER_HEAP_BYTES)?;
        writer.delete_all_documents()?;
        for chunk in chunks {
            self.add_chunk(&mut writer, &language_by_path, chunk)?;
        }
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Remove every document for `path`, then re-add `chunks` for it.
    /// Used for incremental per-file updates.
    pub fn update_file(
        &self,
        path: &str,
        language: &'static str,
        chunks: &[Chunk],
    ) -> Result<(), LexicalError> {
        let mut writer: IndexWriter = self.index.writer(WRITER_HEAP_BYTES)?;
        let term = tantivy::Term::from_field_text(self.fields.path, path);
        writer.delete_term(term);
        for chunk in chunks {
            self.add_chunk(&mut writer, &|_| language, chunk)?;
        }
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Remove every document for `path`. Used when a file is deleted.
    pub fn remove_file(&self, path: &str) -> Result<(), LexicalError> {
        let mut writer: IndexWriter = self.index.writer(WRITER_HEAP_BYTES)?;
        let term = tantivy::Term::from_field_text(self.fields.path, path);
        writer.delete_term(term);
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    fn add_chunk(
        &self,
        writer: &mut IndexWriter,
        language_by_path: &impl Fn(&str) -> &'static str,
        chunk: &Chunk,
    ) -> Result<(), LexicalError> {
        let language = language_by_path(&chunk.path);
        let mut document = doc!(
            self.fields.path => chunk.path.clone(),
            self.fields.language => language,
            self.fields.chunk => chunk.text.clone(),
            self.fields.start_line => chunk.start_line as u64,
            self.fields.end_line => chunk.end_line as u64,
        );
        if let Some(symbol) = &chunk.symbol {
            document.add_text(self.fields.symbol, symbol);
        }
        writer.add_document(document)?;
        Ok(())
    }

    /// BM25 search over the `chunk` and `symbol` fields, returning the top
    /// `k` raw hits (before symbol-name boost merging, which lives in
    /// `retrieve`).
    pub fn search(&self, query: &str, k: usize) -> Result<Vec<RawHit>, LexicalError> {
        let searcher = self.reader.searcher();
        let query_parser =
            QueryParser::for_index(&self.index, vec![self.fields.chunk, self.fields.symbol]);
        let parsed = query_parser.parse_query_lenient(query).0;
        let collector = tantivy::collector::TopDocs::with_limit(k).order_by_score();
        let top_docs = searcher.search(&parsed, &collector)?;

        let mut hits = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let retrieved: TantivyDocument = searcher.doc(doc_address)?;
            hits.push(self.hit_from_doc(&retrieved, score));
        }
        Ok(hits)
    }

    /// All chunks currently indexed for `path`, in no particular order
    /// (`score` is always `0.0` — this isn't a ranked search). Used by
    /// [`crate::retrieve::search_hybrid`] to resolve a vector-only hit
    /// (found by the embedding index but not surfaced by the BM25 query
    /// itself, e.g. because it shares no literal terms with the query) back
    /// into full chunk data (line range, snippet, symbol) by chunk id.
    pub fn chunks_for_path(&self, path: &str) -> Result<Vec<RawHit>, LexicalError> {
        let searcher = self.reader.searcher();
        let term = tantivy::Term::from_field_text(self.fields.path, path);
        let query = tantivy::query::TermQuery::new(term, tantivy::schema::IndexRecordOption::Basic);
        // A file's chunk count is small and bounded by chunker clamps; no
        // realistic file needs more than a few hundred chunks back.
        let collector = tantivy::collector::TopDocs::with_limit(500).order_by_score();
        let top_docs = searcher.search(&query, &collector)?;

        let mut hits = Vec::with_capacity(top_docs.len());
        for (_score, doc_address) in top_docs {
            let retrieved: TantivyDocument = searcher.doc(doc_address)?;
            hits.push(self.hit_from_doc(&retrieved, 0.0));
        }
        Ok(hits)
    }

    fn hit_from_doc(&self, document: &TantivyDocument, score: f32) -> RawHit {
        let text_field = |field: tantivy::schema::Field| -> String {
            document
                .get_first(field)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_owned()
        };
        let u64_field = |field: tantivy::schema::Field| -> usize {
            document
                .get_first(field)
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize
        };
        let symbol = document
            .get_first(self.fields.symbol)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);

        RawHit {
            path: text_field(self.fields.path),
            language: text_field(self.fields.language),
            chunk: text_field(self.fields.chunk),
            start_line: u64_field(self.fields.start_line),
            end_line: u64_field(self.fields.end_line),
            symbol,
            score,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunker::Chunk;

    fn sample_chunk(path: &str, symbol: Option<&str>, text: &str) -> Chunk {
        Chunk {
            path: path.to_owned(),
            start_line: 1,
            end_line: text.lines().count().max(1),
            text: text.to_owned(),
            symbol: symbol.map(str::to_owned),
        }
    }

    #[test]
    fn indexes_and_finds_a_chunk() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let index = LexicalIndex::open_or_create(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        let chunks = vec![sample_chunk(
            "src/session.rs",
            Some("generate_session_title"),
            "fn generate_session_title(first_message: &str) -> String {\n    first_message.chars().take(50).collect()\n}",
        )];
        index
            .rebuild(|_| "rust", &chunks)
            .unwrap_or_else(|e| panic!("{e}"));

        let hits = index
            .search("session title", 5)
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(!hits.is_empty());
        assert_eq!(hits[0].path, "src/session.rs");
    }

    #[test]
    fn update_file_replaces_old_chunks() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let index = LexicalIndex::open_or_create(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        let original = vec![sample_chunk("a.rs", None, "fn alpha() {}")];
        index
            .rebuild(|_| "rust", &original)
            .unwrap_or_else(|e| panic!("{e}"));

        let updated = vec![sample_chunk("a.rs", None, "fn beta() {}")];
        index
            .update_file("a.rs", "rust", &updated)
            .unwrap_or_else(|e| panic!("{e}"));

        let hits = index.search("alpha", 5).unwrap_or_else(|e| panic!("{e}"));
        assert!(hits.is_empty(), "old content should be gone: {hits:?}");
        let hits = index.search("beta", 5).unwrap_or_else(|e| panic!("{e}"));
        assert!(!hits.is_empty());
    }

    #[test]
    fn remove_file_deletes_its_chunks() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let index = LexicalIndex::open_or_create(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        let chunks = vec![sample_chunk("gone.rs", None, "fn removed_thing() {}")];
        index
            .rebuild(|_| "rust", &chunks)
            .unwrap_or_else(|e| panic!("{e}"));
        index
            .remove_file("gone.rs")
            .unwrap_or_else(|e| panic!("{e}"));

        let hits = index
            .search("removed_thing", 5)
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(hits.is_empty());
    }
}
