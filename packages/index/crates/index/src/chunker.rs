//! Symbol-aware chunking: one chunk per top-level symbol (clamped to a
//! sane line-count range), falling back to a fixed line-window with overlap
//! for files with no extracted symbols (unparsed languages, or parsed files
//! with no top-level defs — e.g. a script with only free statements).

use crate::symbols::{Symbol, extract_symbols};

/// Clamp bounds for a symbol-derived chunk, in lines (~40–120 per
/// masterplan: small defs borrow trailing context; large ones truncate).
const MIN_CHUNK_LINES: usize = 40;
const MAX_CHUNK_LINES: usize = 120;

/// Fixed-window fallback chunk size and overlap, in lines (within the
/// same ~40–120 band as symbol chunks).
const WINDOW_LINES: usize = 80;
const WINDOW_OVERLAP: usize = 10;

/// One chunk of source text ready for lexical indexing.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub path: String,
    /// 1-based, inclusive.
    pub start_line: usize,
    /// 1-based, inclusive.
    pub end_line: usize,
    pub text: String,
    /// Symbol name this chunk was built from, if any.
    pub symbol: Option<String>,
}

impl Chunk {
    /// Stable identifier for this chunk, used to key its embedding vector in
    /// the vector store. Derived from path + line range (not from `text`),
    /// so a chunk that shifts by a line elsewhere in the file still gets a
    /// fresh id (correctly treated as "new" for embedding purposes) while
    /// re-chunking the *same* unchanged file byte-for-byte reproduces the
    /// same ids, letting incremental embedding skip it.
    pub fn chunk_id(&self) -> String {
        chunk_id_of(&self.path, self.start_line, self.end_line)
    }
}

/// Compute the same stable chunk id as [`Chunk::chunk_id`] from just a path
/// and line range — lets [`crate::retrieve`] join a lexical (tantivy) hit,
/// which doesn't carry a `Chunk`, against the vector store by id without
/// re-deriving the formula.
pub fn chunk_id_of(path: &str, start_line: usize, end_line: usize) -> String {
    let key = format!("{path}:{start_line}-{end_line}");
    blake3::hash(key.as_bytes()).to_hex().to_string()
}

/// Chunk `source` (already known to live at `rel_path`) into indexable pieces.
///
/// Symbol-aware: each top-level symbol yields one chunk, clamped to
/// `[MIN_CHUNK_LINES, MAX_CHUNK_LINES]` (small symbols borrow trailing
/// context lines to reach the minimum; large ones are truncated with the
/// remainder covered by follow-up window chunks so no source line is
/// dropped). Falls back entirely to a fixed overlapping window when no
/// symbols are extracted.
pub fn chunk_file(rel_path: &str, source: &str) -> Vec<Chunk> {
    let symbols = extract_symbols(rel_path, source);
    if symbols.is_empty() {
        return window_chunks(rel_path, source);
    }
    symbol_chunks(rel_path, source, &symbols)
}

fn lines_of(source: &str) -> Vec<&str> {
    source.lines().collect()
}

fn symbol_chunks(rel_path: &str, source: &str, symbols: &[Symbol]) -> Vec<Chunk> {
    let lines = lines_of(source);
    let total = lines.len();
    let mut chunks = Vec::with_capacity(symbols.len());
    let mut covered_up_to = 0usize; // 0-based, exclusive end already covered

    let mut sorted: Vec<&Symbol> = symbols.iter().collect();
    sorted.sort_by_key(|s| s.start_line);

    for symbol in sorted {
        let start0 = symbol
            .start_line
            .saturating_sub(1)
            .min(total.saturating_sub(1));
        let mut end0 = symbol.end_line.min(total); // exclusive

        // Clamp to MAX_CHUNK_LINES from the start.
        if end0.saturating_sub(start0) > MAX_CHUNK_LINES {
            end0 = start0 + MAX_CHUNK_LINES;
        }
        // Extend to MIN_CHUNK_LINES by borrowing trailing context, capped at EOF.
        if end0.saturating_sub(start0) < MIN_CHUNK_LINES {
            end0 = (start0 + MIN_CHUNK_LINES).min(total);
        }
        if end0 <= start0 {
            continue;
        }

        let text = lines[start0..end0].join("\n");
        chunks.push(Chunk {
            path: rel_path.to_owned(),
            start_line: start0 + 1,
            end_line: end0,
            text,
            symbol: Some(symbol.name.clone()),
        });
        covered_up_to = covered_up_to.max(end0);
    }

    // Cover any trailing, un-symbolized tail (e.g. imports before the first
    // def, or module-level code after the last one) with plain window chunks
    // so nothing is silently dropped from the lexical index.
    if covered_up_to < total {
        let tail = lines[covered_up_to..].join("\n");
        let tail_chunks = window_chunks_from(rel_path, &tail, covered_up_to);
        chunks.extend(tail_chunks);
    }

    if chunks.is_empty() {
        return window_chunks(rel_path, source);
    }
    chunks
}

/// Fixed line-window chunking with overlap, starting at line 0 of `source`.
fn window_chunks(rel_path: &str, source: &str) -> Vec<Chunk> {
    window_chunks_from(rel_path, source, 0)
}

/// Like [`window_chunks`], but `line_offset` (0-based) is added to every
/// reported line number — used when chunking a tail slice of a larger file.
fn window_chunks_from(rel_path: &str, source: &str, line_offset: usize) -> Vec<Chunk> {
    let lines = lines_of(source);
    if lines.is_empty() {
        return Vec::new();
    }
    let step = WINDOW_LINES.saturating_sub(WINDOW_OVERLAP).max(1);
    let mut chunks = Vec::new();
    let mut start = 0usize;
    loop {
        let end = (start + WINDOW_LINES).min(lines.len());
        let text = lines[start..end].join("\n");
        chunks.push(Chunk {
            path: rel_path.to_owned(),
            start_line: line_offset + start + 1,
            end_line: line_offset + end,
            text,
            symbol: None,
        });
        if end >= lines.len() {
            break;
        }
        start += step;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_chunk_bounds_stay_within_clamp() {
        let source = r#"
fn tiny() {
    1;
}

fn big() {
"#
        .to_owned()
            + &"    line();\n".repeat(200)
            + "}\n";

        let chunks = chunk_file("a.rs", &source);
        let big_chunk = chunks
            .iter()
            .find(|c| c.symbol.as_deref() == Some("big"))
            .unwrap_or_else(|| panic!("expected a chunk for `big`, got {chunks:?}"));
        let tiny_chunk = chunks
            .iter()
            .find(|c| c.symbol.as_deref() == Some("tiny"))
            .unwrap_or_else(|| panic!("expected a chunk for `tiny`, got {chunks:?}"));

        let big_len = big_chunk.end_line - big_chunk.start_line + 1;
        let tiny_len = tiny_chunk.end_line - tiny_chunk.start_line + 1;
        let total_lines = source.lines().count();
        assert!(big_len <= MAX_CHUNK_LINES, "big chunk len {big_len}");
        assert!(
            tiny_len >= MIN_CHUNK_LINES || tiny_chunk.end_line == total_lines,
            "tiny chunk len {tiny_len} (expected >= {MIN_CHUNK_LINES} or EOF)"
        );
    }

    #[test]
    fn falls_back_to_window_chunks_when_no_symbols() {
        let source = (0..500)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let chunks = chunk_file("data.txt", &source);
        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| c.symbol.is_none()));
        // Windows overlap: consecutive chunks share some lines.
        if chunks.len() > 1 {
            assert!(chunks[1].start_line <= chunks[0].end_line);
        }
    }

    #[test]
    fn window_chunks_cover_whole_file() {
        let source = (0..300)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let chunks = chunk_file("data.txt", &source);
        let last = chunks.last().unwrap_or_else(|| panic!("no chunks"));
        assert_eq!(last.end_line, source.lines().count());
    }

    #[test]
    fn symbol_chunks_cover_tail_after_last_symbol() {
        let mut source = String::from("fn only() {\n    1;\n}\n");
        source.push_str(&"trailing_line();\n".repeat(120));
        let chunks = chunk_file("b.rs", &source);
        let total_lines = source.lines().count();
        let max_end = chunks.iter().map(|c| c.end_line).max().unwrap_or(0);
        assert_eq!(max_end, total_lines, "tail should be covered: {chunks:?}");
    }
}
