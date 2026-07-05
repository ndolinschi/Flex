//! Humanized engine errors: map [`EngineError`] codes to actionable one-line
//! headlines and demote provider payloads to a dim detail line.

use agentloop_contracts::{EngineError, ErrorCode, Provenance};

/// A user-facing error: one actionable headline plus optional dim detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HumanError {
    /// The `✗ …` line: what failed and what to do about it.
    pub headline: String,
    /// Dim second line: error code, provenance, raw payload message.
    pub detail: Option<String>,
}

/// Map a structured engine error to a human message.
///
/// Returns `None` for [`ErrorCode::Cancelled`] — cancellation is not an
/// error and renders nothing.
pub fn humanize_engine_error(err: &EngineError) -> Option<HumanError> {
    let provider = provenance_name(&err.provenance);
    let message = clean_message(&err.message);
    let headline = match err.code {
        ErrorCode::Cancelled => return None,
        ErrorCode::ModelUnavailable => {
            let what = if message.is_empty() {
                match &provider {
                    Some(provider) => format!("model isn't available on {provider}"),
                    None => "model isn't available".to_owned(),
                }
            } else {
                message.clone()
            };
            format!("{what} — /models to pick another")
        }
        ErrorCode::AuthMissing | ErrorCode::AuthExpired => {
            let provider = provider.as_deref().unwrap_or("provider");
            format!("not signed in to {provider} — /provider {provider} to authenticate")
        }
        ErrorCode::RateLimited => {
            let by = provider
                .as_deref()
                .map(|provider| format!("rate limited by {provider}"))
                .unwrap_or_else(|| "rate limited".to_owned());
            match err.retry_after_ms {
                Some(ms) => format!("{by} — retrying in {}s may work", ms.div_ceil(1000)),
                None => format!("{by} — retrying may work"),
            }
        }
        ErrorCode::NotInstalled => {
            let agent = match &err.provenance {
                Provenance::Delegator { agent_id, .. } => agent_id.as_str(),
                _ => provider.as_deref().unwrap_or("agent"),
            };
            format!("{agent} CLI not found — install it or use /provider")
        }
        ErrorCode::ContextOverflow => "context window exceeded — /compact to summarize".to_owned(),
        ErrorCode::InvalidRequest if looks_like_context_overflow_message(&message) => {
            "context window exceeded — /compact to summarize".to_owned()
        }
        _ => {
            if message.is_empty() {
                format!("{:?} error", err.code)
            } else {
                message.clone()
            }
        }
    };
    Some(HumanError {
        headline,
        detail: detail_line(err, &provider, &message),
    })
}

fn looks_like_context_overflow_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    let tokenish = lower.contains("token")
        || lower.contains("context")
        || lower.contains("prompt")
        || lower.contains("maximum");
    if !tokenish {
        return false;
    }
    lower.contains("exceed")
        || lower.contains("too large")
        || lower.contains("too long")
        || lower.contains("context length")
        || lower.contains("context window")
        || lower.contains("maximum context")
        || lower.contains("max context")
}

/// Extract a human message from a trailing JSON payload in `text`.
///
/// Scans `{` positions, parses the first tail that is valid JSON, and walks
/// common error shapes (`error.message`, `message`, `error`).
pub fn extract_json_message(text: &str) -> Option<String> {
    extract_json_tail(text).map(|(_, message)| message)
}

/// `(payload_start, extracted_message)` for the first parseable JSON tail.
fn extract_json_tail(text: &str) -> Option<(usize, String)> {
    for (start, _) in text.match_indices('{') {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(text[start..].trim()) else {
            continue;
        };
        return json_message(&value).map(|message| (start, message));
    }
    None
}

/// The cleaned message: JSON tail replaced by its embedded message when found.
fn clean_message(message: &str) -> String {
    match extract_json_tail(message) {
        Some((start, extracted)) => {
            // Keep any human-readable prefix before the JSON payload.
            let prefix = message[..start].trim();
            if prefix.is_empty() {
                extracted
            } else {
                format!("{} {extracted}", prefix.trim_end_matches(':').trim_end())
            }
        }
        None => message.trim().to_owned(),
    }
}

fn json_message(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => {
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_owned())
        }
        serde_json::Value::Object(map) => map
            .get("error")
            .and_then(json_message)
            .or_else(|| map.get("message").and_then(json_message)),
        _ => None,
    }
}

fn provenance_name(provenance: &Provenance) -> Option<String> {
    match provenance {
        Provenance::Native { provider } => Some(provider.to_string()),
        Provenance::Delegator { agent_id, .. } => Some(agent_id.clone()),
        Provenance::Engine => None,
        _ => None,
    }
}

/// Dim detail: `code · provider` plus the raw message when the headline
/// replaced it with a canned action.
fn detail_line(err: &EngineError, provider: &Option<String>, cleaned: &str) -> Option<String> {
    let mut parts = vec![code_label(err.code).to_owned()];
    if let Some(provider) = provider {
        parts.push(provider.clone());
    }
    // Codes with canned headlines lose the original message; keep it here.
    let message_dropped = matches!(
        err.code,
        ErrorCode::AuthMissing
            | ErrorCode::AuthExpired
            | ErrorCode::RateLimited
            | ErrorCode::NotInstalled
            | ErrorCode::ContextOverflow
    ) || (err.code == ErrorCode::InvalidRequest
        && looks_like_context_overflow_message(cleaned));
    if message_dropped && !cleaned.is_empty() {
        parts.push(cleaned.to_owned());
    }
    Some(parts.join(" · "))
}

fn code_label(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::AuthMissing => "auth_missing",
        ErrorCode::AuthExpired => "auth_expired",
        ErrorCode::RateLimited => "rate_limited",
        ErrorCode::ModelUnavailable => "model_unavailable",
        ErrorCode::PermissionDenied => "permission_denied",
        ErrorCode::Cancelled => "cancelled",
        ErrorCode::ProcessCrashed => "process_crashed",
        ErrorCode::ProtocolViolation => "protocol_violation",
        ErrorCode::Timeout => "timeout",
        ErrorCode::NotInstalled => "not_installed",
        ErrorCode::InvalidRequest => "invalid_request",
        ErrorCode::ContextOverflow => "context_overflow",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::ProviderId;

    fn native_error(code: ErrorCode, message: &str) -> EngineError {
        EngineError {
            code,
            message: message.to_owned(),
            retryable: false,
            provenance: Provenance::Native {
                provider: ProviderId::from("copilot"),
            },
            retry_after_ms: None,
            detail: None,
        }
    }

    #[test]
    fn cancelled_is_suppressed() {
        let err = native_error(ErrorCode::Cancelled, "turn cancelled");
        assert_eq!(humanize_engine_error(&err), None);
    }

    #[test]
    fn model_unavailable_appends_models_hint() {
        let err = native_error(
            ErrorCode::ModelUnavailable,
            "model \"gpt-5.4-mini\" isn't available",
        );
        let human = humanize_engine_error(&err).expect("not suppressed");
        assert_eq!(
            human.headline,
            "model \"gpt-5.4-mini\" isn't available — /models to pick another"
        );
        assert_eq!(human.detail.as_deref(), Some("model_unavailable · copilot"));
    }

    #[test]
    fn auth_missing_names_the_provider() {
        let err = native_error(ErrorCode::AuthMissing, "no token");
        let human = humanize_engine_error(&err).expect("not suppressed");
        assert_eq!(
            human.headline,
            "not signed in to copilot — /provider copilot to authenticate"
        );
        assert_eq!(
            human.detail.as_deref(),
            Some("auth_missing · copilot · no token")
        );
    }

    #[test]
    fn rate_limited_uses_retry_after() {
        let mut err = native_error(ErrorCode::RateLimited, "");
        err.retry_after_ms = Some(2500);
        let human = humanize_engine_error(&err).expect("not suppressed");
        assert_eq!(
            human.headline,
            "rate limited by copilot — retrying in 3s may work"
        );
    }

    #[test]
    fn not_installed_names_the_agent_cli() {
        let err = EngineError {
            code: ErrorCode::NotInstalled,
            message: "spawn failed".to_owned(),
            retryable: false,
            provenance: Provenance::Delegator {
                agent_id: "claude-code".to_owned(),
                exit_code: None,
                stderr_tail: None,
            },
            retry_after_ms: None,
            detail: None,
        };
        let human = humanize_engine_error(&err).expect("not suppressed");
        assert_eq!(
            human.headline,
            "claude-code CLI not found — install it or use /provider"
        );
    }

    #[test]
    fn context_overflow_suggests_compact() {
        let err = native_error(ErrorCode::ContextOverflow, "too many tokens");
        let human = humanize_engine_error(&err).expect("not suppressed");
        assert_eq!(
            human.headline,
            "context window exceeded — /compact to summarize"
        );
    }

    #[test]
    fn copilot_token_limit_invalid_request_shows_compact_hint() {
        let err = native_error(
            ErrorCode::InvalidRequest,
            "invalid request to copilot prompt token count of 383156 exceeds the limit of 136000",
        );
        let human = humanize_engine_error(&err).expect("not suppressed");
        assert_eq!(
            human.headline,
            "context window exceeded — /compact to summarize"
        );
        assert!(human.detail.unwrap().contains("383156"));
    }

    #[test]
    fn unknown_code_uses_cleaned_message() {
        let err = native_error(
            ErrorCode::InvalidRequest,
            "request rejected: {\"error\":{\"message\":\"bad tool schema\"}}",
        );
        let human = humanize_engine_error(&err).expect("not suppressed");
        assert_eq!(human.headline, "request rejected bad tool schema");
        assert_eq!(human.detail.as_deref(), Some("invalid_request · copilot"));
    }

    #[test]
    fn extract_json_message_walks_error_message() {
        let text = "HTTP 400 {\"error\":{\"message\":\"model not found\",\"code\":404}}";
        assert_eq!(
            extract_json_message(text).as_deref(),
            Some("model not found")
        );
    }

    #[test]
    fn extract_json_message_handles_flat_message() {
        assert_eq!(
            extract_json_message("{\"message\":\"quota exceeded\"}").as_deref(),
            Some("quota exceeded")
        );
        assert_eq!(
            extract_json_message("{\"error\":\"boom\"}").as_deref(),
            Some("boom")
        );
    }

    #[test]
    fn extract_json_message_ignores_non_json() {
        assert_eq!(extract_json_message("plain failure text"), None);
        assert_eq!(extract_json_message("trailing { not json"), None);
    }
}
