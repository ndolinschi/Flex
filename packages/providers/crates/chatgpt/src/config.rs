use agentloop_contracts::ProviderId;
use agentloop_core::ProviderError;
use agentloop_provider_openai::oauth_tokens_discoverable;

pub const CHATGPT_PROVIDER_ID: &str = "chatgpt";
pub const CODEX_RESPONSES_URL: &str = "https://chatgpt.com/backend-api/codex/responses";

pub const DEFAULT_CHATGPT_MODEL: &str = "gpt-5.6-terra";

pub(crate) const CODEX_ORIGINATOR: &str = "codex_cli_rs";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatgptConfig {
    pub default_model: String,
    pub endpoint: String,
    pub account_id: Option<String>,
}

impl ChatgptConfig {
    pub fn from_oauth(model: Option<String>) -> Result<Self, ProviderError> {
        if !oauth_tokens_discoverable() {
            return Err(ProviderError::AuthMissing {
                provider: ProviderId::from(CHATGPT_PROVIDER_ID),
                hint: "sign in with ChatGPT Plus/Pro (desktop: Settings → Models, or CLI OAuth)"
                    .to_owned(),
            });
        }
        let account_id = agentloop_provider_openai::oauth_account_id();
        Ok(Self {
            default_model: model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(DEFAULT_CHATGPT_MODEL)
                .to_owned(),
            endpoint: CODEX_RESPONSES_URL.to_owned(),
            account_id,
        })
    }

    pub fn discoverable() -> bool {
        oauth_tokens_discoverable()
    }
}
