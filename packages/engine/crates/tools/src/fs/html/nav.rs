//! Markdown navigation-boilerplate stripping.

use std::sync::LazyLock;

use regex::Regex;

#[allow(clippy::expect_used)]
static MD_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)").expect("static regex"));

/// Strip navigation-like boilerplate from markdown before content extraction.
///
/// Detects and removes lines that are likely navigation menus or link
/// directories rather than article content:
/// - Lines with a high ratio of markdown links (`[text](url)`) vs plain text
/// - Runs of 3+ consecutive bullet or numbered list items
/// - Short lines (&lt; 80 chars) composed mostly of markdown links
/// - Duplicate non-empty lines (e.g. nav repeated in header and footer)
///
/// If the cleaned result is under 100 characters, the original markdown is
/// returned unchanged to avoid over-stripping short pages.
pub(super) fn strip_navigation_blocks(markdown: &str) -> String {
    let lines: Vec<&str> = markdown.lines().collect();
    if lines.is_empty() {
        return markdown.to_owned();
    }

    // Phase 1: detect nav-suspect lines by link density.
    let mut is_nav = vec![false; lines.len()];
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_nav_line_by_links(trimmed) {
            is_nav[i] = true;
        }
    }

    // Phase 2: detect runs of 3+ consecutive list items.
    let mut i = 0;
    while i < lines.len() {
        if is_list_item_line(lines[i]) {
            let start = i;
            while i < lines.len() && (is_list_item_line(lines[i]) || lines[i].trim().is_empty()) {
                i += 1;
            }
            let consecutive = (start..i).filter(|j| is_list_item_line(lines[*j])).count();
            if consecutive >= 3 {
                for j in start..i {
                    if !lines[j].trim().is_empty() {
                        is_nav[j] = true;
                    }
                }
            }
        } else {
            i += 1;
        }
    }

    // Phase 3: filter nav lines and deduplicate non-empty lines.
    let mut cleaned: Vec<&str> = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        if is_nav[i] {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            cleaned.push(line);
            continue;
        }
        // Skip duplicate non-empty lines (nav blocks repeated in header +
        // footer produce identical link lists).
        if cleaned.iter().any(|prev| prev.trim() == trimmed) {
            continue;
        }
        cleaned.push(line);
    }

    let result = cleaned.join("\n").trim().to_owned();

    // Don't over-strip: if the result is too short, return the original.
    if result.len() < 100 {
        return markdown.to_owned();
    }

    result
}

/// Returns true if `line` looks like a navigation link rather than body text.
///
/// A line is considered navigation if it has 2+ markdown links with very
/// little plain text between them, 3+ links regardless of context, or a
/// single link on a short line (&lt; 80 chars) with almost no surrounding
/// text.
pub(super) fn is_nav_line_by_links(line: &str) -> bool {
    let (_, link_count) = count_markdown_links(line);
    if link_count == 0 {
        return false;
    }

    // Strip all markdown links to see what plain text remains.
    let plain = MD_LINK_RE.replace_all(line, "").trim().to_string();

    // 2+ links with very little plain text between them → nav.
    if link_count >= 2 && plain.len() < 15 {
        return true;
    }

    // 3+ links regardless of context → nav.
    if link_count >= 3 {
        return true;
    }

    // Single link on a short line with almost no surrounding text → nav.
    let total = line.len();
    if total < 80 && link_count == 1 && plain.len() < 10 {
        return true;
    }

    false
}

/// Count characters consumed by markdown `[text](url)` patterns and the number
/// of patterns found.
pub(super) fn count_markdown_links(text: &str) -> (usize, usize) {
    let mut chars = 0usize;
    let mut count = 0usize;
    for m in MD_LINK_RE.find_iter(text) {
        chars += m.len();
        count += 1;
    }
    (chars, count)
}

/// Returns true if `line` starts like a markdown list item (`*`, `-`, or `1.`).
pub(super) fn is_list_item_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with('*') || trimmed.starts_with('-') {
        return true;
    }
    if let Some(first) = trimmed.chars().next() {
        return first.is_ascii_digit() && trimmed.contains(". ");
    }
    false
}
