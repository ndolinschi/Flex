use async_trait::async_trait;
use reqwest::Client;

use super::{SearchBackend, SearchError, SearchResult, http_client, map_status_error};

pub struct WikipediaBackend {
    client: Client,
}

impl WikipediaBackend {
    pub fn new() -> Self {
        Self {
            client: http_client(),
        }
    }
}

impl Default for WikipediaBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SearchBackend for WikipediaBackend {
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        let response = self
            .client
            .get("https://en.wikipedia.org/w/api.php")
            .query(&[
                ("action", "opensearch"),
                ("search", query),
                ("limit", "8"),
                ("namespace", "0"),
                ("format", "json"),
            ])
            .send()
            .await?;

        let response = match response.error_for_status() {
            Ok(r) => r,
            Err(err) => return Err(map_status_error(err)),
        };

        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|err| SearchError::ParseError(format!("Wikipedia JSON parse error: {err}")))?;

        let results = parse_wikipedia_opensearch(&value)?;
        if results.is_empty() {
            return Err(SearchError::NoResults);
        }
        Ok(results)
    }
}

fn parse_wikipedia_opensearch(value: &serde_json::Value) -> Result<Vec<SearchResult>, SearchError> {
    let arr = value
        .as_array()
        .ok_or_else(|| SearchError::ParseError("Wikipedia opensearch: expected array".into()))?;
    if arr.len() < 4 {
        return Err(SearchError::ParseError(
            "Wikipedia opensearch: expected [query, titles, descriptions, urls]".into(),
        ));
    }
    let titles = arr[1].as_array().cloned().unwrap_or_default();
    let descriptions = arr[2].as_array().cloned().unwrap_or_default();
    let urls = arr[3].as_array().cloned().unwrap_or_default();

    let mut results = Vec::new();
    for i in 0..titles.len().min(urls.len()) {
        let title = titles[i].as_str().unwrap_or("").trim();
        let url = urls[i].as_str().unwrap_or("").trim();
        if title.is_empty() || url.is_empty() {
            continue;
        }
        let snippet = descriptions
            .get(i)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        results.push(SearchResult {
            title: title.to_owned(),
            url: url.to_owned(),
            snippet: if snippet.is_empty() {
                format!("Wikipedia article: {title}")
            } else {
                snippet.to_owned()
            },
        });
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wikipedia_opensearch_parses_tuple() {
        let value = serde_json::json!([
            "Rust",
            ["Rust (programming language)", "Rust"],
            ["", "Corrosion"],
            [
                "https://en.wikipedia.org/wiki/Rust_(programming_language)",
                "https://en.wikipedia.org/wiki/Rust"
            ]
        ]);
        let results = parse_wikipedia_opensearch(&value).expect("parse");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Rust (programming language)");
        assert!(results[0].snippet.contains("Wikipedia article"));
        assert_eq!(results[1].snippet, "Corrosion");
    }
}
