use std::sync::Arc;

use agentloop_contracts::{ContentBlock, ProviderId};
use agentloop_core::Provider;
use agentloop_core::provider::ChatRequest;

pub(crate) const MANUAL_COMPACT_STRATEGY: &str = "summarize_oldest";
pub(crate) const AUTO_COMPACT_STRATEGY: &str = "auto_summarize_oldest";

const DEFAULT_CONTEXT_LIMIT: u64 = 128_000;

const COPILOT_CONTEXT_LIMIT: u64 = 136_000;

const DEEPSEEK_CONTEXT_LIMIT: u64 = 1_000_000;

pub(crate) fn estimate_request_tokens(system: &str, request: &ChatRequest) -> u64 {
    estimate_request_chars(system, request).div_ceil(4)
}

pub(crate) fn estimate_request_chars(system: &str, request: &ChatRequest) -> u64 {
    let mut total = system.len() as u64;
    for message in &request.messages {
        for block in &message.content {
            total += match block {
                ContentBlock::Markdown { text } | ContentBlock::Thinking { text, .. } => {
                    text.len() as u64
                }
                ContentBlock::Image { data, .. } => blob_source_chars(data),
                ContentBlock::File { data, .. } => blob_source_chars(data),
                ContentBlock::Opaque { data, .. } => data.to_string().len() as u64,
                ContentBlock::ToolUse { input, name, .. } => {
                    name.len() as u64 + input.to_string().len() as u64
                }
                ContentBlock::ToolResult { content, .. } => content
                    .iter()
                    .map(|block| match block {
                        agentloop_contracts::ToolResultBlock::Markdown { text } => text.len(),
                        agentloop_contracts::ToolResultBlock::Image { data, .. } => {
                            blob_source_chars(data) as usize
                        }
                        _ => 0,
                    })
                    .sum::<usize>()
                    as u64,
                _ => 0,
            };
        }
    }
    for tool in &request.tools {
        total += tool.name.len() as u64 + tool.description.len() as u64;
        total += tool.input_schema.to_string().len() as u64;
    }
    total
}

fn blob_source_chars(source: &agentloop_contracts::BlobSource) -> u64 {
    match source {
        agentloop_contracts::BlobSource::Base64 { data } => data.len() as u64,
        agentloop_contracts::BlobSource::Url { url } => url.len() as u64,
        agentloop_contracts::BlobSource::Path { path } => path.as_os_str().len() as u64,
        _ => 0,
    }
}

pub(crate) fn resolve_context_limit(provider: &Arc<dyn Provider>) -> u64 {
    if let Some(limit) = provider.capabilities().max_context_tokens {
        return limit as u64;
    }
    provider_default_limit(&provider.id())
}

fn provider_default_limit(provider_id: &ProviderId) -> u64 {
    match provider_id.as_str() {
        "copilot" => COPILOT_CONTEXT_LIMIT,
        "deepseek" => DEEPSEEK_CONTEXT_LIMIT,
        _ => DEFAULT_CONTEXT_LIMIT,
    }
}

pub(crate) fn should_auto_compact(
    estimated_tokens: u64,
    context_limit: u64,
    threshold_percent: u64,
) -> bool {
    let pct = threshold_percent.clamp(1, 100);
    let threshold = context_limit.saturating_mul(pct) / 100;
    estimated_tokens >= threshold
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::{Message, Role};
    use agentloop_core::provider::ToolSpec;

    #[test]
    fn resolve_context_limit_prefers_provider_caps() {
        let caps = agentloop_contracts::ProviderCaps {
            max_context_tokens: Some(42_000),
            ..agentloop_contracts::ProviderCaps::default()
        };
        let provider =
            Arc::new(agentloop_testkit::MockProvider::with_caps(caps)) as Arc<dyn Provider>;
        assert_eq!(resolve_context_limit(&provider), 42_000);
    }

    #[test]
    fn resolve_context_limit_uses_copilot_default() {
        let provider = Arc::new(agentloop_testkit::MockProvider::with_caps(
            agentloop_contracts::ProviderCaps {
                max_context_tokens: None,
                ..agentloop_contracts::ProviderCaps::default()
            },
        )) as Arc<dyn Provider>;
        assert_eq!(
            provider_default_limit(&ProviderId::from("copilot")),
            COPILOT_CONTEXT_LIMIT
        );
        assert_eq!(resolve_context_limit(&provider), DEFAULT_CONTEXT_LIMIT);
    }

    #[test]
    fn provider_default_limit_knows_deepseek() {
        assert_eq!(
            provider_default_limit(&ProviderId::from("deepseek")),
            DEEPSEEK_CONTEXT_LIMIT
        );
    }

    #[test]
    fn should_auto_compact_at_eighty_five_percent() {
        let limit = 1_000;
        assert!(!should_auto_compact(849, limit, 85));
        assert!(should_auto_compact(850, limit, 85));
        assert!(should_auto_compact(900, limit, 85));
    }

    #[test]
    fn should_auto_compact_respects_custom_threshold() {
        let limit = 1_000;
        assert!(!should_auto_compact(499, limit, 50));
        assert!(should_auto_compact(500, limit, 50));
    }

    #[test]
    fn should_auto_compact_clamps_threshold_to_valid_range() {
        let limit = 1_000;
        assert!(!should_auto_compact(9, limit, 0));
        assert!(should_auto_compact(10, limit, 0));
        assert!(!should_auto_compact(999, limit, 200));
        assert!(should_auto_compact(1_000, limit, 200));
    }

    #[test]
    fn estimate_request_tokens_counts_system_and_messages() {
        let request = ChatRequest::new(
            "model",
            vec![Message {
                role: Role::User,
                content: vec![ContentBlock::markdown("abcd".to_owned())],
                cache_hint: false,
            }],
        );
        let tokens = estimate_request_tokens("abcd", &request);
        assert_eq!(tokens, 2);
    }

    #[test]
    fn estimate_request_tokens_includes_tool_specs() {
        let mut request = ChatRequest::new("model", vec![]);
        request.tools.push(ToolSpec {
            name: "read".to_owned(),
            description: "read files".to_owned(),
            input_schema: serde_json::json!({"type": "object"}),
        });
        let tokens = estimate_request_tokens("", &request);
        assert!(tokens > 0);
    }
}
