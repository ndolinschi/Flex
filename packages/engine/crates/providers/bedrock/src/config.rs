//! Bedrock endpoint + auth configuration.

/// Registry id for the Bedrock provider.
pub const BEDROCK_PROVIDER_ID: &str = "bedrock";

/// Default region when none is configured.
const DEFAULT_REGION: &str = "us-east-1";

/// Default model — a stable, widely-available Bedrock model id. Override with
/// `BEDROCK_MODEL` or `/model bedrock/<id>`; `list_models` fetches the live set.
const DEFAULT_MODEL: &str = "anthropic.claude-3-5-sonnet-20241022-v2:0";

/// How the provider authenticates to AWS.
#[derive(Debug, Clone)]
pub enum BedrockAuth {
    /// Bedrock API key (bearer token, `AWS_BEARER_TOKEN_BEDROCK`).
    Bearer(String),
    /// SigV4 with AWS credentials (`AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`
    /// / optional `AWS_SESSION_TOKEN`).
    SigV4 {
        access_key_id: String,
        secret_access_key: String,
        session_token: Option<String>,
    },
    /// No credentials resolved; requests fail with an actionable error.
    None,
}

impl BedrockAuth {
    /// Whether any usable credential is present.
    pub fn is_present(&self) -> bool {
        !matches!(self, BedrockAuth::None)
    }
}

/// Resolved Bedrock configuration.
#[derive(Debug, Clone)]
pub struct BedrockConfig {
    /// AWS region, e.g. `us-east-1`.
    pub region: String,
    /// How to authenticate.
    pub auth: BedrockAuth,
    /// Default model id used when a request carries no model.
    pub default_model: String,
}

impl BedrockConfig {
    /// Build with an explicit auth strategy.
    pub fn new(
        region: impl Into<String>,
        auth: BedrockAuth,
        default_model: Option<String>,
    ) -> Self {
        let region = non_empty(region.into()).unwrap_or_else(|| DEFAULT_REGION.to_owned());
        let default_model = default_model
            .and_then(non_empty)
            .unwrap_or_else(|| DEFAULT_MODEL.to_owned());
        Self {
            region,
            auth,
            default_model,
        }
    }

    /// Convenience: bearer-token config (empty key → `None` auth).
    pub fn bearer(
        region: impl Into<String>,
        api_key: impl Into<String>,
        default_model: Option<String>,
    ) -> Self {
        let auth = match non_empty(api_key.into()) {
            Some(key) => BedrockAuth::Bearer(key),
            None => BedrockAuth::None,
        };
        Self::new(region, auth, default_model)
    }

    /// Read region/credentials/model from the environment.
    ///
    /// Region: `BEDROCK_REGION` → `AWS_REGION` → `AWS_DEFAULT_REGION` → default.
    /// Auth: `AWS_BEARER_TOKEN_BEDROCK` (preferred), else the SigV4 credential
    /// trio, else none. Model: `BEDROCK_MODEL` → default.
    pub fn from_env() -> Self {
        let region = env("BEDROCK_REGION")
            .or_else(|| env("AWS_REGION"))
            .or_else(|| env("AWS_DEFAULT_REGION"))
            .unwrap_or_else(|| DEFAULT_REGION.to_owned());
        let default_model = env("BEDROCK_MODEL").unwrap_or_else(|| DEFAULT_MODEL.to_owned());
        let auth = resolve_env_auth();
        Self {
            region,
            auth,
            default_model,
        }
    }

    /// Base host for the runtime (inference) endpoint.
    pub fn runtime_host(&self) -> String {
        format!("bedrock-runtime.{}.amazonaws.com", self.region)
    }

    /// Base host for the control-plane (model listing) endpoint.
    pub fn control_host(&self) -> String {
        format!("bedrock.{}.amazonaws.com", self.region)
    }

    /// The `converse-stream` URL for `model` (model id percent-encoded for the
    /// path — Bedrock model ids contain `:`).
    pub fn converse_stream_url(&self, model: &str) -> String {
        format!(
            "https://{}/model/{}/converse-stream",
            self.runtime_host(),
            encode_model_id(model)
        )
    }

    /// The `ListFoundationModels` control-plane URL. No query — results are
    /// filtered client-side, which keeps SigV4 canonicalization query-free.
    pub fn foundation_models_url(&self) -> String {
        format!("https://{}/foundation-models", self.control_host())
    }

    /// The `ListInferenceProfiles` control-plane URL (cross-region profiles).
    pub fn inference_profiles_url(&self) -> String {
        format!("https://{}/inference-profiles", self.control_host())
    }
}

/// Resolve auth from the environment: bearer token wins, then SigV4 creds.
fn resolve_env_auth() -> BedrockAuth {
    if let Some(token) = env("AWS_BEARER_TOKEN_BEDROCK") {
        return BedrockAuth::Bearer(token);
    }
    match (env("AWS_ACCESS_KEY_ID"), env("AWS_SECRET_ACCESS_KEY")) {
        (Some(access_key_id), Some(secret_access_key)) => BedrockAuth::SigV4 {
            access_key_id,
            secret_access_key,
            session_token: env("AWS_SESSION_TOKEN"),
        },
        _ => BedrockAuth::None,
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
        let config = BedrockConfig::bearer("eu-west-1", "key", None);
        assert_eq!(
            config.converse_stream_url("anthropic.claude-3-5-sonnet-20241022-v2:0"),
            "https://bedrock-runtime.eu-west-1.amazonaws.com/model/\
             anthropic.claude-3-5-sonnet-20241022-v2%3A0/converse-stream"
        );
    }

    #[test]
    fn control_plane_urls_use_bedrock_host() {
        let config = BedrockConfig::bearer("us-east-1", "key", None);
        assert!(
            config
                .foundation_models_url()
                .starts_with("https://bedrock.us-east-1.amazonaws.com/foundation-models")
        );
        assert_eq!(
            config.inference_profiles_url(),
            "https://bedrock.us-east-1.amazonaws.com/inference-profiles"
        );
    }

    #[test]
    fn blanks_fall_back_to_defaults_and_none_auth() {
        let config = BedrockConfig::bearer("  ", "", Some("   ".to_owned()));
        assert_eq!(config.region, DEFAULT_REGION);
        assert_eq!(config.default_model, DEFAULT_MODEL);
        assert!(!config.auth.is_present());
    }

    #[test]
    fn bearer_auth_is_present_when_key_given() {
        let config = BedrockConfig::bearer("us-east-1", "abc", None);
        assert!(matches!(config.auth, BedrockAuth::Bearer(_)));
        assert!(config.auth.is_present());
    }
}
