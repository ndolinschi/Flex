//! Bedrock endpoint + auth configuration.

/// Registry id for the Bedrock provider.
pub const BEDROCK_PROVIDER_ID: &str = "bedrock";

/// Default region when none is configured.
const DEFAULT_REGION: &str = "us-east-1";

/// Default model — a stable, widely-available Bedrock model id. Override with
/// `BEDROCK_MODEL` or `/model bedrock/<id>`; `list_models` exposes more.
const DEFAULT_MODEL: &str = "anthropic.claude-3-5-sonnet-20241022-v2:0";

/// Resolved Bedrock configuration.
#[derive(Debug, Clone)]
pub struct BedrockConfig {
    /// AWS region, e.g. `us-east-1`.
    pub region: String,
    /// Bedrock API key (bearer token). Empty means unauthenticated (rejected
    /// at request time with an actionable error).
    pub api_key: String,
    /// Default model id used when a request carries no model.
    pub default_model: String,
}

impl BedrockConfig {
    /// Build from explicit values, filling blanks with defaults.
    pub fn new(
        region: impl Into<String>,
        api_key: impl Into<String>,
        default_model: Option<String>,
    ) -> Self {
        let region = non_empty(region.into()).unwrap_or_else(|| DEFAULT_REGION.to_owned());
        let default_model = default_model
            .and_then(non_empty)
            .unwrap_or_else(|| DEFAULT_MODEL.to_owned());
        Self {
            region,
            api_key: api_key.into(),
            default_model,
        }
    }

    /// Read region/key/model from the environment.
    ///
    /// Region: `BEDROCK_REGION` → `AWS_REGION` → `AWS_DEFAULT_REGION` → default.
    /// Key: `AWS_BEARER_TOKEN_BEDROCK`. Model: `BEDROCK_MODEL` → default.
    pub fn from_env() -> Self {
        let region = env("BEDROCK_REGION")
            .or_else(|| env("AWS_REGION"))
            .or_else(|| env("AWS_DEFAULT_REGION"))
            .unwrap_or_else(|| DEFAULT_REGION.to_owned());
        let api_key = env("AWS_BEARER_TOKEN_BEDROCK").unwrap_or_default();
        let default_model = env("BEDROCK_MODEL").unwrap_or_else(|| DEFAULT_MODEL.to_owned());
        Self {
            region,
            api_key,
            default_model,
        }
    }

    /// The `converse-stream` URL for `model` (model id percent-encoded for the
    /// path — Bedrock model ids contain `:`).
    pub fn converse_stream_url(&self, model: &str) -> String {
        format!(
            "https://bedrock-runtime.{}.amazonaws.com/model/{}/converse-stream",
            self.region,
            encode_model_id(model)
        )
    }
}

/// Percent-encode the characters in a Bedrock model id that are unsafe in a URL
/// path segment (`:` in `…v2:0`, `/` in ARNs). Alphanumerics and `-._~` pass
/// through unchanged.
fn encode_model_id(model: &str) -> String {
    let mut out = String::with_capacity(model.len());
    for byte in model.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().and_then(non_empty)
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_encodes_model_id_and_region() {
        let config = BedrockConfig::new("eu-west-1", "key", None);
        assert_eq!(
            config.converse_stream_url("anthropic.claude-3-5-sonnet-20241022-v2:0"),
            "https://bedrock-runtime.eu-west-1.amazonaws.com/model/\
             anthropic.claude-3-5-sonnet-20241022-v2%3A0/converse-stream"
        );
    }

    #[test]
    fn blanks_fall_back_to_defaults() {
        let config = BedrockConfig::new("  ", "", Some("   ".to_owned()));
        assert_eq!(config.region, DEFAULT_REGION);
        assert_eq!(config.default_model, DEFAULT_MODEL);
        assert!(config.api_key.is_empty());
    }
}
