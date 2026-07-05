//! Model catalog for pickers: list models across every registered provider
//! concurrently, tolerating slow or failing providers.

use std::time::Duration;

use futures::future::join_all;

use agentloop_contracts::{ModelInfo, ModelRef, ProviderId};
use agentloop_core::ProviderRegistry;

/// How long one provider gets to answer `list_models`.
const LIST_TIMEOUT: Duration = Duration::from_secs(5);

/// One selectable model, provider-qualified.
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    /// The provider serving this model.
    pub provider: ProviderId,
    /// The provider's model metadata.
    pub model: ModelInfo,
}

impl CatalogEntry {
    /// The fully-qualified reference (`provider/model`) to put in
    /// [`agentloop_contracts::TurnOptions::model`].
    pub fn model_ref(&self) -> ModelRef {
        ModelRef(format!("{}/{}", self.provider, self.model.id))
    }
}

/// The merged model list plus per-provider failures (a provider that fails
/// listing stays usable for chat — surface the error, don't drop the
/// provider).
#[derive(Debug, Default)]
pub struct ModelCatalog {
    /// All models that listed successfully, in provider priority order.
    pub entries: Vec<CatalogEntry>,
    /// Providers whose listing failed, with the failure text.
    pub errors: Vec<(ProviderId, String)>,
}

impl ModelCatalog {
    /// Query every provider in `registry` concurrently, each capped at the
    /// catalog timeout.
    pub async fn fetch(registry: &ProviderRegistry) -> Self {
        let providers = registry
            .ids()
            .into_iter()
            .filter_map(|id| registry.get(&id).map(|provider| (id, provider)))
            .collect::<Vec<_>>();

        let queries = providers.into_iter().map(|(id, provider)| async move {
            let listed = tokio::time::timeout(LIST_TIMEOUT, provider.list_models()).await;
            match listed {
                Ok(Ok(models)) => (id, Ok(models)),
                Ok(Err(err)) => (id, Err(err.to_string())),
                Err(_) => (id, Err(format!("timed out after {LIST_TIMEOUT:?}"))),
            }
        });

        let mut catalog = Self::default();
        for (provider, result) in join_all(queries).await {
            match result {
                Ok(models) => {
                    catalog
                        .entries
                        .extend(models.into_iter().map(|model| CatalogEntry {
                            provider: provider.clone(),
                            model,
                        }))
                }
                Err(message) => {
                    tracing::warn!(target: "catalog", provider = %provider, "list_models failed: {message}");
                    catalog.errors.push((provider, message));
                }
            }
        }
        catalog
    }

    /// Whether nothing listed successfully.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use agentloop_contracts::{ModelInfo, ProviderCaps, ProviderId};
    use agentloop_core::{ChatRequest, Provider, ProviderError, ProviderStream};

    use super::*;

    struct AnthropicLikeProvider;

    #[async_trait]
    impl Provider for AnthropicLikeProvider {
        fn id(&self) -> ProviderId {
            ProviderId::from("anthropic")
        }

        fn capabilities(&self) -> ProviderCaps {
            ProviderCaps::default()
        }

        async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
            Ok(vec![
                ModelInfo {
                    id: "claude-sonnet-4-5".to_owned(),
                    display_name: None,
                    context_window: None,
                    reasoning: false,
                    vision: false,
                },
                ModelInfo {
                    id: "claude-haiku-4-5".to_owned(),
                    display_name: Some("Claude Haiku 4.5".to_owned()),
                    context_window: None,
                    reasoning: false,
                    vision: false,
                },
            ])
        }

        async fn stream_chat(
            &self,
            _request: ChatRequest,
            _cancel: CancellationToken,
        ) -> Result<ProviderStream, ProviderError> {
            Err(ProviderError::Cancelled {
                provider: self.id(),
            })
        }
    }

    #[tokio::test]
    async fn fetch_surfaces_haiku_models_from_provider() {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(AnthropicLikeProvider));
        let catalog = ModelCatalog::fetch(&registry).await;
        assert!(
            catalog
                .entries
                .iter()
                .any(|entry| entry.model.id == "claude-haiku-4-5"),
            "catalog entries: {:?}",
            catalog
                .entries
                .iter()
                .map(|entry| entry.model_ref().0.clone())
                .collect::<Vec<_>>()
        );
    }
}
