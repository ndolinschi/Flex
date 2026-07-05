//! Markdown rendering facade over `tui-markdown`.
//!
//! Complete assistant blocks are parsed; in-flight drafts render incrementally
//! (code fences, tables, inline markdown) without waiting for materialization.
//! Parsed output is cached on `(rev, block_index)`.
//! Tables are rendered as bordered ASCII grids because `tui-markdown` skips them.

use std::collections::{HashMap, HashSet};

use pulldown_cmark::{Alignment, Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use tui_markdown::from_str;
use unicode_width::UnicodeWidthChar;

use crate::terminal_text::normalize_terminal_text;
use crate::theme;

/// Byte threshold above which streaming drafts skip markdown parsing.
const PLAIN_FALLBACK_BYTES: usize = 8 * 1024;
/// Line threshold above which streaming drafts skip markdown parsing.
const PLAIN_FALLBACK_LINES: usize = 200;
/// Minimum column width when shrinking wide tables.
const MIN_COL_WIDTH: usize = 3;

/// Cache of parsed markdown lines keyed by assistant `rev` and block index.
#[derive(Debug, Default)]
pub struct MarkdownCache {
    entries: HashMap<(u64, usize), Vec<Line<'static>>>,
}

impl MarkdownCache {
    /// Drop cache entries that no longer correspond to live assistant blocks.
    pub fn retain_live(&mut self, live_keys: impl IntoIterator<Item = (u64, usize)>) {
        let live: HashSet<_> = live_keys.into_iter().collect();
        self.entries.retain(|key, _| live.contains(key));
    }

    /// Clear all cached lines (session reset / agent switch).
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Render one markdown block into prefixed terminal lines.
pub(super) fn lines_for_block(
    cache: &mut MarkdownCache,
    key: (u64, usize),
    text: &str,
    complete: bool,
    prefix: &str,
    viewport_width: u16,
) -> Vec<Line<'static>> {
    if text.is_empty() {
        return Vec::new();
    }

    if let Some(cached) = cache.entries.get(&key) {
        return cached.clone();
    }

    let text = normalize_terminal_text(text);
    if text.is_empty() {
        return Vec::new();
    }

    let max_width = viewport_width.max(1);
    let oversized_draft = !complete
        && (text.len() > PLAIN_FALLBACK_BYTES || text.lines().count() > PLAIN_FALLBACK_LINES);

    let lines = if oversized_draft {
        plain_lines(prefix, &text, theme::assistant())
    } else {
        content_lines(prefix, &text, max_width)
    };

    if complete && !oversized_draft {
        cache.entries.insert(key, lines.clone());
    }
    lines
}

fn plain_lines(prefix: &str, text: &str, style: Style) -> Vec<Line<'static>> {
    if text.is_empty() {
        return Vec::new();
    }
    text.lines()
        .map(|line| {
            Line::from(vec![
                Span::raw(prefix.to_owned()),
                Span::styled(line.to_owned(), style),
            ])
        })
        .collect()
}

fn content_lines(prefix: &str, text: &str, max_width: u16) -> Vec<Line<'static>> {
    let parts = split_stream_parts(text);
    let mut lines = Vec::new();
    for part in parts {
        match part {
            StreamPart::Markdown(chunk) if chunk.trim().is_empty() => {}
            StreamPart::Markdown(chunk) => {
                lines.extend(markdown_with_tables(prefix, &chunk, max_width));
            }
            StreamPart::Code {
                lang,
                body,
                complete,
            } => {
                lines.extend(code_fence_lines(
                    prefix,
                    lang.as_deref(),
                    &body,
                    complete,
                    max_width,
                ));
            }
        }
    }
    if lines.is_empty() {
        plain_lines(prefix, text, theme::assistant())
    } else {
        lines
    }
}

fn markdown_with_tables(prefix: &str, text: &str, max_width: u16) -> Vec<Line<'static>> {
    let segments = split_segments(text);
    let has_table = segments
        .iter()
        .any(|segment| matches!(segment, Segment::Table { .. }));

    if !has_table {
        return tui_markdown_lines(prefix, text);
    }

    let mut lines = Vec::new();
    for segment in segments {
        match segment {
            Segment::Markdown(chunk) if !chunk.trim().is_empty() => {
                lines.extend(tui_markdown_lines(prefix, &chunk));
            }
            Segment::Table { alignments, rows } if !rows.is_empty() => {
                lines.extend(table_lines(prefix, &rows, &alignments, max_width));
            }
            _ => {}
        }
    }
    if lines.is_empty() {
        plain_lines(prefix, text, theme::assistant())
    } else {
        lines
    }
}

enum StreamPart {
    Markdown(String),
    Code {
        lang: Option<String>,
        body: String,
        complete: bool,
    },
}

fn split_stream_parts(text: &str) -> Vec<StreamPart> {
    let mut parts = Vec::new();
    let mut cursor = 0usize;

    while cursor < text.len() {
        let Some(rel) = text[cursor..].find("```") else {
            break;
        };
        let open = cursor + rel;
        if open > cursor {
            parts.push(StreamPart::Markdown(text[cursor..open].to_owned()));
        }
        let after_open = open + 3;
        let rest = &text[after_open..];
        let (lang, body_start) = parse_fence_header(rest);
        let body_offset = after_open + body_start;
        if body_offset > text.len() {
            parts.push(StreamPart::Code {
                lang,
                body: String::new(),
                complete: false,
            });
            return parts;
        }
        if let Some(close_rel) = text[body_offset..].find("```") {
            let body_end = body_offset + close_rel;
            let body = text[body_offset..body_end].to_owned();
            parts.push(StreamPart::Code {
                lang,
                body,
                complete: true,
            });
            cursor = body_end + 3;
        } else {
            let body = text[body_offset..].to_owned();
            parts.push(StreamPart::Code {
                lang,
                body,
                complete: false,
            });
            return parts;
        }
    }

    if cursor < text.len() {
        parts.push(StreamPart::Markdown(text[cursor..].to_owned()));
    } else if parts.is_empty() {
        parts.push(StreamPart::Markdown(text.to_owned()));
    }

    parts
}

fn parse_fence_header(rest: &str) -> (Option<String>, usize) {
    if rest.starts_with('\n') {
        return (None, 1);
    }
    if rest.starts_with("\r\n") {
        return (None, 2);
    }
    let lang_end = rest.find(['\n', '\r']).unwrap_or(rest.len());
    let lang_raw = rest[..lang_end].trim();
    if lang_raw.is_empty() {
        return (None, lang_end);
    }
    if lang_raw
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        let skip = lang_end
            + if rest[lang_end..].starts_with("\r\n") {
                2
            } else if rest.as_bytes().get(lang_end) == Some(&b'\n') {
                1
            } else {
                0
            };
        (Some(lang_raw.to_owned()), skip)
    } else {
        (None, 0)
    }
}

fn code_fence_lines(
    prefix: &str,
    lang: Option<&str>,
    body: &str,
    complete: bool,
    max_width: u16,
) -> Vec<Line<'static>> {
    let prefix_w = display_width(prefix) as u16;
    let inner_budget = max_width.saturating_sub(prefix_w).max(8) as usize;
    let mut lines = Vec::new();

    // opencode-style left rail: a heavy vertical (`┃`) down the block instead
    // of a full box. The header carries the language label; there is no bottom
    // corner (the rail simply stops).
    let mut header = String::from("┃");
    if let Some(lang) = lang.filter(|l| !l.is_empty()) {
        header.push(' ');
        header.push_str(lang);
    }
    if !complete {
        header.push('…');
    }
    lines.push(Line::from(vec![
        Span::raw(prefix.to_owned()),
        Span::styled(header, theme::code_border()),
    ]));

    if body.is_empty() && !complete {
        return lines;
    }

    // A syntect highlighter for the whole fence (stateful across its lines);
    // `None` on non-truecolor terminals or unknown/oversized bodies, in which
    // case we fall back to the flat `code` style.
    let mut highlighter = super::highlight::for_language(lang, body.len());
    for line in body.lines() {
        let visible = fit_to_width(line, inner_budget.saturating_sub(2));
        let mut spans = vec![
            Span::raw(prefix.to_owned()),
            Span::styled("┃ ".to_owned(), theme::code_border()),
        ];
        match highlighter.as_mut() {
            Some(hl) => spans.extend(hl.line(&visible)),
            None => spans.push(Span::styled(visible, theme::code())),
        }
        lines.push(Line::from(spans));
    }

    lines
}

fn tui_markdown_lines(prefix: &str, text: &str) -> Vec<Line<'static>> {
    let rendered = from_str(text);
    if rendered.lines.is_empty() {
        return plain_lines(prefix, text, theme::assistant());
    }
    rendered
        .lines
        .into_iter()
        .map(|line| {
            let mut spans = vec![Span::raw(prefix.to_owned())];
            for span in line.spans {
                spans.push(Span::styled(
                    span.content.into_owned(),
                    retheme_md(span.style),
                ));
            }
            Line::from(spans)
        })
        .collect()
}

/// Remap `tui-markdown`'s hardcoded ANSI colors onto the active theme so
/// markdown stays cohesive. Its list markers ship as electric `LightBlue`,
/// links as `Blue`, inline code as `White`; everything else (bold/italic
/// modifiers, uncolored text) is left untouched.
fn retheme_md(style: Style) -> Style {
    let mut out = style;
    out.fg = match style.fg {
        Some(Color::LightBlue) => theme::dim().fg,
        Some(Color::Blue) => theme::accent().fg,
        Some(Color::White) => theme::code().fg,
        other => other,
    };
    out
}

enum Segment {
    Markdown(String),
    Table {
        alignments: Vec<Alignment>,
        rows: Vec<Vec<String>>,
    },
}

fn split_segments(text: &str) -> Vec<Segment> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(text, options);

    let mut table_ranges: Vec<std::ops::Range<usize>> = Vec::new();
    let mut table_start: Option<usize> = None;

    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(Tag::Table(_)) => table_start = Some(range.start),
            Event::End(TagEnd::Table) => {
                if let Some(start) = table_start.take() {
                    table_ranges.push(start..range.end);
                }
            }
            _ => {}
        }
    }

    if table_ranges.is_empty() {
        return vec![Segment::Markdown(text.to_owned())];
    }

    let mut segments = Vec::new();
    let mut cursor = 0usize;
    for table_range in table_ranges {
        if cursor < table_range.start {
            let chunk = &text[cursor..table_range.start];
            if !chunk.trim().is_empty() {
                segments.push(Segment::Markdown(chunk.to_owned()));
            }
        }
        if let Some(table) = parse_table_segment(&text[table_range.start..table_range.end]) {
            segments.push(table);
        }
        cursor = table_range.end;
    }
    if cursor < text.len() {
        let chunk = &text[cursor..];
        if !chunk.trim().is_empty() {
            segments.push(Segment::Markdown(chunk.to_owned()));
        }
    }

    segments
}

fn parse_table_segment(table_text: &str) -> Option<Segment> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(table_text, options);

    let mut table_alignments = Vec::new();
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut current_cell = String::new();
    let mut in_table = false;

    for event in parser {
        match event {
            Event::Start(Tag::Table(alignments)) => {
                in_table = true;
                table_alignments = alignments;
                table_rows.clear();
            }
            Event::End(TagEnd::Table) => {
                if !current_cell.is_empty() || !current_row.is_empty() {
                    current_row.push(std::mem::take(&mut current_cell));
                    table_rows.push(std::mem::take(&mut current_row));
                }
                in_table = false;
            }
            Event::Start(Tag::TableHead) => {
                current_row.clear();
                current_cell.clear();
            }
            Event::End(TagEnd::TableHead) => {
                if !current_cell.is_empty() {
                    current_row.push(std::mem::take(&mut current_cell));
                }
                if !current_row.is_empty() {
                    table_rows.push(std::mem::take(&mut current_row));
                }
            }
            Event::Start(Tag::TableRow) => {
                current_row.clear();
                current_cell.clear();
            }
            Event::End(TagEnd::TableRow) if in_table => {
                if !current_cell.is_empty() {
                    current_row.push(std::mem::take(&mut current_cell));
                }
                if !current_row.is_empty() {
                    table_rows.push(std::mem::take(&mut current_row));
                }
            }
            Event::Start(Tag::TableCell) => current_cell.clear(),
            Event::End(TagEnd::TableCell) if in_table => {
                current_row.push(std::mem::take(&mut current_cell));
            }
            Event::Text(text) if in_table => current_cell.push_str(&text),
            Event::Code(code) if in_table => current_cell.push_str(&code),
            Event::SoftBreak if in_table => current_cell.push(' '),
            Event::HardBreak if in_table => current_cell.push('\n'),
            Event::Html(html) if in_table => current_cell.push_str(&html),
            _ => {}
        }
    }

    if table_rows.is_empty() {
        None
    } else {
        Some(Segment::Table {
            alignments: table_alignments,
            rows: table_rows,
        })
    }
}

fn table_lines(
    prefix: &str,
    rows: &[Vec<String>],
    alignments: &[Alignment],
    viewport_width: u16,
) -> Vec<Line<'static>> {
    let col_count = rows.iter().map(Vec::len).max().unwrap_or(0);
    if col_count == 0 {
        return Vec::new();
    }

    let prefix_w = display_width(prefix) as u16;
    let available = viewport_width.saturating_sub(prefix_w).max(10) as usize;
    let border_overhead = col_count * 3 + 1;
    let content_budget = available.saturating_sub(border_overhead);

    let mut widths = vec![0usize; col_count];
    for row in rows {
        for (idx, cell) in row.iter().enumerate() {
            let width = display_width(cell);
            if width > widths[idx] {
                widths[idx] = width;
            }
        }
    }
    for width in &mut widths {
        *width = (*width).max(1);
    }

    let total: usize = widths.iter().sum();
    if total > content_budget {
        widths = shrink_column_widths(widths, content_budget);
    }

    let mut lines = Vec::new();
    lines.push(border_line(prefix, &widths, '┌', '┬', '┐'));
    for (row_idx, row) in rows.iter().enumerate() {
        lines.push(data_line(prefix, row, &widths, alignments));
        if row_idx == 0 && rows.len() > 1 {
            lines.push(border_line(prefix, &widths, '├', '┼', '┤'));
        }
    }
    lines.push(border_line(prefix, &widths, '└', '┴', '┘'));
    lines
}

fn shrink_column_widths(mut widths: Vec<usize>, budget: usize) -> Vec<usize> {
    let mut total: usize = widths.iter().sum();
    if total <= budget {
        return widths;
    }

    let col_count = widths.len();
    let min_total = col_count * MIN_COL_WIDTH;
    if budget < min_total {
        let each = (budget / col_count).max(1);
        return vec![each; col_count];
    }

    for width in &mut widths {
        *width = (*width).max(MIN_COL_WIDTH);
    }
    total = widths.iter().sum();

    let mut shrinkable: Vec<usize> = (0..col_count)
        .filter(|&idx| widths[idx] > MIN_COL_WIDTH)
        .collect();
    shrinkable.sort_by(|a, b| widths[*b].cmp(&widths[*a]));

    while total > budget && !shrinkable.is_empty() {
        let idx = shrinkable[0];
        widths[idx] -= 1;
        total -= 1;
        if widths[idx] <= MIN_COL_WIDTH {
            shrinkable.remove(0);
        }
    }

    widths
}

fn border_line(
    prefix: &str,
    widths: &[usize],
    left: char,
    mid: char,
    right: char,
) -> Line<'static> {
    let mut content = String::new();
    content.push(left);
    for (idx, width) in widths.iter().enumerate() {
        if idx > 0 {
            content.push(mid);
        }
        for _ in 0..(*width + 2) {
            content.push('─');
        }
    }
    content.push(right);
    Line::from(vec![
        Span::raw(prefix.to_owned()),
        Span::styled(content, theme::border()),
    ])
}

fn data_line(
    prefix: &str,
    row: &[String],
    widths: &[usize],
    alignments: &[Alignment],
) -> Line<'static> {
    let mut content = String::new();
    content.push('│');
    for (idx, width) in widths.iter().enumerate() {
        let cell = row.get(idx).map(String::as_str).unwrap_or("");
        let alignment = alignments.get(idx).copied().unwrap_or(Alignment::None);
        content.push(' ');
        content.push_str(&pad_cell(cell, *width, alignment));
        content.push(' ');
        content.push('│');
    }
    Line::from(vec![
        Span::raw(prefix.to_owned()),
        Span::styled(content, theme::border()),
    ])
}

fn pad_cell(text: &str, width: usize, alignment: Alignment) -> String {
    let fitted = fit_to_width(text, width);
    let visible = display_width(&fitted);
    if visible >= width {
        return fitted;
    }
    let pad = width - visible;
    match alignment {
        Alignment::Right => {
            let mut out = String::new();
            out.push_str(&" ".repeat(pad));
            out.push_str(&fitted);
            out
        }
        Alignment::Center => {
            let left = pad / 2;
            let right = pad - left;
            format!("{}{}{}", " ".repeat(left), fitted, " ".repeat(right))
        }
        Alignment::Left | Alignment::None => {
            let mut out = fitted;
            out.push_str(&" ".repeat(pad));
            out
        }
    }
}

fn fit_to_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if display_width(text) <= width {
        return text.to_owned();
    }
    if width == 1 {
        return "…".to_owned();
    }
    truncate_with_ellipsis(text, width)
}

fn truncate_with_ellipsis(text: &str, width: usize) -> String {
    let target = width.saturating_sub(1);
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let w = char_width(ch);
        if used + w > target {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

fn display_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

fn char_width(ch: char) -> usize {
    UnicodeWidthChar::width(ch).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(prefix: &str, text: &str, complete: bool, width: u16) -> Vec<Line<'static>> {
        let mut cache = MarkdownCache::default();
        lines_for_block(&mut cache, (1, 0), text, complete, prefix, width)
    }

    fn joined(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn complete_block_uses_markdown_and_caches() {
        let mut cache = MarkdownCache::default();
        let key = (1, 0);
        let text = "# Title\n\n**bold**";
        let first = lines_for_block(&mut cache, key, text, true, "  ", 80);
        let second = lines_for_block(&mut cache, key, text, true, "  ", 80);
        assert_eq!(first, second);
        assert!(first.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains("Title") || span.content.contains("bold"))
        }));
    }

    #[test]
    fn oversized_draft_stays_plain() {
        let mut cache = MarkdownCache::default();
        let text = "# not parsed\n\n".repeat(300);
        let lines = lines_for_block(&mut cache, (1, 0), &text, false, "  ", 80);
        assert!(lines.iter().all(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains('#') || span.content.is_empty())
        }));
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn empty_markdown_block_renders_nothing() {
        let mut cache = MarkdownCache::default();
        let lines = lines_for_block(&mut cache, (1, 0), "", true, "  ", 80);
        assert!(lines.is_empty());
    }

    #[test]
    fn table_includes_header_row() {
        let text = "| Name | Count |\n| --- | ---: |\n| alpha | 1 |";
        let lines = render("  ", text, true, 80);
        let joined = joined(&lines);
        assert!(joined.contains("Name"), "header row missing: {joined}");
        assert!(joined.contains("alpha"));
    }

    #[test]
    fn table_renders_bordered_grid() {
        let text = "| Name | Value |\n| --- | ---: |\n| foo | 42 |";
        let lines = render("  ", text, true, 80);
        assert!(lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains('┌') && span.content.contains('┐'))
        }));
        assert!(lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains("foo") && span.content.contains('│'))
        }));
    }

    #[test]
    fn wide_table_truncates_long_cells() {
        let text = "| Area | Styling |\n| --- | --- |\n| UI | Mix of Tailwind, CSS vars, inline styles |\n| Fonts | Inter + system stack |";
        let lines = render("  ", text, true, 40);
        let joined = joined(&lines);
        assert!(joined.contains('┌'), "expected bordered table: {joined}");
        assert!(
            joined.contains('…'),
            "expected ellipsis truncation: {joined}"
        );
        for line in &lines {
            let row: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            if row.contains('│') {
                assert!(
                    display_width(&row) <= 40,
                    "row wider than viewport: {row:?} ({})",
                    display_width(&row)
                );
            }
        }
    }

    #[test]
    fn mixed_markdown_and_table() {
        let text = "Before\n\n| A | B |\n| - | - |\n| 1 | 2 |\n\nAfter";
        let lines = render("  ", text, true, 80);
        let joined = joined(&lines);
        assert!(joined.contains("Before"));
        assert!(joined.contains('┌'));
        assert!(joined.contains("After"));
    }

    #[test]
    fn post_table_paragraphs_on_separate_lines() {
        let text = "Intro line.\n\n| Tool | Version |\n| --- | --- |\n| React | 19 |\n\n## Stack\n\nParagraph one.\n\nParagraph two with **bold**.\n\n- item one\n- item two";
        let lines = render("  ", text, true, 80);
        let rendered: Vec<String> = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();
        let stack_idx = rendered.iter().position(|l| l.contains("Stack"));
        let para_one_idx = rendered.iter().position(|l| l.contains("Paragraph one"));
        let para_two_idx = rendered.iter().position(|l| l.contains("Paragraph two"));
        assert!(
            stack_idx.is_some() && para_one_idx.is_some() && para_two_idx.is_some(),
            "missing sections: {rendered:?}"
        );
        assert!(
            stack_idx.unwrap() < para_one_idx.unwrap()
                && para_one_idx.unwrap() < para_two_idx.unwrap(),
            "sections collapsed: {rendered:?}"
        );
        assert!(
            !rendered
                .iter()
                .any(|l| { l.contains("Stack") && l.contains("Paragraph one") && l.len() > 40 }),
            "heading and paragraph merged on one line: {rendered:?}"
        );
    }

    #[test]
    fn text_before_table_not_merged_with_table_row() {
        let text =
            "Typography uses class.Fonts\n\n| Area | Styling |\n| --- | --- |\n| UI | Tailwind |";
        let lines = render("  ", text, true, 80);
        let rendered = joined(&lines);
        assert!(rendered.contains("class.Fonts"));
        assert!(rendered.contains('┌'));
        let fonts_line = rendered
            .lines()
            .find(|l| l.contains("class.Fonts"))
            .expect("paragraph line");
        assert!(
            !fonts_line.contains('│'),
            "paragraph merged into table row: {fonts_line}"
        );
    }

    #[test]
    fn streaming_code_fence_renders_body_under_gutter() {
        let text = "Example:\n\n```rust\nfn main() {\n    println!(\"hi\");\n";
        let lines = render("  ", text, false, 80);
        let rendered = joined(&lines);
        assert!(
            rendered.contains("┃ rust"),
            "expected code fence header: {rendered}"
        );
        assert!(
            rendered.contains("fn main()"),
            "expected code body: {rendered}"
        );
        // The body may be split into several syntax-highlighted spans, so
        // assert on the concatenated line text plus the code-border gutter
        // rather than a single flat CODE span.
        assert!(
            lines.iter().any(|line| {
                let text: String = line
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect();
                text.contains("fn main()")
                    && line
                        .spans
                        .iter()
                        .any(|span| span.content == "┃ " && span.style == theme::code_border())
            }),
            "code body line should render under the fence gutter"
        );
    }

    #[test]
    fn list_markers_are_rethemed_off_ansi_blue() {
        theme::set_active(theme::BuiltinTheme::Tokyonight.resolve(true));
        let lines = render("  ", "1. first item\n2. second item\n", true, 80);
        // tui-markdown ships list markers as electric LightBlue; none should survive.
        assert!(
            lines
                .iter()
                .flat_map(|line| line.spans.iter())
                .all(|span| span.style.fg != Some(Color::LightBlue)),
            "no span should keep tui-markdown's LightBlue"
        );
        // The marker now uses the theme's dim foreground.
        assert!(
            lines.iter().flat_map(|line| line.spans.iter()).any(|span| {
                span.content.trim_start().starts_with("1.") && span.style.fg == theme::dim().fg
            }),
            "ordered-list marker should use the dim theme color"
        );
    }

    #[test]
    fn code_fence_syntax_highlights_in_truecolor() {
        theme::set_active(theme::BuiltinTheme::Tokyonight.resolve(true));
        let lines = render("  ", "```rust\nfn main() {}\n```", true, 80);
        let colors: std::collections::HashSet<(u8, u8, u8)> = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .filter_map(|span| match span.style.fg {
                Some(ratatui::style::Color::Rgb(r, g, b)) => Some((r, g, b)),
                _ => None,
            })
            .collect();
        // syntect assigns several distinct token colors across `fn main() {}`.
        assert!(
            colors.len() >= 2,
            "expected multiple syntax-highlight colors, got {colors:?}"
        );
    }

    #[test]
    fn complete_code_fence_renders_with_border() {
        let text = "```\nline one\nline two\n```";
        let lines = render("  ", text, true, 80);
        let rendered = joined(&lines);
        // The fence now renders as an opencode-style left rail (`┃`), not a box.
        assert!(rendered.contains('┃'), "{rendered}");
        assert!(rendered.contains("line one"));
        assert!(rendered.contains("line two"));
    }

    #[test]
    fn streaming_markdown_renders_inline_formatting() {
        let text = "This is **bold** text";
        let lines = render("  ", text, false, 80);
        assert!(lines.iter().any(|line| {
            line.spans.iter().any(|span| {
                span.content.contains("bold")
                    && span
                        .style
                        .add_modifier
                        .contains(ratatui::style::Modifier::BOLD)
            })
        }));
    }
}
