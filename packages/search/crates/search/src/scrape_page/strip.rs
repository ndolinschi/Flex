//! HTML boilerplate stripping for `scrape_page`.

/// Strip boilerplate HTML tags and semantic chrome that are never useful
/// content for a model.
///
/// Removes:
/// - `<script>`, `<style>`, `<noscript>` (container tags with content)
/// - `<link>`, `<meta>` (self-closing tags)
/// - `<nav>`, `<header>`, `<footer>` (semantic chrome — entire element with
///   all descendants removed)
pub(crate) fn strip_html_boilerplate(html: &str) -> String {
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
