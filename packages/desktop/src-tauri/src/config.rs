//! Non-secret provider preferences + keychain-backed API keys.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use keyring::Entry;
use serde::{Deserialize, Serialize};

use crate::error::{DesktopError, DesktopResult};

const SERVICE: &str = "agentloop.desktop";
const KEYS_ACCOUNT: &str = "provider_keys";

/// Which built-in plugins are folded into the engine at composition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPrefs {
    #[serde(default = "default_true")]
    pub search: bool,
    #[serde(default)]
    pub learning: bool,
    #[serde(default)]
    pub verifier: bool,
}

fn default_true() -> bool {
    true
}

impl Default for PluginPrefs {
    fn default() -> Self {
        Self {
            search: true,
            learning: false,
            verifier: false,
        }
    }
}

/// Persisted (non-secret) provider preferences on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPrefs {
    /// Preferred provider id (e.g. `anthropic`, `openai`).
    pub preferred_provider: Option<String>,
    /// Optional base URL / host override for the preferred provider.
    pub base_url: Option<String>,
    /// Default model id (optionally `provider/`-qualified).
    pub default_model: Option<String>,
    /// Working directory for new sessions.
    pub cwd: Option<String>,
    /// Built-in plugins enabled at composition.
    #[serde(default)]
    pub plugins: PluginPrefs,
    /// Engine-wide fallback model chain (`provider/model` ids).
    #[serde(default)]
    pub fallback_models: Vec<String>,
    /// Default isolation for newly created sessions.
    #[serde(default)]
    pub default_isolation: Option<String>,
}

impl Default for ProviderPrefs {
    fn default() -> Self {
        Self {
            preferred_provider: None,
            base_url: None,
            default_model: None,
            cwd: None,
            plugins: PluginPrefs::default(),
            fallback_models: Vec::new(),
            default_isolation: None,
        }
    }
}

/// Full runtime config: prefs + secrets loaded from the OS keychain.
#[derive(Debug, Clone, Default)]
pub struct ProviderConfig {
    pub prefs: ProviderPrefs,
    /// provider id → API key
    pub keys: BTreeMap<String, String>,
}

/// Safe view returned to the frontend (keys masked).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigView {
    pub preferred_provider: Option<String>,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
    pub cwd: Option<String>,
    /// Provider ids that have a stored API key (values never returned).
    pub configured_providers: Vec<String>,
    pub has_any_key: bool,
    pub plugins: PluginPrefs,
    pub fallback_models: Vec<String>,
    pub default_isolation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProviderConfigInput {
    pub preferred_provider: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
    pub cwd: Option<String>,
    pub plugins: Option<PluginPrefs>,
    pub fallback_models: Option<Vec<String>>,
    pub default_isolation: Option<String>,
}

fn prefs_path() -> DesktopResult<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| DesktopError::Config("no config directory".into()))?
        .join("agentloop")
        .join("desktop");
    fs::create_dir_all(&dir).map_err(|e| DesktopError::Config(e.to_string()))?;
    Ok(dir.join("provider_prefs.json"))
}

pub fn sessions_dir() -> DesktopResult<PathBuf> {
    let dir = dirs::data_dir()
        .ok_or_else(|| DesktopError::Config("no data directory".into()))?
        .join("agentloop")
        .join("desktop")
        .join("sessions");
    fs::create_dir_all(&dir).map_err(|e| DesktopError::Config(e.to_string()))?;
    Ok(dir)
}

pub fn worktrees_dir() -> DesktopResult<PathBuf> {
    let dir = dirs::data_dir()
        .ok_or_else(|| DesktopError::Config("no data directory".into()))?
        .join("agentloop")
        .join("desktop")
        .join("worktrees");
    fs::create_dir_all(&dir).map_err(|e| DesktopError::Config(e.to_string()))?;
    Ok(dir)
}

pub fn load_prefs() -> DesktopResult<ProviderPrefs> {
    let path = prefs_path()?;
    if !path.exists() {
        return Ok(ProviderPrefs::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| DesktopError::Config(e.to_string()))?;
    serde_json::from_str(&raw).map_err(|e| DesktopError::Config(e.to_string()))
}

pub fn save_prefs(prefs: &ProviderPrefs) -> DesktopResult<()> {
    let path = prefs_path()?;
    let raw = serde_json::to_string_pretty(prefs).map_err(|e| DesktopError::Config(e.to_string()))?;
    fs::write(&path, raw).map_err(|e| DesktopError::Config(e.to_string()))
}

fn load_keys() -> DesktopResult<BTreeMap<String, String>> {
    let entry = Entry::new(SERVICE, KEYS_ACCOUNT)
        .map_err(|e| DesktopError::Keychain(e.to_string()))?;
    match entry.get_password() {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| DesktopError::Keychain(e.to_string())),
        Err(keyring::Error::NoEntry) => Ok(BTreeMap::new()),
        Err(e) => Err(DesktopError::Keychain(e.to_string())),
    }
}

fn save_keys(keys: &BTreeMap<String, String>) -> DesktopResult<()> {
    let entry = Entry::new(SERVICE, KEYS_ACCOUNT)
        .map_err(|e| DesktopError::Keychain(e.to_string()))?;
    if keys.is_empty() {
        let _ = entry.delete_credential();
        return Ok(());
    }
    let raw = serde_json::to_string(keys).map_err(|e| DesktopError::Keychain(e.to_string()))?;
    entry
        .set_password(&raw)
        .map_err(|e| DesktopError::Keychain(e.to_string()))
}

pub fn load_config() -> DesktopResult<ProviderConfig> {
    Ok(ProviderConfig {
        prefs: load_prefs()?,
        keys: load_keys()?,
    })
}

pub fn persist_config(cfg: &ProviderConfig) -> DesktopResult<()> {
    save_prefs(&cfg.prefs)?;
    save_keys(&cfg.keys)?;
    Ok(())
}

impl ProviderConfig {
    pub fn view(&self) -> ProviderConfigView {
        let configured: Vec<String> = self.keys.keys().cloned().collect();
        ProviderConfigView {
            preferred_provider: self.prefs.preferred_provider.clone(),
            base_url: self.prefs.base_url.clone(),
            default_model: self.prefs.default_model.clone(),
            cwd: self.prefs.cwd.clone(),
            has_any_key: !configured.is_empty(),
            configured_providers: configured,
            plugins: self.prefs.plugins.clone(),
            fallback_models: self.prefs.fallback_models.clone(),
            default_isolation: self.prefs.default_isolation.clone(),
        }
    }

    pub fn is_ready(&self) -> bool {
        let Some(preferred) = self.prefs.preferred_provider.as_deref() else {
            return false;
        };
        // Ollama needs a host, not an API key.
        if preferred == "ollama" {
            return true;
        }
        self.keys.contains_key(preferred)
    }
}
