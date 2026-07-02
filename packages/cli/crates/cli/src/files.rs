//! Workdir-scoped file index for `@` mention autocomplete.

use std::path::{Path, PathBuf};

use thiserror::Error;

/// Maximum files indexed under the workdir (keeps startup and memory bounded).
const MAX_INDEX_FILES: usize = 20_000;

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
}
