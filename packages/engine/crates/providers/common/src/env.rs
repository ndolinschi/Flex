//! Environment variable helpers for provider configuration.

use agentloop_contracts::ProviderId;
use agentloop_core::ProviderError;

/// Read an optional env var, treating empty and whitespace-only values as unset.
pub fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Read a required provider credential from the environment.
pub fn required_env(
    provider: &ProviderId,
    name: &str,
    purpose: &str,
) -> Result<String, ProviderError> {
    optional_env(name).ok_or_else(|| ProviderError::AuthMissing {
        provider: provider.clone(),
        hint: format!("set `{name}` to {purpose}"),
    })
}
