pub const BEDROCK_PROVIDER_ID: &str = "bedrock";

const DEFAULT_REGION: &str = "us-east-1";

const DEFAULT_MODEL: &str = "anthropic.claude-3-5-sonnet-20241022-v2:0";

#[derive(Debug, Clone)]
pub enum BedrockAuth {
    Bearer(String),

    SigV4 {
        access_key_id: String,
        secret_access_key: String,
        session_token: Option<String>,
    },

    None,
}

impl BedrockAuth {
    pub fn is_present(&self) -> bool {
        !matches!(self, BedrockAuth::None)
    }
}

#[derive(Debug, Clone)]
pub struct BedrockConfig {
    pub region: String,

    pub auth: BedrockAuth,

    pub default_model: String,
}

impl BedrockConfig {
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

    pub fn runtime_host(&self) -> String {
        format!("bedrock-runtime.{}.amazonaws.com", self.region)
    }

    pub fn control_host(&self) -> String {
        format!("bedrock.{}.amazonaws.com", self.region)
    }

    pub fn converse_stream_url(&self, model: &str) -> String {
        format!(
            "https://{}/model/{}/converse-stream",
            self.runtime_host(),
            encode_model_id(model)
        )
    }

    pub fn foundation_models_url(&self) -> String {
        format!("https://{}/foundation-models", self.control_host())
    }

    pub fn inference_profiles_url(&self) -> String {
        format!("https://{}/inference-profiles", self.control_host())
    }

    pub fn inference_profiles_page_url(
        &self,
        next_token: Option<&str>,
        profile_type: &str,
    ) -> String {
        let mut url = format!(
            "https://{}/inference-profiles?maxResults=1000",
            self.control_host()
        );
        if let Some(token) = next_token {
            url.push_str("&nextToken=");
            url.push_str(&percent_encode_rfc3986(token));
        }
        url.push_str("&typeEquals=");
        url.push_str(profile_type);
        url
    }
}

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

fn encode_model_id(model: &str) -> String {
    percent_encode_rfc3986(model)
}

fn percent_encode_rfc3986(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
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
    fn control_and_listing_endpoints_follow_the_region() {
        let config = BedrockConfig::bearer("eu-west-1", "key", None);
        assert_eq!(config.control_host(), "bedrock.eu-west-1.amazonaws.com");
        assert!(
            config
                .foundation_models_url()
                .starts_with("https://bedrock.eu-west-1.amazonaws.com/")
        );
        assert!(
            config
                .inference_profiles_page_url(None, "SYSTEM_DEFINED")
                .starts_with("https://bedrock.eu-west-1.amazonaws.com/")
        );
    }

    #[test]
    fn inference_profiles_page_url_requests_full_page_by_type() {
        let config = BedrockConfig::bearer("us-east-1", "key", None);
        assert_eq!(
            config.inference_profiles_page_url(None, "SYSTEM_DEFINED"),
            "https://bedrock.us-east-1.amazonaws.com/inference-profiles\
             ?maxResults=1000&typeEquals=SYSTEM_DEFINED"
        );
    }

    #[test]
    fn inference_profiles_page_url_encodes_continuation_token() {
        let config = BedrockConfig::bearer("us-east-1", "key", None);
        let url = config.inference_profiles_page_url(Some("a/b+c=="), "SYSTEM_DEFINED");
        assert_eq!(
            url,
            "https://bedrock.us-east-1.amazonaws.com/inference-profiles\
             ?maxResults=1000&nextToken=a%2Fb%2Bc%3D%3D&typeEquals=SYSTEM_DEFINED"
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
