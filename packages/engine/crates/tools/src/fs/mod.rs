//! Filesystem tools and shared per-session state.
//!
//! `FsState` remembers which absolute paths were `Read` this session and the
//! file's modification time at that moment. `Write` and `Edit` consult it to
//! enforce the read-before-modify discipline and to detect files that changed
//! on disk between the model's `Read` and its mutation.

mod edit;
mod read;
mod write;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

use agentloop_core::ToolError;
use regex::Regex;

pub use edit::EditTool;
pub use read::ReadTool;
pub use write::WriteTool;

/// Tracks the paths read this session and their mtime at read.
///
/// One instance is shared (via `Arc`) by the `Read`, `Write`, and `Edit`
/// tools of a session.
#[derive(Default)]
pub struct FsState {
    reads: Mutex<HashMap<PathBuf, SystemTime>>,
}

impl FsState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `path` was read while its mtime was `mtime`. Also called
    /// after `Write`/`Edit` so a freshly mutated file stays editable.
    pub fn record_read(&self, path: PathBuf, mtime: SystemTime) {
        let mut map = self.reads.lock().unwrap_or_else(|p| p.into_inner());
        map.insert(path, mtime);
    }

    /// The mtime `path` had when it was last read, or `None` if it was never
    /// read this session.
    pub fn recorded_mtime(&self, path: &Path) -> Option<SystemTime> {
        let map = self.reads.lock().unwrap_or_else(|p| p.into_inner());
        map.get(path).copied()
    }
}

/// Derive an input schema for a tool.
pub(crate) fn schema_of<I: schemars::JsonSchema>() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(I))
        .unwrap_or_else(|_| serde_json::json!({"type": "object"}))
}

/// Parse a `file_path` argument, teaching the model to pass absolute paths.
pub(crate) fn require_absolute(raw: &str, cwd: &Path) -> Result<PathBuf, ToolError> {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(ToolError::InvalidInput(format!(
            "`file_path` must be an absolute path, but got `{raw}`. The session working \
             directory is `{}`; for a file there, pass `{}`.",
            cwd.display(),
            cwd.join(raw).display()
        )))
    }
}

/// Enforce the read-before-modify discipline for an existing file.
///
/// `current_mtime` is the file's mtime right now; `tool_name` names the
/// caller (`Write` / `Edit`) so errors read naturally.
pub(crate) fn check_freshness(
    state: &FsState,
    path: &Path,
    current_mtime: SystemTime,
    tool_name: &str,
) -> Result<(), ToolError> {
    match state.recorded_mtime(path) {
        None => Err(ToolError::Execution(format!(
            "`{}` already exists but has not been Read in this session. Read it first to see \
             its current content, then retry the {tool_name}.",
            path.display()
        ))),
        Some(recorded) if recorded != current_mtime => Err(ToolError::Execution(format!(
            "`{}` has changed on disk since you last Read it. Read it again to get the current \
             content, then retry the {tool_name}.",
            path.display()
        ))),
        Some(_) => Ok(()),
    }
}

pub(crate) fn modified_time(metadata: &std::fs::Metadata) -> SystemTime {
    metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Resolve an optional search-root argument against the session cwd and
/// verify it is an existing directory.
pub(crate) async fn resolve_search_root(
    path: Option<&str>,
    cwd: &Path,
    tool_name: &str,
) -> Result<PathBuf, ToolError> {
    let base = match path {
        Some(p) => {
            let pb = PathBuf::from(p);
            if pb.is_absolute() { pb } else { cwd.join(pb) }
        }
        None => cwd.to_path_buf(),
    };
    let meta = tokio::fs::metadata(&base).await.map_err(|err| {
        ToolError::InvalidInput(format!(
            "{tool_name} search path `{}` does not exist or is not accessible: {err}. Pass an \
             existing directory (absolute, or relative to the session cwd `{}`), or omit `path` \
             to search the cwd.",
            base.display(),
            cwd.display()
        ))
    })?;
    if !meta.is_dir() {
        return Err(ToolError::InvalidInput(format!(
            "{tool_name} search path `{}` is a file, not a directory. Pass a directory to \
             search under, or omit `path` to search the session cwd.",
            base.display()
        )));
    }
    Ok(base)
}

pub(crate) fn truncate_chars(text: &str, max_chars: usize) -> (String, bool) {
    if text.chars().count() <= max_chars {
        return (text.to_owned(), false);
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("\n\n[... output truncated ...]");
    (out, true)
}

// ---------------------------------------------------------------------------
// HTML cleaning utilities
// ---------------------------------------------------------------------------

/// Strip boilerplate HTML tags that are never useful content for a model:
/// `<script>`, `<style>`, `<noscript>`, `<link>`, and `<meta>`.
///
/// Uses simple regex removal — no DOM parser.
#[allow(clippy::expect_used)]
/// Strip inline boilerplate tags and semantic chrome elements whose content is
/// never useful to a model.
///
/// Removed:
/// - `<script>`, `<style>`, `<noscript>`, `<link>`, `<meta>` (tags + content)
/// - `<nav>`, `<header>`, `<footer>` (entire element with all descendants)
fn strip_html_boilerplate(html: &str) -> String {
    static SCRIPT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<script[^>]*>.*?</script>").expect("static regex"));
    static STYLE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<style[^>]*>.*?</style>").expect("static regex"));
    static NOSCRIPT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<noscript[^>]*>.*?</noscript>").expect("static regex"));
    static LINK_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)<link[^>]*/?>").expect("static regex"));
    static META_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)<meta[^>]*/?>").expect("static regex"));

    // Semantic chrome: remove the entire element including all nested content.
    // These tags are never nested within themselves, so a non-greedy match
    // correctly captures the outermost pair.
    static NAV_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<nav[^>]*>.*?</nav>").expect("static regex"));
    static HEADER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<header[^>]*>.*?</header>").expect("static regex"));
    static FOOTER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<footer[^>]*>.*?</footer>").expect("static regex"));

    let mut cleaned = SCRIPT_RE.replace_all(html, "").into_owned();
    cleaned = STYLE_RE.replace_all(&cleaned, "").into_owned();
    cleaned = NOSCRIPT_RE.replace_all(&cleaned, "").into_owned();
    cleaned = LINK_RE.replace_all(&cleaned, "").into_owned();
    cleaned = META_RE.replace_all(&cleaned, "").into_owned();

    // Strip semantic chrome elements (with content) before markdown conversion.
    // Order matters: strip nav first so we don't miss a closing </nav> that's
    // inside a header.
    cleaned = NAV_RE.replace_all(&cleaned, "").into_owned();
    cleaned = HEADER_RE.replace_all(&cleaned, "").into_owned();
    cleaned = FOOTER_RE.replace_all(&cleaned, "").into_owned();

    cleaned
}

/// Strip `<nav>`, `<header>`, and `<footer>` elements from HTML while
/// preserving the rest. Used before link extraction to exclude chrome links.
fn strip_semantic_chrome(html: &str) -> String {
    static NAV_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<nav[^>]*>.*?</nav>").expect("static regex"));
    static HEADER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<header[^>]*>.*?</header>").expect("static regex"));
    static FOOTER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<footer[^>]*>.*?</footer>").expect("static regex"));

    let mut cleaned = NAV_RE.replace_all(html, "").into_owned();
    cleaned = HEADER_RE.replace_all(&cleaned, "").into_owned();
    cleaned = FOOTER_RE.replace_all(&cleaned, "").into_owned();
    cleaned
}

/// Regex for detecting markdown links `[text](url)` in nav-stripping.
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
fn strip_navigation_blocks(markdown: &str) -> String {
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
            while i < lines.len()
                && (is_list_item_line(lines[i]) || lines[i].trim().is_empty())
            {
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
fn is_nav_line_by_links(line: &str) -> bool {
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
fn count_markdown_links(text: &str) -> (usize, usize) {
    let mut chars = 0usize;
    let mut count = 0usize;
    for m in MD_LINK_RE.find_iter(text) {
        chars += m.len();
        count += 1;
    }
    (chars, count)
}

/// Returns true if `line` starts like a markdown list item (`*`, `-`, or `1.`).
fn is_list_item_line(line: &str) -> bool {
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

/// Extract the main content from a markdown page by finding the densest
/// paragraph region.
///
/// Strategy:
/// 1. Split by double-newline into paragraphs.
/// 2. Find the longest paragraph as the anchor point.
/// 3. Expand left and right from the anchor, including neighboring paragraphs
///    that exceed the minimum length threshold (30 chars).
/// 4. If the resulting block is at least 30% of total content, return it;
///    otherwise return the full text.
///
/// This drops short boilerplate like nav bars and footers while keeping the
/// article body.
pub(crate) fn extract_content_core(markdown: &str) -> String {
    const MIN_PARAGRAPH_LEN: usize = 30;
    const CORE_MIN_FRACTION: f64 = 0.30;

    let paragraphs: Vec<&str> = markdown
        .split("\n\n")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();

    if paragraphs.len() < 3 {
        return markdown.to_owned();
    }

    let total_chars: usize = paragraphs.iter().map(|p| p.len()).sum();
    if total_chars == 0 {
        return markdown.to_owned();
    }

    let anchor = match paragraphs.iter().enumerate().max_by_key(|(_, p)| p.len()) {
        Some((idx, _)) => idx,
        None => return markdown.to_owned(),
    };

    let mut core_start = anchor;
    while core_start > 0 && paragraphs[core_start - 1].len() >= MIN_PARAGRAPH_LEN {
        core_start -= 1;
    }

    let mut core_end = anchor + 1;
    while core_end < paragraphs.len() && paragraphs[core_end].len() >= MIN_PARAGRAPH_LEN {
        core_end += 1;
    }

    let core: Vec<&str> = paragraphs[core_start..core_end].to_vec();
    let core_chars: usize = core.iter().map(|p| p.len()).sum();
    let fraction = core_chars as f64 / total_chars as f64;

    if core.len() >= 2 && fraction >= CORE_MIN_FRACTION {
        let mut result = core.join("\n\n");
        if core_start > 0 || core_end < paragraphs.len() {
            result = format!(
                "[... {:.0}% of page focused as main content ...]\n\n{result}",
                fraction * 100.0
            );
        }
        result
    } else {
        markdown.to_owned()
    }
}

/// Process raw HTML into clean, model-friendly markdown.
///
/// Pipeline: strip script/style/metadata tags → htmd convert → nav stripping
/// → content-core extraction. Falls back to the cleaned HTML if htmd
/// conversion fails.
pub(crate) fn clean_html_for_model(html: &str) -> String {
    let cleaned = strip_html_boilerplate(html);
    match htmd::convert(&cleaned) {
        Ok(markdown) => extract_content_core(&strip_navigation_blocks(&markdown)),
        Err(_) => {
            tracing::warn!("htmd::convert failed, returning cleaned HTML as fallback");
            cleaned
        }
    }
}

// ---------------------------------------------------------------------------
// Link extraction
// ---------------------------------------------------------------------------

/// Regex for extracting `href` and inner text from `<a>` tags.
#[allow(clippy::expect_used)]
static A_HREF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<a\s[^>]*?href\s*=\s*"([^"]*)"[^>]*>\s*(.*?)\s*</a>"#)
        .expect("static regex")
});

/// Strip HTML tags from a string, keeping the inner text.
fn strip_html_tags(input: &str) -> String {
    #[allow(clippy::expect_used)]
    static TAG_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"<[^>]*>").expect("static regex"));
    TAG_RE.replace_all(input, "").into_owned()
}

/// Extract all unique absolute HTTP(S) links from HTML `<a href="...">` tags.
///
/// Resolves relative URLs against `base_url`. Filters out:
/// - Fragment-only links (`#section`)
/// - `javascript:` pseudo-links
/// - `mailto:` links
/// - Links to the same page (path + query match after resolution)
/// - Non-http/https URLs
///
/// Returns `(url, link_text)` tuples. Link text has HTML tags stripped.
pub fn extract_page_links(html: &str, base_url: &str) -> Vec<(String, String)> {
    let base = match reqwest::Url::parse(base_url) {
        Ok(url) => url,
        Err(_) => return vec![],
    };

    let base_no_frag = {
        let mut u = base.clone();
        u.set_fragment(None);
        u.to_string()
    };

    let mut seen = std::collections::HashSet::new();
    let mut links: Vec<(String, String)> = Vec::new();

    // Strip nav/header/footer chrome so we only return content-area links.
    let body = strip_semantic_chrome(html);

    for cap in A_HREF_RE.captures_iter(&body) {
        let href = &cap[1];
        let text = strip_html_tags(&cap[2]);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }

        // Filter out javascript:, mailto:, fragment-only
        if href.starts_with("javascript:")
            || href.starts_with("mailto:")
            || href.starts_with('#')
        {
            continue;
        }

        // Resolve relative URL
        let resolved = match base.join(href) {
            Ok(url) => url,
            Err(_) => continue,
        };

        // Only http/https
        if !matches!(resolved.scheme(), "http" | "https") {
            continue;
        }

        // Compare without fragment to skip same-page links
        let mut url_no_frag = resolved.clone();
        url_no_frag.set_fragment(None);
        let url_key = url_no_frag.to_string();

        if url_key == base_no_frag {
            continue;
        }

        if seen.insert(url_key) {
            links.push((resolved.to_string(), text.to_owned()));
        }
    }

    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_recalls_mtime() {
        let state = FsState::new();
        let path = PathBuf::from("/tmp/a.txt");
        assert_eq!(state.recorded_mtime(&path), None);
        let t = SystemTime::UNIX_EPOCH;
        state.record_read(path.clone(), t);
        assert_eq!(state.recorded_mtime(&path), Some(t));
    }

    #[test]
    fn require_absolute_teaches_relative_paths() {
        let err = require_absolute("src/main.rs", Path::new("/work"));
        assert!(matches!(err, Err(ToolError::InvalidInput(_))));
        let message = err.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(message.contains("absolute"), "{message}");
        assert!(message.contains("/work/src/main.rs"), "{message}");
    }

    // --- strip_navigation_blocks tests ---

    #[test]
    fn list_item_detection() {
        assert!(is_list_item_line("* item one"));
        assert!(is_list_item_line("- item two"));
        assert!(is_list_item_line("1. numbered item"));
        assert!(is_list_item_line("  * indented item"));
        assert!(!is_list_item_line(""));
        assert!(!is_list_item_line("This is just regular text."));
        // Bold text should not be flagged as a list item even though it starts
        // with '*'; the 3-consecutive threshold protects against false
        // positives.
        assert!(is_list_item_line("*bold* text"));
    }

    #[test]
    fn markdown_link_counting() {
        let (chars, count) = count_markdown_links("[Home](/) [About](/about)");
        assert_eq!(count, 2);
        assert!(chars > 10);
        let (_, count) = count_markdown_links("Just plain text, no links here.");
        assert_eq!(count, 0);
    }

    #[test]
    fn nav_line_detection_short_link_heavy() {
        // Short line composed almost entirely of links → nav.
        assert!(is_nav_line_by_links(
            "[Home](/) [Docs](/docs) [API](/api) [Blog](/blog)"
        ));
        // Single link with substantial surrounding text → not nav.
        assert!(!is_nav_line_by_links(
            "Read the [installation guide](/install) before proceeding."
        ));
    }

    #[test]
    fn nav_line_detection_many_links() {
        // Line with 3+ links regardless of length → nav.
        assert!(is_nav_line_by_links(
            "Check [A](/a) [B](/b) and [C](/c) for a very long description of something interesting here"
        ));
    }

    #[test]
    fn nav_line_detection_body_text_with_link() {
        // A single mid-sentence link in genuine body text → not nav.
        assert!(!is_nav_line_by_links(
            "For more details please refer to the [official documentation](https://example.com)."
        ));
    }

    #[test]
    fn strip_nav_removes_link_dense_lines() {
        let md = "\
[Home](/) [Products](/p) [About](/a) [Contact](/c)

## Introduction

This is the main content of the page. It provides a thorough overview of the
topic and contains enough characters to pass the 200-char safety threshold so
the cleaned version is returned instead of the original.";
        let cleaned = strip_navigation_blocks(md);
        assert!(!cleaned.contains("[Home]"), "nav line should be stripped: {cleaned}");
        assert!(cleaned.contains("## Introduction"), "heading should survive");
        assert!(cleaned.contains("thorough overview"), "body text should survive");
    }

    #[test]
    fn strip_nav_removes_consecutive_list_items() {
        let md = "\
* Navigation Item 1 with a long enough description here okay
* Navigation Item 2 with a long enough description here okay
* Navigation Item 3 with a long enough description here okay

## Article

The actual article content starts here and provides meaningful information
that should be preserved by the content extraction pipeline during scraping.";
        let cleaned = strip_navigation_blocks(md);
        assert!(
            !cleaned.contains("Navigation Item 1"),
            "consecutive list should be stripped: {cleaned}"
        );
        assert!(cleaned.contains("## Article"), "heading should survive");
        assert!(cleaned.contains("actual article"), "body text should survive");
    }

    #[test]
    fn strip_nav_collapses_duplicate_lines() {
        let md = "\
Copyright 2024 Example Inc.

Copyright 2024 Example Inc.

Unique content that appears only once in the document and must be retained
for proper content extraction to work correctly during web scraping.";
        let cleaned = strip_navigation_blocks(md);
        let copyright_count = cleaned.matches("Copyright 2024 Example Inc.").count();
        assert_eq!(
            copyright_count, 1,
            "duplicate line should be collapsed to one: {cleaned}"
        );
        assert!(cleaned.contains("Unique content"), "unique content should survive");
    }

    #[test]
    fn strip_nav_preserves_short_content() {
        // Content under 200 chars after stripping should return original.
        let md = "[Home](/) [About](/a)";
        let cleaned = strip_navigation_blocks(md);
        assert_eq!(
            cleaned, md,
            "short content after stripping should return original"
        );
    }

    #[test]
    fn strip_nav_preserves_normal_text() {
        let md = "\
# Welcome

This is a normal page with some [links](https://example.com) but not enough
to be considered navigation. The content is substantial and well-structured
so it should pass through the cleaning pipeline without any modification.";
        let cleaned = strip_navigation_blocks(md);
        assert_eq!(
            cleaned, md,
            "normal text with few links should not be modified"
        );
    }

    #[test]
    fn clean_html_for_model_integration() {
        // Simulate an ITER-like page: nav-heavy header + real article body.
        let html = r#"<html><body>
        <nav>
          <a href="/">Home</a>
          <a href="/news">News</a>
          <a href="/science">Science</a>
          <a href="/tech">Tech</a>
          <a href="/contact">Contact</a>
        </nav>
        <article>
          <h1>Fusion Energy Breakthrough</h1>
          <p>Scientists at the ITER project have achieved a significant milestone
          in plasma confinement, sustaining a reaction for over five minutes at
          temperatures exceeding 150 million degrees Celsius.</p>
          <p>The breakthrough represents years of collaborative research across
          multiple international partners and brings commercial fusion energy
          one step closer to reality.</p>
        </article>
        </body></html>"#;
        let result = clean_html_for_model(html);
        // The nav links should not dominate the output; body text must survive.
        assert!(
            result.contains("plasma") || result.contains("ITER"),
            "article body should appear in cleaned output: {result}"
        );
        assert!(
            result.contains("collaborative") || result.contains("commercial"),
            "second paragraph should survive cleaning: {result}"
        );
    }
}
