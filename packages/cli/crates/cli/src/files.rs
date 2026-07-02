//! Workdir-scoped file index for `@` mention autocomplete.
//!
//! Mentions support an optional line slice suffix: `@path/to/file:[0:12]`
//! (Python-style 0-based indices; end is exclusive — `[0:12]` is the first 12 lines).

use std::path::{Path, PathBuf};

use thiserror::Error;

/// Maximum files indexed under the workdir (keeps startup and memory bounded).
const MAX_INDEX_FILES: usize = 20_000;

/// Maximum lines rendered in the inline slice preview panel.
pub const MENTION_PREVIEW_MAX_LINES: usize = 12;

/// 0-based half-open line range (`[0:12]` → indices 0..12, displayed as lines 1–12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineSlice {
    /// 0-based start index (inclusive).
    pub start: usize,
    /// 0-based end index (exclusive). `None` = through EOF.
    pub end: Option<usize>,
}

/// Loaded lines for the inline preview panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionPreview {
    pub path: String,
    pub label: String,
    pub lines: Vec<(usize, String)>,
    pub total_lines: usize,
    pub truncated: bool,
    pub error: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SliceParseError {
    #[error("slice must start with '['")]
    MissingOpen,
    #[error("slice is incomplete")]
    Incomplete,
    #[error("invalid line number: {0}")]
    InvalidNumber(String),
    #[error("start line must be ≤ end line")]
    InvertedRange,
}

#[derive(Debug, Error)]
pub enum FileReadError {
    #[error("not a file: {0}")]
    NotFile(PathBuf),
    #[error("failed to read {0}: {1}")]
    Io(PathBuf, String),
    #[error("file is not valid UTF-8: {0}")]
    NotUtf8(PathBuf),
}

/// Relative paths of files under the workdir, sorted for stable display.
#[derive(Debug, Clone, Default)]
pub struct FileIndex {
    paths: Vec<String>,
}

#[derive(Debug, Error)]
pub enum FileIndexError {
    #[error("workdir does not exist: {0}")]
    MissingRoot(PathBuf),
    #[error("failed to walk workdir: {0}")]
    Walk(String),
}

impl FileIndex {
    /// Build a gitignore-aware index of files under `workdir`.
    pub fn build(workdir: &Path) -> Result<Self, FileIndexError> {
        if !workdir.is_dir() {
            return Err(FileIndexError::MissingRoot(workdir.to_path_buf()));
        }
        let workdir = workdir.to_path_buf();
        let paths = std::thread::spawn(move || walk_files(&workdir))
            .join()
            .map_err(|_| FileIndexError::Walk("index worker panicked".to_owned()))??;
        Ok(Self { paths })
    }

    /// Construct an index from explicit relative paths (tests and snapshots).
    pub fn from_paths(paths: Vec<String>) -> Self {
        let mut paths = paths;
        paths.sort();
        paths.dedup();
        Self { paths }
    }

    /// All indexed relative paths.
    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    /// Filter paths for an `@` mention query (case-insensitive).
    pub fn matches(&self, query: &str) -> Vec<String> {
        let query = query.trim();
        if query.is_empty() {
            return self.paths.iter().take(50).cloned().collect();
        }
        let filter = query.to_lowercase();
        let mut ranked: Vec<(u8, &String)> = self
            .paths
            .iter()
            .filter_map(|path| score_match(&filter, path).map(|rank| (rank, path)))
            .collect();
        ranked.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.len().cmp(&b.1.len()))
                .then_with(|| a.1.cmp(b.1))
        });
        ranked
            .into_iter()
            .take(50)
            .map(|(_, path)| path.clone())
            .collect()
    }
}

fn walk_files(workdir: &Path) -> Result<Vec<String>, FileIndexError> {
    let mut paths = Vec::new();
    for entry in ignore::WalkBuilder::new(workdir)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .parents(true)
        .build()
    {
        let entry = entry.map_err(|err| FileIndexError::Walk(err.to_string()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let rel = path
            .strip_prefix(workdir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        paths.push(rel);
        if paths.len() >= MAX_INDEX_FILES {
            break;
        }
    }
    paths.sort();
    Ok(paths)
}

/// Lower rank is better. `None` means no match.
fn score_match(filter: &str, path: &str) -> Option<u8> {
    let lower = path.to_lowercase();
    if lower.starts_with(filter) {
        return Some(0);
    }
    if lower.contains(filter) {
        return Some(1);
    }
    if is_subsequence(filter, &lower) {
        return Some(2);
    }
    None
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut chars = haystack.chars();
    needle.chars().all(|n| chars.any(|h| h == n))
}

/// Byte span of the `@` token under the cursor, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionSpan {
    pub start: usize,
    pub end: usize,
    pub query: String,
}

/// Find the active `@` mention token containing `cursor` (byte index in `text`).
pub fn active_mention(text: &str, cursor: usize) -> Option<MentionSpan> {
    let cursor = cursor.min(text.len());
    let before = &text[..cursor];
    let at = before.rfind('@')?;
    if at > 0 {
        let prev = text.as_bytes()[at - 1];
        if !prev.is_ascii_whitespace() {
            return None;
        }
    }
    let token_end = mention_token_end(text, at);
    if cursor > token_end {
        return None;
    }
    let query = text[at + 1..cursor].to_owned();
    if query.contains(char::is_whitespace) {
        return None;
    }
    Some(MentionSpan {
        start: at,
        end: token_end,
        query,
    })
}

/// End byte index of an `@` token starting at `start`.
pub fn mention_token_end(text: &str, start: usize) -> usize {
    text[start..]
        .find(char::is_whitespace)
        .map(|offset| start + offset)
        .unwrap_or(text.len())
}

/// Replace `span` in `text` with `replacement` (byte indices).
pub fn replace_mention(text: &str, span: &MentionSpan, replacement: &str) -> String {
    format!(
        "{}{}{}",
        &text[..span.start],
        replacement,
        &text[span.end..]
    )
}

/// Split `@` query into fuzzy-match path prefix and optional `:[slice]` suffix.
pub fn split_mention_query(query: &str) -> (&str, Option<&str>) {
    let Some(colon) = query.find(':') else {
        return (query, None);
    };
    let after = &query[colon + 1..];
    if after.starts_with('[') {
        (&query[..colon], Some(after))
    } else {
        (query, None)
    }
}

/// Parse `:[0:12]`, `[:]`, `[20:]`, etc. (0-based indices; end exclusive).
pub fn parse_line_slice(raw: &str) -> Result<LineSlice, SliceParseError> {
    let raw = raw.trim();
    if !raw.starts_with('[') {
        return Err(SliceParseError::MissingOpen);
    }
    if !raw.ends_with(']') {
        return Err(SliceParseError::Incomplete);
    }
    let inner = &raw[1..raw.len() - 1];
    if inner.is_empty() || inner == ":" {
        return Ok(LineSlice {
            start: 0,
            end: None,
        });
    }
    let (start_str, end_str) = match inner.split_once(':') {
        Some(parts) => parts,
        None => {
            let index = parse_slice_index(inner)?;
            return Ok(LineSlice {
                start: index,
                end: Some(index + 1),
            });
        }
    };
    let start = if start_str.is_empty() {
        0
    } else {
        parse_slice_index(start_str)?
    };
    let end = if end_str.is_empty() {
        None
    } else {
        Some(parse_slice_index(end_str)?)
    };
    if let Some(end_idx) = end {
        if start > end_idx {
            return Err(SliceParseError::InvertedRange);
        }
    }
    Ok(LineSlice { start, end })
}

fn parse_slice_index(raw: &str) -> Result<usize, SliceParseError> {
    raw.parse()
        .map_err(|_| SliceParseError::InvalidNumber(raw.to_owned()))
}

/// Human-readable slice label for UI (`lines 1–12`, `lines 21–end`, …).
pub fn slice_label(slice: &LineSlice) -> String {
    let first_line = slice.start + 1;
    match slice.end {
        None if slice.start == 0 => "all lines".to_owned(),
        None => format!("lines {first_line}–end"),
        Some(end) if end == slice.start + 1 => format!("line {first_line}"),
        Some(end) => format!("lines {first_line}–{end}"),
    }
}

/// Pick the file path for preview / expansion.
pub fn resolve_mention_path(
    workdir: &Path,
    path_part: &str,
    matches: &[String],
    selected: usize,
) -> Option<String> {
    if !path_part.is_empty() {
        let candidate = workdir.join(path_part);
        if candidate.is_file() {
            return Some(path_part.replace('\\', "/"));
        }
    }
    if matches.len() == 1 {
        return Some(matches[0].clone());
    }
    matches.get(selected).cloned()
}

/// Read a line range from `workdir` / `rel_path`.
pub fn read_line_range(
    workdir: &Path,
    rel_path: &str,
    slice: &LineSlice,
) -> Result<Vec<(usize, String)>, FileReadError> {
    let full = workdir.join(rel_path);
    if !full.is_file() {
        return Err(FileReadError::NotFile(full));
    }
    let raw = std::fs::read_to_string(&full)
        .map_err(|err| FileReadError::Io(full.clone(), err.to_string()))?;
    let all: Vec<&str> = raw.lines().collect();
    let total = all.len();
    if total == 0 {
        return Ok(Vec::new());
    }
    let start = slice.start.min(total);
    let end = slice.end.unwrap_or(total).min(total);
    if start >= end {
        return Ok(Vec::new());
    }
    Ok(all
        .iter()
        .enumerate()
        .skip(start)
        .take(end - start)
        .map(|(idx, line)| (idx + 1, line.to_string()))
        .collect())
}

/// Build preview state for the inline panel (truncates to [`MENTION_PREVIEW_MAX_LINES`]).
pub fn build_mention_preview(workdir: &Path, path: &str, slice: &LineSlice) -> MentionPreview {
    let label = slice_label(slice);
    match read_line_range(workdir, path, slice) {
        Ok(mut lines) => {
            let total_lines = lines.len();
            let truncated = total_lines > MENTION_PREVIEW_MAX_LINES;
            lines.truncate(MENTION_PREVIEW_MAX_LINES);
            MentionPreview {
                path: path.to_owned(),
                label,
                lines,
                total_lines,
                truncated,
                error: None,
            }
        }
        Err(err) => MentionPreview {
            path: path.to_owned(),
            label,
            lines: Vec::new(),
            total_lines: 0,
            truncated: false,
            error: Some(err.to_string()),
        },
    }
}

/// Expand `@path:[slice]` tokens to include the referenced source in the prompt.
pub fn expand_file_mentions(text: &str, workdir: &Path, file_index: &FileIndex) -> String {
    let mut out = String::new();
    let mut idx = 0;
    while idx < text.len() {
        let Some(rel_at) = text[idx..].find('@') else {
            out.push_str(&text[idx..]);
            break;
        };
        let at = idx + rel_at;
        if at > 0 && !text.as_bytes()[at - 1].is_ascii_whitespace() {
            out.push('@');
            idx = at + 1;
            continue;
        }
        out.push_str(&text[idx..at]);
        let token_end = mention_token_end(text, at);
        let token = &text[at + 1..token_end];
        let (path_part, slice_part) = split_mention_query(token);
        let matches = file_index.matches(path_part);
        let expanded = slice_part
            .and_then(|slice_raw| parse_line_slice(slice_raw).ok())
            .and_then(|slice| {
                resolve_mention_path(workdir, path_part, &matches, 0).and_then(|path| {
                    read_line_range(workdir, &path, &slice)
                        .ok()
                        .map(|lines| format_mention_expansion(&path, &slice, &lines))
                })
            });
        if let Some(body) = expanded {
            out.push_str(&body);
        } else {
            out.push('@');
            out.push_str(token);
        }
        idx = token_end;
    }
    out
}

fn format_mention_expansion(path: &str, slice: &LineSlice, lines: &[(usize, String)]) -> String {
    let mut out = format!("Referenced file `{path}` ({})", slice_label(slice));
    if lines.is_empty() {
        out.push_str(" — (empty range)\n");
        return out;
    }
    out.push_str(":\n```\n");
    for (num, line) in lines {
        out.push_str(&format!("{num:>4} | {line}\n"));
    }
    out.push_str("```\n");
    out
}

/// Cursor byte offset from textarea `(row, col)` character positions.
pub fn cursor_byte_offset(lines: &[String], (row, col): (usize, usize)) -> usize {
    let mut offset = 0usize;
    for (index, line) in lines.iter().enumerate() {
        if index == row {
            let byte_col = line.chars().take(col).map(char::len_utf8).sum::<usize>();
            return offset + byte_col;
        }
        offset += line.len() + 1;
    }
    offset
}

/// `(row, col)` character position for a byte offset in joined textarea text.
pub fn byte_offset_to_cursor(lines: &[String], offset: usize) -> (usize, usize) {
    let mut remaining = offset;
    for (row, line) in lines.iter().enumerate() {
        let line_len = line.len();
        if remaining <= line_len {
            let col = line[..remaining].chars().count();
            return (row, col);
        }
        remaining -= line_len + 1;
    }
    let row = lines.len().saturating_sub(1);
    let col = lines.last().map(|line| line.chars().count()).unwrap_or(0);
    (row, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_mention_at_end_of_query() {
        let text = "fix bug in @src/ap";
        let span = active_mention(text, text.len()).expect("mention");
        assert_eq!(span.start, 11);
        assert_eq!(span.query, "src/ap");
    }

    #[test]
    fn active_mention_requires_boundary() {
        let text = "email user@domain.com";
        assert!(active_mention(text, text.len()).is_none());
    }

    #[test]
    fn active_mention_multiple_in_line() {
        let text = "see @src/a.rs and @src/b.rs";
        let cursor = text.len();
        let span = active_mention(text, cursor).expect("mention");
        assert_eq!(span.query, "src/b.rs");
    }

    #[test]
    fn replace_mention_inserts_path() {
        let text = "check @src/f";
        let span = active_mention(text, text.len()).expect("mention");
        let updated = replace_mention(text, &span, "@src/foo.rs ");
        assert_eq!(updated, "check @src/foo.rs ");
    }

    #[test]
    fn file_index_prefix_then_substring() {
        let index = FileIndex::from_paths(vec![
            "src/lib.rs".to_owned(),
            "src/main.rs".to_owned(),
            "README.md".to_owned(),
        ]);
        let matches = index.matches("src/l");
        assert_eq!(matches.first().map(String::as_str), Some("src/lib.rs"));
        assert!(matches.contains(&"src/lib.rs".to_owned()));
    }

    #[test]
    fn file_index_subsequence_match() {
        let index = FileIndex::from_paths(vec!["packages/cli/src/input.rs".to_owned()]);
        let matches = index.matches("pcinp");
        assert_eq!(matches, vec!["packages/cli/src/input.rs".to_owned()]);
    }

    #[test]
    fn cursor_byte_offset_round_trip_single_line() {
        let lines = vec!["hello @src".to_owned()];
        let offset = cursor_byte_offset(&lines, (0, 9));
        assert_eq!(offset, 9);
        assert_eq!(byte_offset_to_cursor(&lines, offset), (0, 9));
    }

    #[test]
    fn parse_line_slice_variants() {
        assert_eq!(
            parse_line_slice("[:]").unwrap(),
            LineSlice {
                start: 0,
                end: None
            }
        );
        assert_eq!(
            parse_line_slice("[0:12]").unwrap(),
            LineSlice {
                start: 0,
                end: Some(12)
            }
        );
        assert_eq!(
            parse_line_slice("[20:100]").unwrap(),
            LineSlice {
                start: 20,
                end: Some(100)
            }
        );
        assert_eq!(
            parse_line_slice("[20:]").unwrap(),
            LineSlice {
                start: 20,
                end: None
            }
        );
        assert_eq!(
            parse_line_slice("[:50]").unwrap(),
            LineSlice {
                start: 0,
                end: Some(50)
            }
        );
        assert!(matches!(
            parse_line_slice("[20:100"),
            Err(SliceParseError::Incomplete)
        ));
    }

    #[test]
    fn split_mention_query_splits_slice() {
        let (path, slice) = split_mention_query("src/a.rs:[20:30]");
        assert_eq!(path, "src/a.rs");
        assert_eq!(slice, Some("[20:30]"));
    }

    #[test]
    fn read_line_range_uses_zero_based_half_open_slices() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("sample.txt");
        std::fs::write(&file, "a\nb\nc\nd\ne\n").expect("write");
        let slice = LineSlice {
            start: 1,
            end: Some(4),
        };
        let lines = read_line_range(dir.path(), "sample.txt", &slice).expect("read");
        assert_eq!(
            lines,
            vec![
                (2, "b".to_owned()),
                (3, "c".to_owned()),
                (4, "d".to_owned()),
            ]
        );
        let first_twelve = LineSlice {
            start: 0,
            end: Some(12),
        };
        let all = read_line_range(dir.path(), "sample.txt", &first_twelve).expect("read");
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn expand_file_mentions_inlines_slice_content() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("README.md"), "one\ntwo\nthree\n").expect("write");
        let index = FileIndex::from_paths(vec!["README.md".to_owned()]);
        let expanded = expand_file_mentions("summarize @README.md:[0:2]", dir.path(), &index);
        assert!(expanded.contains("Referenced file `README.md`"));
        assert!(expanded.contains("one"));
        assert!(expanded.contains("two"));
        assert!(!expanded.contains("three"));
    }
}
