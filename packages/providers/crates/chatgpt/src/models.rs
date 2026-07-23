use agentloop_contracts::ModelInfo;

use crate::config::DEFAULT_CHATGPT_MODEL;

pub fn static_models() -> Vec<ModelInfo> {
    [
        ("gpt-5.6-sol", "GPT-5.6 Sol", Some(272_000), true),
        ("gpt-5.6-terra", "GPT-5.6 Terra", Some(272_000), true),
        ("gpt-5.6-luna", "GPT-5.6 Luna", Some(272_000), true),
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

pub(crate) fn uses_responses_lite(model: &str) -> bool {
    let bare = model.rsplit('/').next().unwrap_or(model);
    bare == "gpt-5.6" || bare.starts_with("gpt-5.6-")
}

pub(crate) fn resolve_model(requested: &str) -> String {
    let trimmed = requested.trim();
    if trimmed.is_empty() {
        return DEFAULT_CHATGPT_MODEL.to_owned();
    }
    let bare = trimmed.rsplit('/').next().unwrap_or(trimmed);

    if bare == "gpt-5.6" {
        return "gpt-5.6-sol".to_owned();
    }
    if static_models().iter().any(|m| m.id == bare) {
        bare.to_owned()
    } else {
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
    fn catalog_includes_gpt_5_6_family() {
        let ids: Vec<_> = static_models().into_iter().map(|m| m.id).collect();
        assert!(ids.contains(&"gpt-5.6-sol".to_owned()));
        assert!(ids.contains(&"gpt-5.6-terra".to_owned()));
        assert!(ids.contains(&"gpt-5.6-luna".to_owned()));
    }

    #[test]
    fn resolve_strips_provider_prefix() {
        assert_eq!(resolve_model("chatgpt/gpt-5.4"), "gpt-5.4");
    }

    #[test]
    fn resolve_alias_gpt_5_6_to_sol() {
        assert_eq!(resolve_model("chatgpt/gpt-5.6"), "gpt-5.6-sol");
    }

    #[test]
    fn lite_gate_matches_5_6_family() {
        assert!(uses_responses_lite("gpt-5.6-luna"));
        assert!(uses_responses_lite("chatgpt/gpt-5.6-sol"));
        assert!(!uses_responses_lite("gpt-5.5"));
        assert!(!uses_responses_lite("gpt-5.4"));
    }
}
