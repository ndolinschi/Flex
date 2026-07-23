use serde::Deserialize;

use super::SearchResult;

#[derive(Debug, Deserialize)]
pub(crate) struct DdgIaResponse {
    #[serde(default, rename = "Heading")]
    pub(crate) heading: String,
    #[serde(default, rename = "Abstract")]
    pub(crate) abstract_text: String,
    #[serde(default, rename = "AbstractText")]
    pub(crate) abstract_text_alt: String,
    #[serde(default, rename = "AbstractURL")]
    pub(crate) abstract_url: String,
    #[serde(default, rename = "AbstractSource")]
    pub(crate) abstract_source: String,
    #[serde(default, rename = "Results")]
    pub(crate) results: Vec<DdgIaLink>,
    #[serde(default, rename = "RelatedTopics")]
    pub(crate) related_topics: Vec<DdgIaTopic>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DdgIaLink {
    #[serde(default, rename = "FirstURL")]
    pub(crate) first_url: String,
    #[serde(default, rename = "Text")]
    pub(crate) text: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DdgIaTopic {
    #[serde(default, rename = "FirstURL")]
    pub(crate) first_url: Option<String>,
    #[serde(default, rename = "Text")]
    pub(crate) text: Option<String>,
    #[serde(default, rename = "Topics")]
    pub(crate) topics: Option<Vec<DdgIaTopic>>,
    #[serde(default, rename = "Name")]
    #[allow(dead_code)]
    pub(crate) name: Option<String>,
}

pub(crate) fn ddg_ia_results(parsed: DdgIaResponse) -> Vec<SearchResult> {
    let mut out = Vec::new();
    let abstract_body = if !parsed.abstract_text.is_empty() {
        parsed.abstract_text
    } else {
        parsed.abstract_text_alt
    };
    if !parsed.abstract_url.is_empty() && !abstract_body.is_empty() {
        let title = if parsed.heading.is_empty() {
            if parsed.abstract_source.is_empty() {
                parsed.abstract_url.clone()
            } else {
                parsed.abstract_source.clone()
            }
        } else if parsed.abstract_source.is_empty() {
            parsed.heading.clone()
        } else {
            format!("{} ({})", parsed.heading, parsed.abstract_source)
        };
        out.push(SearchResult {
            title,
            url: parsed.abstract_url,
            snippet: abstract_body,
        });
    }
    for link in parsed.results {
        push_ddg_link(&mut out, &link.first_url, &link.text);
    }
    flatten_ddg_topics(&mut out, &parsed.related_topics);
    out
}

fn flatten_ddg_topics(out: &mut Vec<SearchResult>, topics: &[DdgIaTopic]) {
    for topic in topics {
        if let Some(nested) = &topic.topics {
            flatten_ddg_topics(out, nested);
            continue;
        }
        let url = topic.first_url.as_deref().unwrap_or("");
        let text = topic.text.as_deref().unwrap_or("");
        push_ddg_link(out, url, text);
    }
}

fn push_ddg_link(out: &mut Vec<SearchResult>, url: &str, text: &str) {
    let url = url.trim();
    let text = text.trim();
    if url.is_empty() || text.is_empty() {
        return;
    }

    if url.contains("duckduckgo.com/c/") || url.contains("duckduckgo.com/?") {
        return;
    }
    if out.iter().any(|r| r.url == url) {
        return;
    }
    out.push(SearchResult {
        title: text.chars().take(120).collect(),
        url: url.to_owned(),
        snippet: text.to_owned(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ddg_ia_parses_abstract_and_filters_category_links() {
        let parsed = DdgIaResponse {
            heading: "Rust".into(),
            abstract_text: "A systems language.".into(),
            abstract_text_alt: String::new(),
            abstract_url: "https://en.wikipedia.org/wiki/Rust_(programming_language)".into(),
            abstract_source: "Wikipedia".into(),
            results: vec![DdgIaLink {
                first_url: "https://www.rust-lang.org/".into(),
                text: "Official site".into(),
            }],
            related_topics: vec![
                DdgIaTopic {
                    first_url: Some("https://duckduckgo.com/c/Rust_(programming_language)".into()),
                    text: Some("Category".into()),
                    topics: None,
                    name: None,
                },
                DdgIaTopic {
                    first_url: Some("https://doc.rust-lang.org/book/".into()),
                    text: Some("The Rust Book".into()),
                    topics: None,
                    name: None,
                },
            ],
        };
        let results = ddg_ia_results(parsed);
        assert_eq!(results.len(), 3);
        assert!(results[0].url.contains("wikipedia.org"));
        assert_eq!(results[1].url, "https://www.rust-lang.org/");
        assert_eq!(results[2].url, "https://doc.rust-lang.org/book/");
    }
}
