
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use keyring::Entry;
use serde::{Deserialize, Serialize};

use crate::error::{DesktopError, DesktopResult};
use crate::secrets::{resolve_mode, set_configured_mode, SecretStorageMode, SecretsStore};

const SERVICE: &str = "agentloop.desktop";
const KEYS_ACCOUNT: &str = "provider_keys";
const PROFILE_KEYS_ACCOUNT: &str = "profile_keys";
const LEGACY_KEY_PREFIX: &str = "legacy:";
pub const MCP_SECRET_PREFIX: &str = "mcp:";
const MCP_ARGS_SUFFIX_META: &str = "__args_suffix__";
const DEFAULT_PROFILE_ID: &str = "default";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPrefs {
    #[serde(default = "default_true")]
    pub search: bool,
    #[serde(default)]
    pub index: bool,
    #[serde(default)]
    pub auto_context: bool,
    #[serde(default)]
    pub auto_update_index: bool,
    #[serde(default)]
    pub learning: bool,
    #[serde(default)]
    pub learning_require_human_approval: bool,
    #[serde(default)]
    pub learning_require_verified_memory: bool,
    #[serde(default)]
    pub verifier: bool,
    #[serde(default)]
    pub browser: bool,
    #[serde(default)]
    pub computer: bool,

    #[serde(default = "default_true")]
    pub artifacts: bool,

    #[serde(default)]
    pub messaging: bool,
    #[serde(default)]
    pub council: bool,
    #[serde(default)]
    pub auto_mode: bool,
    #[serde(default)]
    pub auto_mode_router_model: Option<String>,
    #[serde(default = "default_true")]
    pub auto_compact: bool,
    #[serde(default = "default_auto_compact_threshold")]
    pub auto_compact_threshold_percent: u8,
    #[serde(default = "default_compaction_mode")]
    pub compaction_mode: String,
    #[serde(default = "default_mode_switch_veto_ms")]
    pub mode_switch_veto_ms: u32,
    #[serde(default)]
    pub delegation_rules: String,

    #[serde(default = "default_cost_mode")]
    pub cost_mode: String,
    #[serde(default = "default_cost_models_low")]
    pub cost_models_low: Vec<String>,
    #[serde(default = "default_cost_models_medium")]
    pub cost_models_medium: Vec<String>,
    #[serde(default = "default_cost_models_high")]
    pub cost_models_high: Vec<String>,
}

fn default_cost_mode() -> String {
    "auto".to_owned()
}

fn default_cost_models_low() -> Vec<String> {
    vec![
        "anthropic/claude-haiku-4-5".to_owned(),
        "openai/gpt-4.1-mini".to_owned(),
        "deepseek/deepseek-v4-flash".to_owned(),
        "gemini/gemini-2.0-flash".to_owned(),
    ]
}

fn default_cost_models_medium() -> Vec<String> {
    vec![
        "anthropic/claude-sonnet-4-5".to_owned(),
        "openai/gpt-4.1".to_owned(),
        "deepseek/deepseek-v4-pro".to_owned(),
        "gemini/gemini-2.5-pro".to_owned(),
    ]
}

fn default_cost_models_high() -> Vec<String> {
    vec![
        "anthropic/claude-opus-4-5".to_owned(),
        "openai/o3".to_owned(),
        "openai/o1".to_owned(),
    ]
}

fn default_true() -> bool {
    true
}

fn default_auto_compact_threshold() -> u8 {
    85
}

fn default_compaction_mode() -> String {
    "standard".to_owned()
}

fn default_mode_switch_veto_ms() -> u32 {
    2000
}

impl Default for PluginPrefs {
    fn default() -> Self {
        Self {
            search: true,
            index: false,
            auto_context: false,
            auto_update_index: false,
            learning: false,
            learning_require_human_approval: false,
            learning_require_verified_memory: false,
            verifier: false,
            browser: false,
            computer: false,
            artifacts: true,
            messaging: false,
            council: false,
            auto_mode: false,
            auto_mode_router_model: None,
            auto_compact: true,
            auto_compact_threshold_percent: 85,
            compaction_mode: "standard".to_owned(),
            mode_switch_veto_ms: 2000,
            delegation_rules: String::new(),
            cost_mode: default_cost_mode(),
            cost_models_low: default_cost_models_low(),
            cost_models_medium: default_cost_models_medium(),
            cost_models_high: default_cost_models_high(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct InlineCompletionPrefs {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub setup_dismissed: bool,
}

pub fn normalize_inline_model_id(provider_id: &str, model_id: &str) -> String {
    let provider_id = provider_id.trim();
    let model_id = model_id.trim();
    let prefix = format!("{provider_id}/");
    model_id
        .strip_prefix(&prefix)
        .unwrap_or(model_id)
        .to_string()
}

impl InlineCompletionPrefs {
    pub fn is_configured(&self) -> bool {
        self.provider_id.as_deref().is_some_and(|p| !p.is_empty())
            && self.model_id.as_deref().is_some_and(|m| !m.is_empty())
    }

    pub fn model_ref(&self) -> Option<String> {
        let provider = self.provider_id.as_deref()?.trim();
        let model = self.model_id.as_deref()?.trim();
        if provider.is_empty() || model.is_empty() {
            return None;
        }
        let model = normalize_inline_model_id(provider, model);
        Some(format!("{provider}/{model}"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfile {
    pub id: String,
    pub label: String,
    pub provider: String,
    pub base_url: Option<String>,
    pub region: Option<String>,
    pub default_model: Option<String>,
    pub fallback_models: Option<String>,
    pub default_isolation: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPrefs {
    pub preferred_provider: Option<String>,
    pub base_url: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    pub default_model: Option<String>,
    pub cwd: Option<String>,
    #[serde(default)]
    pub plugins: PluginPrefs,
    #[serde(default)]
    pub fallback_models: Vec<String>,
    #[serde(default)]
    pub default_isolation: Option<String>,
    #[serde(default)]
    pub max_workspaces_per_project: Option<u32>,
    #[serde(default)]
    pub profiles: Vec<ProviderProfile>,
    #[serde(default)]
    pub active_profile_id: Option<String>,
    #[serde(default)]
    pub secret_storage: Option<String>,
    #[serde(default)]
    pub inline_completion: InlineCompletionPrefs,
}

#[derive(Debug, Clone, Default)]
pub struct ProviderConfig {
    pub prefs: ProviderPrefs,
    pub keys: BTreeMap<String, String>,
    pub profile_keys: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigView {
    pub preferred_provider: Option<String>,
    pub base_url: Option<String>,
    pub region: Option<String>,
    pub default_model: Option<String>,
    pub cwd: Option<String>,
    pub configured_providers: Vec<String>,
    pub has_any_key: bool,
    pub plugins: PluginPrefs,
    pub fallback_models: Vec<String>,
    pub default_isolation: Option<String>,
    pub secret_storage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProviderConfigInput {
    pub preferred_provider: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub region: Option<String>,
    pub default_model: Option<String>,
    pub cwd: Option<String>,
    pub plugins: Option<PluginPrefs>,
    pub fallback_models: Option<Vec<String>>,
    pub default_isolation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfileView {
    pub id: String,
    pub label: String,
    pub provider: String,
    pub base_url: Option<String>,
    pub region: Option<String>,
    pub default_model: Option<String>,
    pub fallback_models: Option<String>,
    pub default_isolation: Option<String>,
    pub has_key: bool,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfileInput {
    #[serde(default)]
    pub id: Option<String>,
    pub label: String,
    pub provider: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub region: Option<String>,
    pub default_model: Option<String>,
    pub fallback_models: Option<String>,
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
    let raw =
        serde_json::to_string_pretty(prefs).map_err(|e| DesktopError::Config(e.to_string()))?;
    fs::write(&path, raw).map_err(|e| DesktopError::Config(e.to_string()))
}

fn strip_legacy_prefix(secrets: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    secrets
        .iter()
        .filter_map(|(k, v)| {
            k.strip_prefix(LEGACY_KEY_PREFIX)
                .map(|id| (id.to_owned(), v.clone()))
        })
        .collect()
}

fn migrate_legacy_provider_keys_blob_into(secrets: &mut BTreeMap<String, String>) {
    let entry = match Entry::new(SERVICE, KEYS_ACCOUNT) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, "migration: failed to open legacy provider_keys keychain entry");
            return;
        }
    };
    let raw = match entry.get_password() {
        Ok(raw) => raw,
        Err(keyring::Error::NoEntry) => return,
        Err(e) => {
            tracing::warn!(error = %e, "migration: failed to read legacy provider_keys keychain entry");
            return;
        }
    };
    let legacy: BTreeMap<String, String> = match serde_json::from_str(&raw) {
        Ok(map) => map,
        Err(e) => {
            tracing::warn!(error = %e, "migration: legacy provider_keys blob is corrupt, skipping");
            return;
        }
    };
    for (id, key) in legacy {
        let namespaced = format!("{LEGACY_KEY_PREFIX}{id}");
        secrets.entry(namespaced).or_insert(key);
    }
    if let Err(e) = entry.delete_credential() {
        tracing::warn!(error = %e, "migration: failed to delete legacy provider_keys keychain entry");
    } else {
        tracing::info!("migrated legacy provider_keys keychain entry to encrypted secrets store");
    }
}

fn secrets_dir() -> DesktopResult<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| DesktopError::Config("no config directory".into()))?
        .join("agentloop")
        .join("desktop");
    fs::create_dir_all(&dir).map_err(|e| DesktopError::Config(e.to_string()))?;
    Ok(dir)
}

fn migrate_legacy_profile_keys_blob_into(secrets: &mut BTreeMap<String, String>) {
    let entry = match Entry::new(SERVICE, PROFILE_KEYS_ACCOUNT) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, "migration: failed to open legacy profile_keys keychain entry");
            return;
        }
    };
    let raw = match entry.get_password() {
        Ok(raw) => raw,
        Err(keyring::Error::NoEntry) => return,
        Err(e) => {
            tracing::warn!(error = %e, "migration: failed to read legacy profile_keys keychain entry");
            return;
        }
    };
    let legacy: BTreeMap<String, String> = match serde_json::from_str(&raw) {
        Ok(map) => map,
        Err(e) => {
            tracing::warn!(error = %e, "migration: legacy profile_keys blob is corrupt, skipping");
            return;
        }
    };
    for (id, key) in legacy {
        secrets.entry(id).or_insert(key);
    }
    if let Err(e) = entry.delete_credential() {
        tracing::warn!(error = %e, "migration: failed to delete legacy profile_keys keychain entry");
    } else {
        tracing::info!("migrated legacy profile_keys keychain entry to encrypted secrets store");
    }
}

fn provider_display_label(id: &str) -> String {
    match id {
        "anthropic" => "Anthropic",
        "openai" => "OpenAI",
        "gemini" => "Google Gemini",
        "deepseek" => "DeepSeek",
        "openrouter" => "OpenRouter",
        "groq" => "Groq",
        "mistral" => "Mistral",
        "xai" => "xAI",
        "ollama" => "Ollama",
        "bedrock" => "Amazon Bedrock",
        "copilot" => "GitHub Copilot",
        "chatgpt" => "ChatGPT",
        other => other,
    }
    .to_owned()
}

fn load_combined_secrets() -> DesktopResult<BTreeMap<String, String>> {
    let dir = secrets_dir()?;
    let mut secrets = SecretsStore::load_all(&dir, &[])?;
    let marker = legacy_migration_marker_path(&dir);
    if !marker.exists() {
        let before = secrets.clone();
        migrate_legacy_provider_keys_blob_into(&mut secrets);
        migrate_legacy_profile_keys_blob_into(&mut secrets);
        if secrets != before {
            SecretsStore::save_all(&dir, &secrets)?;
        }
        let _ = fs::write(&marker, b"");
    }
    Ok(secrets)
}

fn legacy_migration_marker_path(dir: &std::path::Path) -> PathBuf {
    dir.join(".legacy_keys_migrated")
}

fn strip_profile_keys(secrets: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    secrets
        .iter()
        .filter(|(k, _)| !k.starts_with(LEGACY_KEY_PREFIX) && !k.starts_with(MCP_SECRET_PREFIX))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

fn merge_combined_secrets(
    keys: &BTreeMap<String, String>,
    profile_keys: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut combined: BTreeMap<String, String> = profile_keys.clone();
    for (id, key) in keys {
        combined.insert(format!("{LEGACY_KEY_PREFIX}{id}"), key.clone());
    }
    combined
}

fn preserve_mcp_secrets(
    combined: &mut BTreeMap<String, String>,
    existing: &BTreeMap<String, String>,
) {
    for (k, v) in existing {
        if k.starts_with(MCP_SECRET_PREFIX) {
            combined.insert(k.clone(), v.clone());
        }
    }
}

fn mcp_server_prefix(server_id: &str) -> String {
    format!("{MCP_SECRET_PREFIX}{server_id}:")
}

fn mcp_env_secret_key(server_id: &str, env_name: &str) -> String {
    format!("{}{env_name}", mcp_server_prefix(server_id))
}

fn mcp_args_suffix_key(server_id: &str) -> String {
    format!("{}{MCP_ARGS_SUFFIX_META}", mcp_server_prefix(server_id))
}

pub fn is_likely_secret_env_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    if upper.ends_with("_TEAM_ID")
        || upper.ends_with("_CHANNEL_IDS")
        || upper.ends_with("_CHANNEL_ID")
    {
        return false;
    }
    upper.contains("TOKEN")
        || upper.contains("SECRET")
        || upper.contains("PASSWORD")
        || upper.contains("PASSWD")
        || upper.contains("API_KEY")
        || upper.ends_with("_KEY")
        || upper.contains("ACCESS_KEY")
        || upper.contains("PRIVATE_KEY")
        || upper.contains("AUTH")
}

pub fn load_mcp_server_secrets(
    server_id: &str,
) -> DesktopResult<(BTreeMap<String, String>, Vec<String>)> {
    let dir = secrets_dir()?;
    let all = SecretsStore::load_all(&dir, &[])?;
    let prefix = mcp_server_prefix(server_id);
    let mut env = BTreeMap::new();
    let mut args_suffix = Vec::new();
    for (k, v) in all {
        let Some(rest) = k.strip_prefix(&prefix) else {
            continue;
        };
        if rest == MCP_ARGS_SUFFIX_META {
            args_suffix = serde_json::from_str(v.as_str()).unwrap_or_default();
            continue;
        }
        if rest.is_empty() || rest.contains(':') {
            continue;
        }
        env.insert(rest.to_owned(), v);
    }
    Ok((env, args_suffix))
}

pub fn list_mcp_configured_secret_env(server_id: &str) -> DesktopResult<Vec<String>> {
    let (env, _) = load_mcp_server_secrets(server_id)?;
    Ok(env.into_keys().collect())
}

pub fn mcp_has_secret_args_suffix(server_id: &str) -> DesktopResult<bool> {
    let (_, suffix) = load_mcp_server_secrets(server_id)?;
    Ok(!suffix.is_empty())
}

pub fn upsert_mcp_server_secrets(
    server_id: &str,
    secret_env: &BTreeMap<String, String>,
    replace_env: bool,
    args_suffix: Option<&[String]>,
) -> DesktopResult<()> {
    let dir = secrets_dir()?;
    let mut all = SecretsStore::load_all(&dir, &[])?;
    let prefix = mcp_server_prefix(server_id);

    if replace_env {
        let stale: Vec<String> = all
            .keys()
            .filter(|k| {
                k.strip_prefix(&prefix).is_some_and(|rest| {
                    rest != MCP_ARGS_SUFFIX_META && !rest.is_empty() && !rest.contains(':')
                })
            })
            .cloned()
            .collect();
        for k in stale {
            let rest = k.strip_prefix(&prefix).unwrap_or("");
            if secret_env.contains_key(rest) {
                continue;
            }
            all.remove(&k);
        }
    }

    for (name, value) in secret_env {
        let key = mcp_env_secret_key(server_id, name);
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        all.insert(key, trimmed.to_owned());
    }

    if let Some(suffix) = args_suffix {
        let key = mcp_args_suffix_key(server_id);
        if suffix.is_empty() {
            all.remove(&key);
        } else {
            let encoded =
                serde_json::to_string(suffix).map_err(|e| DesktopError::Config(e.to_string()))?;
            all.insert(key, encoded);
        }
    }

    SecretsStore::save_all(&dir, &all)?;
    Ok(())
}

pub fn clear_mcp_server_secrets(server_id: &str) -> DesktopResult<()> {
    let dir = secrets_dir()?;
    let mut all = SecretsStore::load_all(&dir, &[])?;
    let prefix = mcp_server_prefix(server_id);
    let stale: Vec<String> = all
        .keys()
        .filter(|k| k.starts_with(&prefix))
        .cloned()
        .collect();
    if stale.is_empty() {
        return Ok(());
    }
    for k in stale {
        all.remove(&k);
    }
    SecretsStore::save_all(&dir, &all)?;
    Ok(())
}

pub fn load_config() -> DesktopResult<ProviderConfig> {
    let prefs = load_prefs()?;
    let mode = resolve_mode(prefs.secret_storage.as_deref());
    set_configured_mode(mode);

    let secrets = load_combined_secrets()?;
    let mut cfg = ProviderConfig {
        prefs,
        keys: strip_legacy_prefix(&secrets),
        profile_keys: strip_profile_keys(&secrets),
    };
    cfg.migrate_legacy_to_profile();
    Ok(cfg)
}

pub fn persist_config(cfg: &ProviderConfig) -> DesktopResult<()> {
    save_prefs(&cfg.prefs)?;
    let dir = secrets_dir()?;
    let existing = SecretsStore::load_all(&dir, &[])?;
    let mut combined = merge_combined_secrets(&cfg.keys, &cfg.profile_keys);
    preserve_mcp_secrets(&mut combined, &existing);
    SecretsStore::save_all(&dir, &combined)?;
    Ok(())
}

pub fn current_secret_storage_mode(prefs: &ProviderPrefs) -> &'static str {
    resolve_mode(prefs.secret_storage.as_deref()).as_str()
}

pub fn set_secret_storage(
    cfg: &mut ProviderConfig,
    target: SecretStorageMode,
) -> DesktopResult<()> {
    if target == SecretStorageMode::Keychain && !cfg!(target_os = "macos") {
        return Err(DesktopError::Message(
            "Keychain secret storage is only available on macOS".into(),
        ));
    }
    let current = resolve_mode(cfg.prefs.secret_storage.as_deref());
    if current == target {
        cfg.prefs.secret_storage = Some(target.as_str().to_owned());
        save_prefs(&cfg.prefs)?;
        return Ok(());
    }

    let dir = secrets_dir()?;
    SecretsStore::switch_mode(&dir, current, target)?;
    cfg.prefs.secret_storage = Some(target.as_str().to_owned());
    save_prefs(&cfg.prefs)?;
    Ok(())
}

impl ProviderConfig {
    fn migrate_legacy_to_profile(&mut self) {
        if !self.prefs.profiles.is_empty() {
            return;
        }
        let Some(provider) = self.prefs.preferred_provider.clone() else {
            return;
        };
        let profile = ProviderProfile {
            id: DEFAULT_PROFILE_ID.to_owned(),
            label: provider_display_label(&provider),
            provider: provider.clone(),
            base_url: self.prefs.base_url.clone(),
            region: self.prefs.region.clone(),
            default_model: self.prefs.default_model.clone(),
            fallback_models: (!self.prefs.fallback_models.is_empty())
                .then(|| self.prefs.fallback_models.join(", ")),
            default_isolation: self.prefs.default_isolation.clone(),
        };
        self.prefs.profiles.push(profile);
        self.prefs.active_profile_id = Some(DEFAULT_PROFILE_ID.to_owned());
        if let Some(key) = self.keys.get(&provider) {
            self.profile_keys
                .insert(DEFAULT_PROFILE_ID.to_owned(), key.clone());
        }
    }

    pub fn active_profile(&self) -> Option<&ProviderProfile> {
        let id = self.prefs.active_profile_id.as_deref()?;
        self.prefs.profiles.iter().find(|p| p.id == id)
    }

    pub fn active_profile_key(&self) -> Option<&String> {
        let id = self.prefs.active_profile_id.as_deref()?;
        self.profile_keys.get(id)
    }

    pub fn view(&self) -> ProviderConfigView {
        if let Some(profile) = self.active_profile() {
            let oauth_ready = (profile.provider == "copilot"
                && agentloop_sdk::providers::copilot::CopilotConfig::discoverable())
                || (profile.provider == "chatgpt"
                    && agentloop_sdk::providers::chatgpt::ChatgptConfig::discoverable());
            let has_key = self.profile_keys.contains_key(&profile.id) || oauth_ready;
            let configured: Vec<String> = if has_key {
                vec![profile.provider.clone()]
            } else {
                Vec::new()
            };
            return ProviderConfigView {
                preferred_provider: Some(profile.provider.clone()),
                base_url: profile.base_url.clone(),
                region: profile.region.clone(),
                default_model: profile.default_model.clone(),
                cwd: self.prefs.cwd.clone(),
                has_any_key: has_key || profile.provider == "ollama",
                configured_providers: configured,
                plugins: self.prefs.plugins.clone(),
                fallback_models: profile
                    .fallback_models
                    .as_deref()
                    .map(|s| {
                        s.split(',')
                            .map(|m| m.trim().to_owned())
                            .filter(|m| !m.is_empty())
                            .collect()
                    })
                    .unwrap_or_default(),
                default_isolation: profile.default_isolation.clone(),
                secret_storage: current_secret_storage_mode(&self.prefs).to_owned(),
            };
        }
        let configured: Vec<String> = self.keys.keys().cloned().collect();
        ProviderConfigView {
            preferred_provider: self.prefs.preferred_provider.clone(),
            base_url: self.prefs.base_url.clone(),
            region: self.prefs.region.clone(),
            default_model: self.prefs.default_model.clone(),
            cwd: self.prefs.cwd.clone(),
            has_any_key: !configured.is_empty(),
            configured_providers: configured,
            plugins: self.prefs.plugins.clone(),
            fallback_models: self.prefs.fallback_models.clone(),
            default_isolation: self.prefs.default_isolation.clone(),
            secret_storage: current_secret_storage_mode(&self.prefs).to_owned(),
        }
    }

    pub fn is_ready(&self) -> bool {
        if let Some(profile) = self.active_profile() {
            if profile.provider == "ollama" {
                return true;
            }
            if profile.provider == "copilot" {
                return self.profile_keys.contains_key(&profile.id)
                    || agentloop_sdk::providers::copilot::CopilotConfig::discoverable();
            }
            if profile.provider == "chatgpt" {
                return agentloop_sdk::providers::chatgpt::ChatgptConfig::discoverable();
            }
            return self.profile_keys.contains_key(&profile.id);
        }
        let Some(preferred) = self.prefs.preferred_provider.as_deref() else {
            return false;
        };
        if preferred == "ollama" {
            return true;
        }
        if preferred == "copilot" {
            return self.keys.contains_key(preferred)
                || agentloop_sdk::providers::copilot::CopilotConfig::discoverable();
        }
        if preferred == "chatgpt" {
            return agentloop_sdk::providers::chatgpt::ChatgptConfig::discoverable();
        }
        self.keys.contains_key(preferred)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_plugin_defaults_off() {
        let prefs = PluginPrefs::default();
        assert!(
            !prefs.index,
            "IndexPlugin must default off — opt in from Settings or --plugin index"
        );
        assert!(prefs.search);
        assert!(
            !prefs.auto_context,
            "auto-context must default off (opt-in via Settings or AGENTLOOP_AUTO_CONTEXT)"
        );
        assert!(
            !prefs.auto_update_index,
            "auto-update index must default off so warm indexes are reused across chats"
        );
        assert!(!prefs.learning);
        assert!(!prefs.learning_require_human_approval);
        assert!(!prefs.learning_require_verified_memory);
        assert!(!prefs.verifier);
        assert!(
            prefs.artifacts,
            "artifacts office tools must default on so agents can create docx/xlsx/pptx"
        );
    }

    #[test]
    fn index_missing_from_json_defaults_off() {
        let json = r#"{"search":true}"#;
        let prefs: PluginPrefs = serde_json::from_str(json).unwrap();
        assert!(!prefs.index);
        assert!(prefs.search);
    }

    #[test]
    fn coordination_defaults() {
        let prefs = PluginPrefs::default();
        assert!(!prefs.messaging, "messaging must default off");
        assert!(!prefs.council, "council must default off");
        assert!(!prefs.auto_mode, "auto_mode must default off");
        assert!(prefs.auto_mode_router_model.is_none());
        assert!(prefs.auto_compact, "auto_compact must default on");
        assert_eq!(prefs.auto_compact_threshold_percent, 85);
        assert_eq!(prefs.compaction_mode, "standard");
        assert_eq!(prefs.mode_switch_veto_ms, 2000);
        assert!(prefs.delegation_rules.is_empty());
    }

    #[test]
    fn coordination_fields_round_trip_json() {
        let prefs = PluginPrefs {
            messaging: true,
            council: true,
            auto_mode: true,
            auto_mode_router_model: Some("anthropic/claude-sonnet-4-5".into()),
            auto_compact: false,
            auto_compact_threshold_percent: 70,
            compaction_mode: "turn_pair".into(),
            mode_switch_veto_ms: 3000,
            delegation_rules: "use Agent role for sub-tasks".into(),
            ..PluginPrefs::default()
        };
        let json = serde_json::to_string(&prefs).unwrap();
        let back: PluginPrefs = serde_json::from_str(&json).unwrap();
        assert!(back.messaging);
        assert!(back.council);
        assert!(back.auto_mode);
        assert_eq!(
            back.auto_mode_router_model.as_deref(),
            Some("anthropic/claude-sonnet-4-5")
        );
        assert!(!back.auto_compact);
        assert_eq!(back.auto_compact_threshold_percent, 70);
        assert_eq!(back.compaction_mode, "turn_pair");
        assert_eq!(back.mode_switch_veto_ms, 3000);
        assert_eq!(back.delegation_rules, "use Agent role for sub-tasks");
    }

    #[test]
    fn coordination_fields_backward_compat_missing_from_json() {
        let old_json = r#"{"search":true,"index":true,"autoContext":false,"autoUpdateIndex":false,"learning":false,"learningRequireHumanApproval":false,"learningRequireVerifiedMemory":false,"verifier":false,"browser":false,"computer":false}"#;
        let prefs: PluginPrefs = serde_json::from_str(old_json).unwrap();
        assert!(!prefs.messaging);
        assert!(!prefs.council);
        assert!(!prefs.auto_mode);
        assert!(prefs.auto_compact);
        assert_eq!(prefs.auto_compact_threshold_percent, 85);
        assert_eq!(prefs.compaction_mode, "standard");
        assert_eq!(prefs.mode_switch_veto_ms, 2000);
    }

    #[test]
    fn inline_completion_prefs_default_unconfigured() {
        let prefs = InlineCompletionPrefs::default();
        assert!(!prefs.enabled);
        assert!(!prefs.is_configured());
        assert!(prefs.model_ref().is_none());
        let configured = InlineCompletionPrefs {
            enabled: true,
            provider_id: Some("ollama".into()),
            model_id: Some("qwen2.5:0.5b".into()),
            setup_dismissed: false,
        };
        assert!(configured.is_configured());
        assert_eq!(
            configured.model_ref().as_deref(),
            Some("ollama/qwen2.5:0.5b")
        );
        let doubled = InlineCompletionPrefs {
            enabled: true,
            provider_id: Some("ollama".into()),
            model_id: Some("ollama/qwen2.5:0.5b".into()),
            setup_dismissed: false,
        };
        assert_eq!(doubled.model_ref().as_deref(), Some("ollama/qwen2.5:0.5b"));
    }

    #[test]
    fn secret_env_name_heuristic_matches_tokens_not_team_ids() {
        assert!(is_likely_secret_env_name("SLACK_BOT_TOKEN"));
        assert!(is_likely_secret_env_name("GITHUB_PERSONAL_ACCESS_TOKEN"));
        assert!(is_likely_secret_env_name("BRAVE_API_KEY"));
        assert!(is_likely_secret_env_name("OPENAI_API_KEY"));
        assert!(!is_likely_secret_env_name("SLACK_TEAM_ID"));
        assert!(!is_likely_secret_env_name("SLACK_CHANNEL_IDS"));
        assert!(!is_likely_secret_env_name("PATH"));
        assert!(!is_likely_secret_env_name("HOME"));
    }
}
