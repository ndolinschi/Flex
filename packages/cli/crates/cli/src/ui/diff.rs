//! Line diffs for Edit/Write tool rows (and the permission dialog preview).

use similar::{ChangeTag, TextDiff};

/// How many lines the collapsed diff preview may show.
pub(crate) const DIFF_PREVIEW_MAX_LINES: usize = 4;

/// One rendered diff line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiffLine {
    pub kind: DiffKind,
    /// 1-based line number: old side for `Del`/`Ctx`, new side for `Add`.
    pub line_no: Option<usize>,
    pub text: String,
}

/// Diff line category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffKind {
    Del,
    Add,
    Ctx,
}

/// A computed diff plus the size of its first hunk (the collapsed preview).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiffPreview {
    /// All hunk lines in order (context trimmed to `context` per side).
    pub lines: Vec<DiffLine>,
    /// Lines belonging to the first hunk; the collapsed preview shows at
    /// most [`DIFF_PREVIEW_MAX_LINES`] of them.
    pub first_hunk_len: usize,
}

impl DiffPreview {
    /// How many lines the collapsed preview shows.
    pub(crate) fn preview_len(&self) -> usize {
        self.first_hunk_len.min(DIFF_PREVIEW_MAX_LINES)
    }
}

/// Diff `old` → `new` with `context` unchanged lines around each change.
///
/// Not yet called outside tests: the permission-dialog diff preview (a
/// later phase) renders through this entry point.
#[allow(dead_code)]
pub(crate) fn diff_lines(old: &str, new: &str, context: usize) -> Vec<DiffLine> {
    diff_preview(old, new, context).lines
}

/// Diff `old` → `new`, keeping the first-hunk boundary for collapsed rows.
pub(crate) fn diff_preview(old: &str, new: &str, context: usize) -> DiffPreview {
    let diff = TextDiff::from_lines(old, new);
    let mut lines = Vec::new();
    let mut first_hunk_len = 0;
    for (hunk_idx, group) in diff.grouped_ops(context).iter().enumerate() {
        for op in group {
            for change in diff.iter_changes(op) {
                let (kind, line_no) = match change.tag() {
                    ChangeTag::Delete => (DiffKind::Del, change.old_index()),
                    ChangeTag::Insert => (DiffKind::Add, change.new_index()),
                    ChangeTag::Equal => (DiffKind::Ctx, change.old_index()),
                };
                lines.push(DiffLine {
                    kind,
                    line_no: line_no.map(|idx| idx + 1),
                    text: change.value().trim_end_matches('\n').to_owned(),
                });
            }
        }
        if hunk_idx == 0 {
            first_hunk_len = lines.len();
        }
    }
    DiffPreview {
        lines,
        first_hunk_len,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line_replacement() {
        let lines = diff_lines("let x = old();\n", "let x = new();\n", 1);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].kind, DiffKind::Del);
        assert_eq!(lines[0].line_no, Some(1));
        assert_eq!(lines[0].text, "let x = old();");
        assert_eq!(lines[1].kind, DiffKind::Add);
        assert_eq!(lines[1].line_no, Some(1));
        assert_eq!(lines[1].text, "let x = new();");
    }

    #[test]
    fn context_lines_surround_changes() {
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nb\nC\nd\ne\n";
        let lines = diff_lines(old, new, 1);
        let kinds: Vec<DiffKind> = lines.iter().map(|line| line.kind).collect();
        assert_eq!(
            kinds,
            vec![DiffKind::Ctx, DiffKind::Del, DiffKind::Add, DiffKind::Ctx]
        );
        assert_eq!(lines[0].text, "b");
        assert_eq!(lines[3].text, "d");
    }

    #[test]
    fn whole_file_insert_is_all_adds() {
        let preview = diff_preview("", "line one\nline two\n", 1);
        assert!(preview.lines.iter().all(|line| line.kind == DiffKind::Add));
        assert_eq!(preview.lines.len(), 2);
        assert_eq!(preview.first_hunk_len, 2);
    }

    #[test]
    fn first_hunk_len_stops_at_hunk_boundary() {
        // Two edits far apart produce two hunks at context 1.
        let old = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";
        let new = "one\n2\n3\n4\n5\n6\n7\n8\n9\nten\n";
        let preview = diff_preview(old, new, 1);
        assert!(preview.first_hunk_len < preview.lines.len());
        // First hunk: -1 +one, plus one context line below.
        assert_eq!(preview.first_hunk_len, 3);
        assert_eq!(preview.preview_len(), 3);
    }

    #[test]
    fn preview_len_caps_large_first_hunk() {
        let preview = diff_preview("", "a\nb\nc\nd\ne\nf\n", 1);
        assert_eq!(preview.first_hunk_len, 6);
        assert_eq!(preview.preview_len(), DIFF_PREVIEW_MAX_LINES);
    }

    #[test]
    fn identical_inputs_produce_no_lines() {
        let preview = diff_preview("same\n", "same\n", 2);
        assert!(preview.lines.is_empty());
        assert_eq!(preview.first_hunk_len, 0);
    }
}
