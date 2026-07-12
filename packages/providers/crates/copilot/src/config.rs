//! Copilot configuration: GitHub token discovery, persistence, and endpoint
//! defaults.

use std::path::{Path, PathBuf};

use agentloop_contracts::ProviderId;
use agentloop_core::ProviderError;

use crate::device_flow::COPILOT_DEVICE_CLIENT_ID;

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

/// Persist a GitHub OAuth token where every Copilot client looks for it:
/// `apps.json` under `~/.config/github-copilot` (honoring `XDG_CONFIG_HOME`),
/// keyed by the GitHub app id. Entries written by other clients are
/// preserved. Returns the path of the file written.
pub fn store_github_token(token: &str) -> Result<PathBuf, ProviderError> {
    let Some(dir) = default_config_dir() else {
        return Err(ProviderError::AuthMissing {
            provider: ProviderId::from(COPILOT_PROVIDER_ID),
            hint: "cannot store the GitHub sign-in: neither XDG_CONFIG_HOME nor HOME is set"
                .to_owned(),
        });
    };
    store_github_token_in(&dir, token)
}

pub(crate) fn store_github_token_in(dir: &Path, token: &str) -> Result<PathBuf, ProviderError> {
    if !dir.exists() {
        std::fs::create_dir_all(dir).map_err(|err| store_error("create", dir, &err))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|err| store_error("restrict permissions on", dir, &err))?;
        }
    }

    let path = dir.join("apps.json");
    let mut entries = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|value| match value {
            serde_json::Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default();
    entries.insert(
        format!("github.com:{COPILOT_DEVICE_CLIENT_ID}"),
        serde_json::json!({
            "oauth_token": token,
            "githubAppId": COPILOT_DEVICE_CLIENT_ID,
        }),
    );
    let body = serde_json::Value::Object(entries).to_string();

    let tmp = dir.join(format!("apps.json.tmp-{}", std::process::id()));
    std::fs::write(&tmp, &body).map_err(|err| store_error("write", &tmp, &err))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(err) = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)) {
            let _ = std::fs::remove_file(&tmp);
            return Err(store_error("restrict permissions on", &tmp, &err));
        }
    }
    if let Err(err) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(store_error("atomically replace", &path, &err));
    }
    tracing::debug!(target: "copilot", path = %path.display(), "stored the GitHub sign-in");
    Ok(path)
}

fn store_error(action: &str, path: &Path, err: &std::io::Error) -> ProviderError {
    ProviderError::AuthMissing {
        provider: ProviderId::from(COPILOT_PROVIDER_ID),
        hint: format!(
            "the sign-in could not be saved: failed to {action} {}: {err}",
            path.display()
        ),
    }
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

    fn our_key() -> String {
        format!("github.com:{COPILOT_DEVICE_CLIENT_ID}")
    }

    fn read_apps_json(dir: &Path) -> serde_json::Value {
        let raw = std::fs::read_to_string(dir.join("apps.json")).expect("read");
        serde_json::from_str(&raw).expect("apps.json must stay valid JSON")
    }

    #[test]
    fn store_creates_the_file_and_directory() {
        let root = tempfile::tempdir().expect("tempdir");
        let dir = root.path().join("github-copilot");

        let path = store_github_token_in(&dir, "gho_new").expect("store");
        assert_eq!(path, dir.join("apps.json"));
        assert_eq!(discover_github_token(&dir).as_deref(), Some("gho_new"));

        let value = read_apps_json(&dir);
        assert_eq!(value[our_key()]["oauth_token"], "gho_new");
        assert_eq!(value[our_key()]["githubAppId"], COPILOT_DEVICE_CLIENT_ID);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir_mode = std::fs::metadata(&dir)
                .expect("dir meta")
                .permissions()
                .mode();
            assert_eq!(dir_mode & 0o777, 0o700);
            let file_mode = std::fs::metadata(&path)
                .expect("file meta")
                .permissions()
                .mode();
            assert_eq!(file_mode & 0o777, 0o600);
        }
    }

    #[test]
    fn store_preserves_foreign_entries() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("apps.json"),
            r#"{"github.com:Iv1.deadbeefdeadbeef":{"user":"octocat","oauth_token":"gho_vscode"}}"#,
        )
        .expect("write");

        store_github_token_in(dir.path(), "gho_new").expect("store");

        let value = read_apps_json(dir.path());
        assert_eq!(
            value["github.com:Iv1.deadbeefdeadbeef"]["oauth_token"], "gho_vscode",
            "foreign entry must survive"
        );
        assert_eq!(value[our_key()]["oauth_token"], "gho_new");
    }

    #[test]
    fn store_updates_an_existing_entry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let existing = format!(
            r#"{{"{}":{{"user":"octocat","oauth_token":"gho_old"}}}}"#,
            our_key()
        );
        std::fs::write(dir.path().join("apps.json"), existing).expect("write");

        store_github_token_in(dir.path(), "gho_new").expect("store");

        let value = read_apps_json(dir.path());
        assert_eq!(value[our_key()]["oauth_token"], "gho_new");
        assert_eq!(
            value.as_object().map(|entries| entries.len()),
            Some(1),
            "the entry is replaced, not duplicated"
        );
    }

    #[test]
    fn store_overwrites_an_unparseable_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("apps.json"), "not json").expect("write");

        store_github_token_in(dir.path(), "gho_new").expect("store");

        assert_eq!(
            discover_github_token(dir.path()).as_deref(),
            Some("gho_new")
        );
        read_apps_json(dir.path());
    }
}
