use agentloop_tools::fs::extract_page_links;

use super::extract::{extract_content_core, truncate_chars};
use super::strip::strip_html_boilerplate;

pub(crate) fn convert_page_body(
    bytes: &[u8],
    kept_len: usize,
    is_html: bool,
    max_bytes: usize,
    include_links: bool,
    final_url: &str,
) -> String {
    if is_html {
        let html_str = String::from_utf8_lossy(&bytes[..kept_len]);
        let pre_cleaned = strip_html_boilerplate(&html_str);
        let markdown = htmd::convert(&pre_cleaned).unwrap_or(pre_cleaned);
        let core = extract_content_core(&markdown);
        let (mut truncated, _) = truncate_chars(&core, max_bytes);

        if include_links {
            let links = extract_page_links(&html_str, final_url);
            if !links.is_empty() {
                truncated.push_str("\n\n--- Links found on this page ---\n");
                for (i, (link_url, link_text)) in links.iter().enumerate() {
                    truncated.push_str(&format!("{}. [{}]({})\n", i + 1, link_text, link_url));
                }
                truncated.push_str("\nUse `scrape_page` to explore any of these links.");
            }
        }

        truncated
    } else {
        let raw = String::from_utf8_lossy(&bytes[..kept_len.min(max_bytes)]);
        let (truncated, _) = truncate_chars(raw.as_ref(), max_bytes);
        truncated
    }
}

pub(crate) fn render_scrape_output(
    final_url: &str,
    status: u16,
    content_type: &Option<String>,
    body: &str,
    max_output_chars: usize,
) -> (String, bool) {
    let mut rendered = String::new();
    rendered.push_str("url: ");
    rendered.push_str(final_url);
    rendered.push_str("\nstatus: ");
    rendered.push_str(&status.to_string());
    if let Some(ct) = content_type {
        rendered.push_str("\ncontent_type: ");
        rendered.push_str(ct);
    }
    rendered.push_str("\n\n");
    rendered.push_str(body);
    truncate_chars(&rendered, max_output_chars)
}
