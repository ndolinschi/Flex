use schemars::JsonSchema;

pub(crate) const CORE_MIN_FRACTION: f64 = 0.30;

pub(crate) fn extract_content_core(markdown: &str) -> String {
    const MIN_PARAGRAPH_LEN: usize = 30;

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

#[cfg(test)]
#[path = "extract_tests.rs"]
mod tests;
