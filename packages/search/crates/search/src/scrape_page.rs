//! `scrape_page` tool: fetch a URL and return its text content.
//!
//! Follows the same pattern as the engine's `WebFetch` tool: reqwest for
//! HTTP, `htmd` for HTML-to-markdown conversion, and truncation with
//! explicit markers so the model knows content was cut.
//!
//! After markdown conversion, a heuristic extracts the "content core" — the
//! largest contiguous block of paragraphs — to keep the output token-efficient
//! by dropping boilerplate (nav, footer, sidebars) from the response.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::{ToolOutput, ToolResultBlock};
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};
use agentloop_tools::fs::extract_page_links;

/// Default max output bytes/chars.
const DEFAULT_MAX_BYTES: usize = 200_000;

/// Hard cap on max_bytes input parameter.
const HARD_MAX_BYTES: usize = 1_000_000;

/// Raw HTML fetch limit — applies before conversion, large enough that we don't
/// cut HTML mid-tag.
const RAW_FETCH_LIMIT: usize = 4 * 1024 * 1024;

/// Maximum characters in the output fed back to the model.
const MAX_OUTPUT_CHARS: usize = 120_000;

/// Minimum fraction of total content the content core must represent to be used.
/// If the best contiguous block is less than this fraction of the total, the full
/// content is returned instead.
const CORE_MIN_FRACTION: f64 = 0.30;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ScrapePageInput {
    /// HTTP or HTTPS URL of the page to scrape.
    url: String,
    /// Maximum response bytes to keep. Defaults to 200000, capped at 1000000.
    max_bytes: Option<usize>,
    /// Whether to include links found on the page in the output. Defaults to true.
    include_links: Option<bool>,
}

/// Fetches a web page and returns its text content as markdown.
///
/// HTML pages are converted to markdown via `htmd`; non-HTML content is
/// returned as plain text. The output is truncated with an explicit marker
/// so the model can decide whether to refine the query.
#[derive(Clone)]
pub struct ScrapePageTool {
    client: reqwest::Client,
}

impl ScrapePageTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for ScrapePageTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ScrapePageTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "scrape_page".to_owned(),
            description: "Fetch and read the text content of a web page. Returns the page content \
                 converted to readable markdown (HTML pages) or plain text (other content \
                 types). Use this after `search_web` to read the full content of a result. \
                 Set `max_bytes` to limit large pages. Non-success HTTP statuses are \
                 returned as tool errors so you can decide whether to retry with a \
                 different URL. When `include_links` is true (the default), a numbered list \
                 of links found on the page is appended."
                .to_owned(),
            input_schema: schema_of::<ScrapePageInput>(),
            read_only: true,
            category: ToolCategory::Web,
            needs_permission: PermissionHint::Always,
        }
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let parsed: ScrapePageInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `scrape_page` must be {{\"url\": \"https://...\", \"max_bytes\": \
                 <optional number>}}: {err}."
            ))
        })?;

        let url = reqwest::Url::parse(&parsed.url).map_err(|err| {
            ToolError::InvalidInput(format!(
                "`url` is not a valid absolute URL for `scrape_page`: {err}. Pass an \
                 http:// or https:// URL."
            ))
        })?;

        if !matches!(url.scheme(), "http" | "https") {
            return Err(ToolError::InvalidInput(format!(
                "`scrape_page` supports only http:// and https:// URLs, but got `{}`.",
                url.scheme()
            )));
        }

        let max_bytes = parsed
            .max_bytes
            .unwrap_or(DEFAULT_MAX_BYTES)
            .min(HARD_MAX_BYTES);
        let include_links = parsed.include_links.unwrap_or(true);

        let response = tokio::select! {
            _ = ctx.cancel.cancelled() => return Err(ToolError::Cancelled),
            result = self.client.get(url.clone()).send() => result.map_err(|err| {
                ToolError::Execution(format!(
                    "`scrape_page` request to `{url}` failed: {err}."
                ))
            })?,
        };

        let status = response.status();
        let final_url = response.url().clone();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        if !status.is_success() {
            return Err(ToolError::Execution(format!(
                "`scrape_page` returned HTTP {} for `{}`. Check the URL or fetch a more \
                 specific page.",
                status.as_u16(),
                final_url
            )));
        }

        if response
            .content_length()
            .is_some_and(|len| len as usize > RAW_FETCH_LIMIT)
        {
            return Err(ToolError::Execution(format!(
                "`scrape_page` URL `{}` is too large (content-length exceeds {} bytes). \
                 Use a more specific URL.",
                final_url, RAW_FETCH_LIMIT
            )));
        }

        let bytes = tokio::select! {
            _ = ctx.cancel.cancelled() => return Err(ToolError::Cancelled),
            result = response.bytes() => result.map_err(|err| {
                ToolError::Execution(format!(
                    "`scrape_page` could not read `{final_url}`: {err}."
                ))
            })?,
        };

        let kept_len = bytes.len().min(RAW_FETCH_LIMIT);
        let raw_truncated = kept_len < bytes.len();
        let is_html = content_type
            .as_deref()
            .is_some_and(|ct| ct.contains("text/html") || ct.contains("application/xhtml"));

        let body = if is_html {
            let html_str = String::from_utf8_lossy(&bytes[..kept_len]);
            let pre_cleaned = strip_html_boilerplate(&html_str);
            let markdown = htmd::convert(&pre_cleaned).unwrap_or(pre_cleaned);
            let core = extract_content_core(&markdown);
            let (mut truncated, _) = truncate_chars(&core, max_bytes);

            if include_links {
                let links = extract_page_links(&html_str, final_url.as_str());
                if !links.is_empty() {
                    truncated.push_str("\n\n--- Links found on this page ---\n");
                    for (i, (link_url, link_text)) in links.iter().enumerate() {
                        truncated.push_str(&format!(
                            "{}. [{}]({})\n",
                            i + 1,
                            link_text,
                            link_url
                        ));
                    }
                    truncated.push_str(
                        "\nUse `scrape_page` to explore any of these links.",
                    );
                }
            }

            truncated
        } else {
            let raw = String::from_utf8_lossy(&bytes[..kept_len.min(max_bytes)]);
            let (truncated, _) = truncate_chars(raw.as_ref(), max_bytes);
            truncated
        };

        let mut rendered = String::new();
        rendered.push_str("url: ");
        rendered.push_str(final_url.as_str());
        rendered.push_str("\nstatus: ");
        rendered.push_str(&status.as_u16().to_string());
        if let Some(ref ct) = content_type {
            rendered.push_str("\ncontent_type: ");
            rendered.push_str(ct);
        }
        rendered.push_str("\n\n");
        rendered.push_str(&body);

        let (rendered, output_truncated) = truncate_chars(&rendered, MAX_OUTPUT_CHARS);

        Ok(ToolOutput {
            content: vec![ToolResultBlock::markdown(rendered)],
            is_error: false,
            structured: Some(serde_json::json!({
                "url": final_url.as_str(),
                "status": status.as_u16(),
                "content_type": content_type,
                "bytes": bytes.len(),
                "kept_bytes": kept_len,
                "truncated": output_truncated || raw_truncated,
            })),
        })
    }
}

/// Strip boilerplate HTML tags and semantic chrome that are never useful
/// content for a model.
///
/// Removes:
/// - `<script>`, `<style>`, `<noscript>` (container tags with content)
/// - `<link>`, `<meta>` (self-closing tags)
/// - `<nav>`, `<header>`, `<footer>` (semantic chrome — entire element with
///   all descendants removed)
fn strip_html_boilerplate(html: &str) -> String {
    let mut result = strip_container_tag(html, "script");
    result = strip_container_tag(&result, "style");
    result = strip_container_tag(&result, "noscript");
    result = strip_container_tag(&result, "nav");
    result = strip_container_tag(&result, "header");
    result = strip_container_tag(&result, "footer");
    result = strip_self_closing_tag(&result, "link");
    result = strip_self_closing_tag(&result, "meta");
    result
}

/// Remove `<tag ...>content</tag>` pairs (case-insensitive).
fn strip_container_tag(html: &str, tag_name: &str) -> String {
    let html_lower = html.to_lowercase();
    let open_marker = format!("<{}", tag_name);
    let close_marker = format!("</{}>", tag_name);
    let mut out = String::with_capacity(html.len());
    let mut pos = 0;
    let len = html.len();
    while pos < len {
        let remaining = &html_lower[pos..];
        match remaining.find(&open_marker) {
            None => {
                out.push_str(&html[pos..]);
                break;
            }
            Some(rel) => {
                out.push_str(&html[pos..pos + rel]);
                let tag_start = pos + rel;
                // Find the `>` that closes the opening tag.
                match html[tag_start..].find('>') {
                    None => {
                        out.push_str(&html[tag_start..]);
                        break;
                    }
                    Some(gt) => {
                        let after_open = tag_start + gt + 1;
                        // Search for the closing tag.
                        match html_lower[after_open..].find(&close_marker) {
                            None => {
                                out.push_str(&html[after_open..]);
                                break;
                            }
                            Some(close_rel) => {
                                pos = after_open + close_rel + close_marker.len();
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// Remove `<tag .../>` or `<tag ...>` self-closing elements (case-insensitive).
fn strip_self_closing_tag(html: &str, tag_name: &str) -> String {
    let html_lower = html.to_lowercase();
    let open_marker = format!("<{}", tag_name);
    let mut out = String::with_capacity(html.len());
    let mut pos = 0;
    let len = html.len();
    while pos < len {
        let remaining = &html_lower[pos..];
        match remaining.find(&open_marker) {
            None => {
                out.push_str(&html[pos..]);
                break;
            }
            Some(rel) => {
                out.push_str(&html[pos..pos + rel]);
                let tag_start = pos + rel;
                match html[tag_start..].find('>') {
                    None => {
                        out.push_str(&html[tag_start..]);
                        break;
                    }
                    Some(gt) => {
                        pos = tag_start + gt + 1;
                    }
                }
            }
        }
    }
    out
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
fn extract_content_core(markdown: &str) -> String {
    const MIN_PARAGRAPH_LEN: usize = 30;

    let paragraphs: Vec<&str> = markdown
        .split("\n\n")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();

    if paragraphs.len() < 3 {
        // Too few paragraphs to meaningfully extract a core.
        return markdown.to_owned();
    }

    let total_chars: usize = paragraphs.iter().map(|p| p.len()).sum();
    if total_chars == 0 {
        return markdown.to_owned();
    }

    // Find the longest paragraph as the anchor.
    let anchor = match paragraphs.iter().enumerate().max_by_key(|(_, p)| p.len()) {
        Some((idx, _)) => idx,
        None => return markdown.to_owned(),
    };

    // Expand left from the anchor.
    let mut core_start = anchor;
    while core_start > 0 && paragraphs[core_start - 1].len() >= MIN_PARAGRAPH_LEN {
        core_start -= 1;
    }

    // Expand right from the anchor.
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

fn schema_of<I: JsonSchema>() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(I))
        .unwrap_or_else(|_| serde_json::json!({"type": "object"}))
}

fn truncate_chars(text: &str, max_chars: usize) -> (String, bool) {
    if text.chars().count() <= max_chars {
        return (text.to_owned(), false);
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("\n\n[... output truncated ...]");
    (out, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_content_core_drops_boilerplate() {
        let input = "\
# Nav\nHome | About | Contact\n\n\
# Footer\nCopyright 2025\n\n\
# Main Article\n\
This is the first paragraph of the main article.\n\
\n\
This is the second paragraph of the main article.\n\
\n\
This is the third paragraph with important details.";

        let core = extract_content_core(input);
        // The core should contain the "Main Article" section (the largest contiguous block).
        assert!(core.contains("Main Article"));
        assert!(core.contains("first paragraph"));
        assert!(core.contains("third paragraph"));
        // The small nav/footer blocks should not be in the core.
        assert!(!core.contains("Copyright 2025"));
        assert!(!core.contains("Home | About"));
    }

    #[test]
    fn extract_content_core_full_when_too_small() {
        // Only two paragraphs — not enough to extract a core, returns full content.
        let input = "Single paragraph only.\n\nShort second paragraph.";
        let core = extract_content_core(input);
        assert_eq!(core, input);
    }

    #[test]
    fn extract_content_core_single_section() {
        // All paragraphs are part of one large block — should return all.
        let input = "\
Introduction paragraph with some context.\n\n\
Main body paragraph with the bulk of the content here.\n\n\
Conclusion paragraph wrapping everything up nicely.";
        let core = extract_content_core(input);
        assert!(core.contains("Introduction"));
        assert!(core.contains("Main body"));
        assert!(core.contains("Conclusion"));
    }
}
