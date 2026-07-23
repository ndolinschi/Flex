use agentloop_contracts::ModelRef;
use agentloop_core::ProviderRegistry;
use agentloop_engine::{EngineConfig, RoleSpec, RoleToolProfile};

const TIER_PRESETS: &[(&str, &str, &str)] = &[
    ("deepseek", "deepseek-v4-flash", "deepseek-v4-pro"),
    ("anthropic", "claude-haiku-4-5", "claude-sonnet-4-5"),
    ("openai", "gpt-4.1-mini", "gpt-4.1"),
    ("gemini", "gemini-2.0-flash", "gemini-2.5-pro"),
];

pub fn apply_research_model_tiers(
    providers: &ProviderRegistry,
    config: &mut EngineConfig,
    preferred_provider: Option<&str>,
) -> Option<ModelRef> {
    let (cheap, strong) = pick_tier_pair(providers, preferred_provider)?;
    push_role_if_absent(
        config,
        RoleSpec {
            models: vec![cheap.clone()],
            ..RoleSpec::new("searcher")
        },
    );
    push_role_if_absent(
        config,
        RoleSpec {
            models: vec![strong],
            tools: RoleToolProfile::Full,
            ..RoleSpec::new("worker")
        },
    );
    Some(cheap)
}

fn pick_tier_pair(
    providers: &ProviderRegistry,
    preferred_provider: Option<&str>,
) -> Option<(ModelRef, ModelRef)> {
    let mut ordered: Vec<&(&str, &str, &str)> = TIER_PRESETS.iter().collect();
    if let Some(preferred) = preferred_provider {
        if let Some(idx) = ordered.iter().position(|(id, _, _)| *id == preferred) {
            let preferred_preset = ordered.remove(idx);
            ordered.insert(0, preferred_preset);
        }
    }
    for &(provider_id, cheap_id, strong_id) in ordered {
        if cheap_id == strong_id {
            continue;
        }
        let cheap = ModelRef(format!("{provider_id}/{cheap_id}"));
        let strong = ModelRef(format!("{provider_id}/{strong_id}"));
        if providers.resolve(&cheap).is_some() && providers.resolve(&strong).is_some() {
            return Some((cheap, strong));
        }
    }
    None
}

fn push_role_if_absent(config: &mut EngineConfig, spec: RoleSpec) {
    if config.roles.iter().any(|role| role.name == spec.name) {
        return;
    }
    config.roles.push(spec);
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::ProviderId;
    use agentloop_testkit::MockProvider;
    use std::sync::Arc;

    fn registry_with(ids: &[&str]) -> ProviderRegistry {
        let mut providers = ProviderRegistry::new();
        for id in ids {
            providers.register(Arc::new(MockProvider::with_id(ProviderId::from(*id))));
        }
        providers
    }

    #[test]
    fn applies_deepseek_flash_pro_when_registered() {
        let providers = registry_with(&["deepseek"]);
        let mut config = EngineConfig::default();
        let cheap = apply_research_model_tiers(&providers, &mut config, None);
        assert_eq!(
            cheap.as_ref().map(|m| m.0.as_str()),
            Some("deepseek/deepseek-v4-flash")
        );
        let searcher = config.roles.iter().find(|r| r.name == "searcher").unwrap();
        assert_eq!(searcher.models[0].0, "deepseek/deepseek-v4-flash");
        let worker = config.roles.iter().find(|r| r.name == "worker").unwrap();
        assert_eq!(worker.models[0].0, "deepseek/deepseek-v4-pro");
        assert!(matches!(worker.tools, RoleToolProfile::Full));
    }

    #[test]
    fn prefers_explicit_provider_over_preset_order() {
        let providers = registry_with(&["deepseek", "anthropic"]);
        let mut config = EngineConfig::default();
        let cheap = apply_research_model_tiers(&providers, &mut config, Some("anthropic"));
        assert_eq!(
            cheap.as_ref().map(|m| m.0.as_str()),
            Some("anthropic/claude-haiku-4-5")
        );
    }

    #[test]
    fn respects_explicit_role_config() {
        let providers = registry_with(&["deepseek"]);
        let mut config = EngineConfig::default();
        config.roles.push(RoleSpec {
            models: vec![ModelRef::from("deepseek/custom-flash")],
            ..RoleSpec::new("searcher")
        });
        let cheap = apply_research_model_tiers(&providers, &mut config, None);

        assert!(cheap.is_some());
        let searcher = config.roles.iter().find(|r| r.name == "searcher").unwrap();
        assert_eq!(searcher.models[0].0, "deepseek/custom-flash");
        assert_eq!(
            config.roles.iter().filter(|r| r.name == "searcher").count(),
            1
        );
    }

    #[test]
    fn no_op_when_no_known_provider() {
        let providers = ProviderRegistry::new();
        let mut config = EngineConfig::default();
        assert!(apply_research_model_tiers(&providers, &mut config, None).is_none());
        assert!(config.roles.is_empty());
    }
}
