use super::SearchResult;

pub(crate) fn looks_like_ddg_block_page(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    lower.contains("anomaly-modal")
        || lower.contains("please complete the following challenge")
        || lower.contains("bots use duckduckgo")
        || (!lower.contains("result__body") && lower.contains("challenge"))
}

pub(crate) fn parse_duckduckgo_html(html: &str) -> Vec<SearchResult> {
    let mut results = Vec::new();
    let marker = "result__body";
    let mut search_pos = 0usize;

    while let Some(rel_start) = html[search_pos..].find(marker) {
        let block_start = search_pos + rel_start;

        let content_start = block_start + marker.len();

        let block_end = html[content_start..]
            .find("class=\"result \"")
            .map(|p| content_start + p)
            .unwrap_or(html.len());

        let block = &html[block_start..block_end];

        if let Some((title, url)) = extract_result_link(block) {
            if !is_ad_result(&url) {
                let snippet = extract_snippet(block).unwrap_or_default();
                results.push(SearchResult {
                    title,
                    url,
                    snippet,
                });
            }
        }

        search_pos = block_end;
    }

    results
}

fn extract_result_link(html: &str) -> Option<(String, String)> {
    let link_marker = "class=\"result__a\"";
    let pos = html.find(link_marker)?;
    let after_marker = &html[pos + link_marker.len()..];

    let href_start = after_marker.find("href=\"")? + 6;
    let href_end = after_marker[href_start..].find('"')?;
    let url = html_entity_decode(&after_marker[href_start..href_start + href_end]);

    let tag_close = after_marker[href_start + href_end..].find('>')?;
    let text_start = href_start + href_end + tag_close + 1;
    let text_end = after_marker[text_start..]
        .find("</a>")
        .unwrap_or(after_marker.len() - text_start);
    let title = strip_html_tags(&after_marker[text_start..text_start + text_end])
        .trim()
        .to_owned();

    if title.is_empty() || url.is_empty() {
        return None;
    }
    Some((title, url))
}

fn extract_snippet(html: &str) -> Option<String> {
    if let Some(snippet) = extract_tag_content(html, "class=\"result__snippet\"") {
        return Some(snippet);
    }
    if let Some(snippet) = extract_tag_content(html, "class=\"result-snippet\"") {
        return Some(snippet);
    }
    None
}

fn extract_tag_content(html: &str, marker: &str) -> Option<String> {
    let pos = html.find(marker)?;
    let after_marker = &html[pos + marker.len()..];
    let tag_close = after_marker.find('>')?;
    let content_start = tag_close + 1;
    let content_end = after_marker[content_start..]
        .find('<')
        .unwrap_or(after_marker.len() - content_start);
    let raw = &after_marker[content_start..content_start + content_end];
    let decoded = html_entity_decode(raw);
    let cleaned = decoded.trim().to_owned();
    if cleaned.is_empty() {
        return None;
    }
    Some(cleaned)
}

fn html_entity_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&apos;", "'")
}

fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    html_entity_decode(&out)
}

fn is_ad_result(url: &str) -> bool {
    url.contains("duckduckgo.com/y.js") || url.contains("duckduckgo.com/ac.js")
}

#[cfg(test)]
#[path = "html_parse_tests.rs"]
mod tests;
