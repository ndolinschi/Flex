//! The Copilot provider: Copilot session auth over the OpenAI-compatible
//! wire layer.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::{Client, Method, RequestBuilder};
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{ModelInfo, ProviderCaps, ProviderId, branding};
use agentloop_core::{ChatRequest, Provider, ProviderError, ProviderStream};
use agentloop_provider_common::status_to_provider_error;
use agentloop_provider_openai::compat;

use crate::auth::TokenExchanger;
use crate::config::{COPILOT_PROVIDER_ID, CopilotConfig};

/// Integration identity headers the Copilot API requires. These identify the
/// API dialect spoken (VS Code chat integration), which is how every
/// third-party Copilot client authenticates its request shape.
const COPILOT_INTEGRATION_ID: &str = "vscode-chat";
const EDITOR_VERSION: &str = "vscode/1.99.0";
const EDITOR_PLUGIN_VERSION: &str = "copilot-chat/0.26.0";

pub struct CopilotProvider {
    config: CopilotConfig,
    exchanger: Arc<TokenExchanger>,
    client: Client,
}

impl CopilotProvider {
    pub fn new(config: CopilotConfig) -> Self {
        Self {
            exchanger: Arc::new(TokenExchanger::new(config.clone())),
            config,
            client: Client::new(),
        }
    }

    pub fn from_env() -> Result<Self, ProviderError> {
        Ok(Self::new(CopilotConfig::from_env()?))
    }

    pub fn default_model(&self) -> &str {
        &self.config.default_model
    }

    fn provider_id() -> ProviderId {
        ProviderId::from(COPILOT_PROVIDER_ID)
    }

    fn copilot_request(&self, method: Method, url: &str, bearer: &str) -> RequestBuilder {
        self.client
            .request(method, url)
            .bearer_auth(bearer)
            .header("Accept", "application/json")
            .header("Copilot-Integration-Id", COPILOT_INTEGRATION_ID)
            .header("Editor-Version", EDITOR_VERSION)
            .header("Editor-Plugin-Version", EDITOR_PLUGIN_VERSION)
            .header("User-Agent", branding::USER_AGENT)
    }
}

#[async_trait]
impl Provider for CopilotProvider {
    fn id(&self) -> ProviderId {
        Self::provider_id()
    }

    fn capabilities(&self) -> ProviderCaps {
        ProviderCaps {
            tool_use: true,
            parallel_tool_use: true,
            vision: true,
            documents: false,
            thinking: false,
            prompt_caching: false,
            native_json_schema_tools: true,
            max_context_tokens: None,
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let provider = self.id();
        let session = self.exchanger.session(&self.client).await?;
        let url = format!("{}/models", session.api_base);
        let response = self
            .copilot_request(Method::GET, &url, &session.bearer)
            .send()
            .await
            .map_err(|err| ProviderError::Http {
                provider: provider.clone(),
                message: err.to_string(),
            })?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_else(|err| err.to_string());
            return Err(status_to_provider_error(&provider, status, body, None));
        }
        let value =
            response
                .json::<serde_json::Value>()
                .await
                .map_err(|err| ProviderError::Stream {
                    provider: provider.clone(),
                    message: format!("Copilot models response was not JSON: {err}"),
                })?;
        compat::models_from_json(&provider, value)
    }

    async fn stream_chat(
        &self,
        request: ChatRequest,
        cancel: CancellationToken,
    ) -> Result<ProviderStream, ProviderError> {
        let provider = self.id();
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled { provider });
        }

        let session = self.exchanger.session(&self.client).await?;
        let url = format!("{}/chat/completions", session.api_base);
        let model = request.model.clone();
        let body = compat::chat_body(request);

        let response = tokio::select! {
            _ = cancel.cancelled() => {
                return Err(ProviderError::Cancelled { provider });
            }
            result = self
                .copilot_request(Method::POST, &url, &session.bearer)
                .json(&body)
                .send() => {
                result.map_err(|err| ProviderError::Http {
                    provider: provider.clone(),
                    message: err.to_string(),
                })?
            }
        };

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_else(|err| err.to_string());
            return Err(status_to_provider_error(
                &provider,
                status,
                body,
                Some(&model),
            ));
        }

        Ok(compat::stream_response(provider, model, response))
    }
}
