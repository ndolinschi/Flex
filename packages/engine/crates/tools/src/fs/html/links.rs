use std::sync::LazyLock;

use regex::Regex;

use super::strip::strip_semantic_chrome;

#[allow(clippy::expect_used)]
static A_HREF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<a\s[^>]*?href\s*=\s*"([^"]*)"[^>]*>\s*(.*?)\s*</a>"#).expect("static regex")
});

fn strip_html_tags(input: &str) -> String {
    #[allow(clippy::expect_used)]
    static TAG_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"<[^>]*>").expect("static regex"));
    TAG_RE.replace_all(input, "").into_owned()
}

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

    let body = strip_semantic_chrome(html);

    for cap in A_HREF_RE.captures_iter(&body) {
        let href = &cap[1];
        let text = strip_html_tags(&cap[2]);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }

        if href.starts_with("javascript:") || href.starts_with("mailto:") || href.starts_with('#') {
            continue;
        }

        let resolved = match base.join(href) {
            Ok(url) => url,
            Err(_) => continue,
        };

        if !matches!(resolved.scheme(), "http" | "https") {
            continue;
        }

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
