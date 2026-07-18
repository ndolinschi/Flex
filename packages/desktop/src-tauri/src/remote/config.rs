//! Persisted Remote Access preferences (non-secret).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{DesktopError, DesktopResult};
use crate::secrets::SecretsStore;

/// Secrets-store key for the remote-access bearer token.
pub const REMOTE_TOKEN_SECRET_KEY: &str = "remote:access_token";

const CONFIG_FILE: &str = "remote_access.json";
const DEFAULT_PORT: u16 = 4520;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudflarePrefs {
    #[serde(default)]
    pub enabled: bool,
    /// Optional named-tunnel hostname; empty = quick tunnel when available.
    #[serde(default)]
    pub hostname: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodPrefs {
    /// Manual host:port + token (always the baseline when remote is on).
    #[serde(default = "default_true")]
    pub manual: bool,
    /// Advertise LAN IPs and bind non-loopback.
    #[serde(default)]
    pub lan: bool,
    /// mDNS / Bonjour discovery on the LAN.
    #[serde(default)]
    pub bonjour: bool,
    /// Expose on a chosen public/WAN-facing port (same listener; UX warns).
    #[serde(default)]
    pub public_port: bool,
    #[serde(default)]
    pub cloudflare: CloudflarePrefs,
    /// Bluetooth adapter — stub in v1.
    #[serde(default)]
    pub bluetooth: bool,
}

fn default_true() -> bool {
    true
}

impl Default for MethodPrefs {
    fn default() -> Self {
        Self {
            manual: true,
            lan: false,
            bonjour: false,
            public_port: false,
            cloudflare: CloudflarePrefs::default(),
            bluetooth: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAccessConfig {
    /// Master switch — remote server is off until the user enables this.
    #[serde(default)]
    pub enabled: bool,
    /// Human label shown in pairing info (defaults to hostname).
    #[serde(default)]
    pub device_name: String,
    /// Stable UUID for this desktop install (pairing / Bonjour TXT).
    #[serde(default)]
    pub device_id: String,
    /// TCP port for the shared HTTP listener.
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub methods: MethodPrefs,
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

impl Default for RemoteAccessConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            device_name: default_device_name(),
            device_id: uuid::Uuid::now_v7().to_string(),
            port: DEFAULT_PORT,
            methods: MethodPrefs::default(),
        }
    }
}

impl RemoteAccessConfig {
    /// Whether any TCP-facing method needs a non-loopback bind.
    pub fn needs_non_loopback(&self) -> bool {
        self.methods.lan
            || self.methods.bonjour
            || self.methods.public_port
            || self.methods.cloudflare.enabled
    }

    /// Whether the shared HTTP listener should be running.
    pub fn wants_http_listener(&self) -> bool {
        self.enabled
            && (self.methods.manual
                || self.methods.lan
                || self.methods.bonjour
                || self.methods.public_port
                || self.methods.cloudflare.enabled)
    }
}

fn default_device_name() -> String {
    std::env::var("HOST")
        .or_else(|_| std::env::var("HOSTNAME"))
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Desktop".into())
}

fn config_path() -> DesktopResult<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| DesktopError::Config("no config directory".into()))?
        .join("agentloop")
        .join("desktop");
    fs::create_dir_all(&dir).map_err(|e| DesktopError::Config(e.to_string()))?;
    Ok(dir.join(CONFIG_FILE))
}

fn secrets_dir() -> DesktopResult<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| DesktopError::Config("no config directory".into()))?
        .join("agentloop")
        .join("desktop");
    fs::create_dir_all(&dir).map_err(|e| DesktopError::Config(e.to_string()))?;
    Ok(dir)
}

pub fn load_remote_config() -> DesktopResult<RemoteAccessConfig> {
    let path = config_path()?;
    if !path.exists() {
        let cfg = RemoteAccessConfig::default();
        save_remote_config(&cfg)?;
        return Ok(cfg);
    }
    let raw = fs::read_to_string(&path).map_err(|e| DesktopError::Config(e.to_string()))?;
    let mut cfg: RemoteAccessConfig =
        serde_json::from_str(&raw).map_err(|e| DesktopError::Config(e.to_string()))?;
    if cfg.device_id.trim().is_empty() {
        cfg.device_id = uuid::Uuid::now_v7().to_string();
    }
    if cfg.device_name.trim().is_empty() {
        cfg.device_name = default_device_name();
    }
    if cfg.port == 0 {
        cfg.port = DEFAULT_PORT;
    }
    Ok(cfg)
}

pub fn save_remote_config(cfg: &RemoteAccessConfig) -> DesktopResult<()> {
    let path = config_path()?;
    let raw = serde_json::to_string_pretty(cfg).map_err(|e| DesktopError::Config(e.to_string()))?;
    fs::write(&path, raw).map_err(|e| DesktopError::Config(e.to_string()))
}

/// Load the remote bearer token from the encrypted secrets store, or `None`.
pub fn load_remote_token() -> DesktopResult<Option<String>> {
    let dir = secrets_dir()?;
    let all = SecretsStore::load_all(&dir, &[])?;
    Ok(all.get(REMOTE_TOKEN_SECRET_KEY).cloned())
}

/// Ensure a remote token exists; generate + persist one if missing. Returns the token.
pub fn ensure_remote_token() -> DesktopResult<String> {
    if let Some(existing) = load_remote_token()? {
        if !existing.is_empty() {
            return Ok(existing);
        }
    }
    rotate_remote_token()
}

/// Generate a new token and persist it, replacing any prior value.
pub fn rotate_remote_token() -> DesktopResult<String> {
    let token = super::auth::AuthToken::generate().as_str().to_owned();
    let dir = secrets_dir()?;
    let mut all = SecretsStore::load_all(&dir, &[])?;
    all.insert(REMOTE_TOKEN_SECRET_KEY.to_owned(), token.clone());
    SecretsStore::save_all(&dir, &all)?;
    Ok(token)
}
