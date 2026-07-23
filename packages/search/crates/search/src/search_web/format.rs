use schemars::JsonSchema;

use crate::search_backend::SearchResult;

pub(crate) fn format_search_results(query: &str, results: &[SearchResult]) -> String {
    let mut out = String::new();
    out.push_str("## Search results for \"");
    out.push_str(query);
    out.push_str("\"\n\n");
    for (i, result) in results.iter().enumerate() {
        out.push_str(&(i + 1).to_string());
        out.push_str(". [");
        out.push_str(&result.title);
        out.push_str("](");
        out.push_str(&result.url);
        out.push_str(")\n   ");
        out.push_str(&result.snippet);
        out.push('\n');
    }
    out
}

pub(crate) fn schema_of<I: JsonSchema>() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(I))
        .unwrap_or_else(|_| serde_json::json!({"type": "object"}))
}

pub(crate) fn truncate_chars(text: &str, max_chars: usize) -> (String, bool) {
    if text.chars().count() <= max_chars {
        return (text.to_owned(), false);
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("\n\n[... output truncated ...]");
    (out, true)
}
