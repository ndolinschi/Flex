//! High-level HTML → model-facing markdown cleaning.

use super::nav::strip_navigation_blocks;
use super::strip::strip_html_boilerplate;

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
