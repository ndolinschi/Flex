use crate::search_backend::SearchResult;

pub trait SearchReranker: Send + Sync {
    fn rerank(&self, query: &str, results: &[SearchResult]) -> Vec<SearchResult>;
}

pub struct KeywordReranker;

impl KeywordReranker {
    pub fn new() -> Self {
        Self
    }
}

impl Default for KeywordReranker {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchReranker for KeywordReranker {
    fn rerank(&self, query: &str, results: &[SearchResult]) -> Vec<SearchResult> {
        if results.is_empty() {
            return Vec::new();
        }

        let query_words: Vec<String> = query
            .split_whitespace()
            .map(|w| {
                w.trim_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase()
            })
            .filter(|w| !w.is_empty())
            .collect();

        if query_words.is_empty() {
            return results.to_vec();
        }

        let mut scored: Vec<(usize, &SearchResult)> = results
            .iter()
            .map(|r| {
                let title_lower = r.title.to_lowercase();
                let snippet_lower = r.snippet.to_lowercase();
                let score = query_words
                    .iter()
                    .filter(|qw| {
                        title_lower.contains(qw.as_str()) || snippet_lower.contains(qw.as_str())
                    })
                    .count();
                (score, r)
            })
            .collect();

        scored.sort_by_key(|b| std::cmp::Reverse(b.0));

        scored.into_iter().map(|(_, r)| r.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_reranker_sorts_by_relevance() {
        let results = vec![
            SearchResult {
                title: "Python history".to_owned(),
                url: "https://a.com".to_owned(),
                snippet: "Python was created by Guido van Rossum.".to_owned(),
            },
            SearchResult {
                title: "Rust programming language".to_owned(),
                url: "https://b.com".to_owned(),
                snippet: "Rust is a systems programming language focused on safety.".to_owned(),
            },
            SearchResult {
                title: "Rust for Python developers".to_owned(),
                url: "https://c.com".to_owned(),
                snippet: "Learn Rust if you already know Python.".to_owned(),
            },
        ];

        let reranker = KeywordReranker::new();
        let reranked = reranker.rerank("Rust programming", &results);

        assert_eq!(reranked[0].url, "https://b.com");
        assert_eq!(reranked[1].url, "https://c.com");
        assert_eq!(reranked[2].url, "https://a.com");
    }

    #[test]
    fn keyword_reranker_empty_query_returns_original() {
        let results = vec![SearchResult {
            title: "Test".to_owned(),
            url: "https://a.com".to_owned(),
            snippet: "Some content.".to_owned(),
        }];

        let reranker = KeywordReranker::new();
        let reranked = reranker.rerank("", &results);
        assert_eq!(reranked.len(), 1);
        assert_eq!(reranked[0].url, "https://a.com");
    }

    #[test]
    fn keyword_reranker_empty_results_returns_empty() {
        let reranker = KeywordReranker::new();
        let reranked = reranker.rerank("query", &[]);
        assert!(reranked.is_empty());
    }
}
