use crate::symbols::{Symbol, extract_symbols};

const MIN_CHUNK_LINES: usize = 40;
const MAX_CHUNK_LINES: usize = 120;

const WINDOW_LINES: usize = 80;
const WINDOW_OVERLAP: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
    pub symbol: Option<String>,
}

impl Chunk {
    pub fn chunk_id(&self) -> String {
        chunk_id_of(&self.path, self.start_line, self.end_line)
    }
}

pub fn chunk_id_of(path: &str, start_line: usize, end_line: usize) -> String {
    let key = format!("{path}:{start_line}-{end_line}");
    blake3::hash(key.as_bytes()).to_hex().to_string()
}

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
    let mut covered_up_to = 0usize;

    let mut sorted: Vec<&Symbol> = symbols.iter().collect();
    sorted.sort_by_key(|s| s.start_line);

    for symbol in sorted {
        let start0 = symbol
            .start_line
            .saturating_sub(1)
            .min(total.saturating_sub(1));
        let mut end0 = symbol.end_line.min(total);

        if end0.saturating_sub(start0) > MAX_CHUNK_LINES {
            end0 = start0 + MAX_CHUNK_LINES;
        }
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

fn window_chunks(rel_path: &str, source: &str) -> Vec<Chunk> {
    window_chunks_from(rel_path, source, 0)
}

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
