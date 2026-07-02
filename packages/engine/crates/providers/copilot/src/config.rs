//! Copilot configuration: GitHub token discovery and endpoint defaults.

use std::path::{Path, PathBuf};

use agentloop_contracts::ProviderId;
use agentloop_core::ProviderError;

pub const COPILOT_PROVIDER_ID: &str = "copilot";
/// Multiplier-free default available on every Copilot plan.
pub const DEFAULT_COPILOT_MODEL: &str = "gpt-4.1";
pub const DEFAULT_COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
/// Used when the token exchange doesn't name an API endpoint.
pub const FALLBACK_COPILOT_API_BASE: &str = "https://api.githubcopilot.com";

/// Environment variables holding a GitHub OAuth token with Copilot access,
/// checked in order.
const TOKEN_ENV_VARS: [&str; 2] = ["COPILOT_GITHUB_TOKEN", "GH_COPILOT_TOKEN"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotConfig {
    /// Long-lived GitHub OAuth token (`gho_…`/`ghu_…`) with Copilot access.
    pub github_token: String,
    pub default_model: String,
    pub token_url: String,
    pub fallback_api_base: String,
}

impl CopilotConfig {
    /// Resolve from the environment: explicit token env vars first, then an
    /// existing editor/CLI sign-in on disk.
    pub fn from_env() -> Result<Self, ProviderError> {
        let token = TOKEN_ENV_VARS
            .iter()
            .find_map(|name| std::env::var(name).ok())
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .or_else(|| default_config_dir().and_then(|dir| discover_github_token(&dir)));
        let Some(github_token) = token else {
            return Err(ProviderError::AuthMissing {
                provider: ProviderId::from(COPILOT_PROVIDER_ID),
                hint: "no GitHub Copilot credentials found: sign in with VS Code or the \
                       Copilot CLI (creates ~/.config/github-copilot/apps.json), or set \
                       `COPILOT_GITHUB_TOKEN` to a GitHub OAuth token with Copilot access"
                    .to_owned(),
            });
        };
        Ok(Self::with_token(github_token))
    }

    pub fn with_token(github_token: String) -> Self {
        Self {
            github_token,
            default_model: std::env::var("COPILOT_MODEL")
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| DEFAULT_COPILOT_MODEL.to_owned()),
            token_url: DEFAULT_COPILOT_TOKEN_URL.to_owned(),
            fallback_api_base: FALLBACK_COPILOT_API_BASE.to_owned(),
        }
    }

    /// Whether credentials are discoverable without constructing a provider —
    /// used by resolvers to decide if Copilot participates in auto-detection.
    pub fn discoverable() -> bool {
        TOKEN_ENV_VARS
            .iter()
            .any(|name| std::env::var(name).is_ok_and(|v| !v.trim().is_empty()))
            || default_config_dir()
                .map(|dir| discover_github_token(&dir).is_some())
                .unwrap_or(false)
    }
}

/// `~/.config/github-copilot` (honoring `XDG_CONFIG_HOME`) — where VS Code,
/// JetBrains, and the Copilot CLI store their sign-in.
pub(crate) fn default_config_dir() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.trim().is_empty() {
            return Some(PathBuf::from(xdg).join("github-copilot"));
        }
    }
    std::env::var("HOME")
        .ok()
        .filter(|home| !home.trim().is_empty())
        .map(|home| PathBuf::from(home).join(".config").join("github-copilot"))
}

/// Find an `oauth_token` in `apps.json` (current format: one entry per GitHub
/// app, keyed like `"github.com:Iv1.…"`) or `hosts.json` (older format,
/// keyed by host).
pub(crate) fn discover_github_token(dir: &Path) -> Option<String> {
    for file in ["apps.json", "hosts.json"] {
        let Ok(raw) = std::fs::read_to_string(dir.join(file)) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let Some(entries) = value.as_object() else {
            continue;
        };
        for (key, entry) in entries {
            if !key.starts_with("github.com") {
                continue;
            }
            if let Some(token) = entry
                .get("oauth_token")
                .and_then(|token| token.as_str())
                .map(str::trim)
                .filter(|token| !token.is_empty())
            {
                return Some(token.to_owned());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_token_from_apps_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("apps.json"),
            r#"{"github.com:Iv1.b507a08c87ecfe98":{"user":"octocat","oauth_token":"gho_test123"}}"#,
        )
        .expect("write");
        assert_eq!(
            discover_github_token(dir.path()).as_deref(),
            Some("gho_test123")
        );
    }

    #[test]
    fn discovers_token_from_hosts_json_fallback() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("hosts.json"),
            r#"{"github.com":{"user":"octocat","oauth_token":"gho_hosts456"}}"#,
        )
        .expect("write");
        assert_eq!(
            discover_github_token(dir.path()).as_deref(),
            Some("gho_hosts456")
        );
    }

    #[test]
    fn ignores_foreign_hosts_and_malformed_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("apps.json"),
            r#"{"ghe.internal:App":{"oauth_token":"gho_wrong"}}"#,
        )
        .expect("write");
        std::fs::write(dir.path().join("hosts.json"), "not json").expect("write");
        assert_eq!(discover_github_token(dir.path()), None);
    }

    #[test]
    fn missing_dir_is_not_an_error() {
        assert_eq!(
            discover_github_token(Path::new("/nonexistent/definitely-not-here")),
            None
        );
    }
}
