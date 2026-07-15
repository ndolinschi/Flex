//! Static OAuth-eligible model catalog for the ChatGPT Codex backend.

use agentloop_contracts::ModelInfo;

use crate::config::DEFAULT_CHATGPT_MODEL;

/// Models the ChatGPT subscription / Codex backend accepts under OAuth.
/// Aligned with OpenCode's allow-list (gpt-5.2+ family + codex variants).
pub fn static_models() -> Vec<ModelInfo> {
    [
        ("gpt-5.5", "GPT-5.5", Some(400_000), true),
        ("gpt-5.4", "GPT-5.4", Some(272_000), true),
        ("gpt-5.4-mini", "GPT-5.4 Mini", Some(272_000), true),
        ("gpt-5.3-codex", "GPT-5.3 Codex", Some(272_000), true),
        ("gpt-5.2", "GPT-5.2", Some(272_000), true),
        ("gpt-5.2-codex", "GPT-5.2 Codex", Some(272_000), true),
        ("gpt-5.1", "GPT-5.1", Some(272_000), true),
        ("gpt-5.1-codex", "GPT-5.1 Codex", Some(272_000), true),
        (
            "gpt-5.1-codex-max",
            "GPT-5.1 Codex Max",
            Some(272_000),
            true,
        ),
        (
            "gpt-5.1-codex-mini",
            "GPT-5.1 Codex Mini",
            Some(272_000),
            true,
        ),
    ]
    .into_iter()
    .map(|(id, name, context_window, reasoning)| ModelInfo {
        id: id.to_owned(),
        display_name: Some(name.to_owned()),
        context_window,
        reasoning,
        vision: true,
    })
    .collect()
}

/// Resolve a requested model id to a catalog entry, falling back to the default.
pub(crate) fn resolve_model(requested: &str) -> String {
    let trimmed = requested.trim();
    if trimmed.is_empty() {
        return DEFAULT_CHATGPT_MODEL.to_owned();
    }
    let bare = trimmed.rsplit('/').next().unwrap_or(trimmed);
    if static_models().iter().any(|m| m.id == bare) {
        bare.to_owned()
    } else {
        // Pass through unknown ids — the backend will reject if unsupported.
        bare.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_includes_default_model() {
        assert!(
            static_models()
                .iter()
                .any(|m| m.id == DEFAULT_CHATGPT_MODEL)
        );
    }

    #[test]
    fn resolve_strips_provider_prefix() {
        assert_eq!(resolve_model("chatgpt/gpt-5.4"), "gpt-5.4");
    }
}
