//! CLI preferences persisted under the XDG config directory.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use agentloop_contracts::{ModelInfo, ModelRef};
use agentloop_engine::{CustomProviderSpec, RoleRegistry, RoleSpec, RoleToolProfile, valid_name};
use serde::{Deserialize, Serialize};

use crate::catalog::CatalogEntry;

/// On-disk CLI preferences.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CliPrefs {
    /// Last successfully selected model (`provider/model` or bare id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_model: Option<String>,
    /// Requested extended-thinking budget in tokens; `None` = thinking off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<u32>,
    /// Whether thinking output is rendered in the transcript.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_visible: Option<bool>,
    /// Session mode default: `"code"` or `"plan"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_mode: Option<String>,
    /// Permission mode default: `"default"`, `"accept-edits"`, `"plan"`,
    /// `"dont-ask"`, or `"bypass"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    /// Custom OpenAI-compatible providers keyed by provider id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub providers: BTreeMap<String, ProviderConfig>,
    /// Orchestration roles for the Task tool, keyed by role name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub roles: BTreeMap<String, RoleConfig>,
    /// Models to fall back to (in order) when the active model's provider
    /// fails mid-turn (`provider/model` refs).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallback_models: Vec<String>,
    /// Forward-compat catch-all: preserves keys this build doesn't know
    /// across load-mutate-save cycles.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// One user-configured OpenAI-compatible provider.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Display name; the id is used when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// API base URL (e.g. `https://api.deepseek.com/v1`).
    pub base_url: String,
    /// Literal key or a `{env:VAR}` reference resolved at load time.
    pub api_key: String,
    /// Static model catalog; served without a network call when non-empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<ModelEntry>,
    /// Model used when none is qualified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    /// Whether the endpoint accepts extended-thinking config
    /// (DeepSeek-style `thinking` request field).
    #[serde(default)]
    pub thinking: bool,
}

/// One user-configured orchestration role. Unset fields inherit the
/// same-name built-in role's values (or conservative defaults for new roles).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleConfig {
    /// Ordered model preference chain (`provider/model` refs).
    /// Empty = inherit the spawning session's effective model.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
    /// `"read-only"`, `"full"`, or a comma-separated tool allow-list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<String>,
    /// System-prompt addition for subagents of this role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Distribute parallel spawns across the chain (round-robin).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<bool>,
    /// Concurrent subagents of this role per batch (engine clamps 1..=8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_parallel: Option<usize>,
    /// Spawn-tree depth below this role (engine clamps 0..=3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u8>,
}

/// One model in a custom provider's static catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelEntry {
    /// Model id as sent to the API.
    pub id: String,
    /// Display name; the id is used when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Context window in tokens, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
}

/// Failure reading or writing CLI preferences.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PrefsError {
    /// Neither `XDG_CONFIG_HOME` nor `HOME` is set.
    #[error("cannot locate config directory: set XDG_CONFIG_HOME or HOME")]
    NoConfigDir,
    /// An I/O or serialization error.
    #[error("{context}: {message}")]
    Io {
        /// What was being attempted (e.g. "read config").
        context: &'static str,
        /// Underlying error text.
        message: String,
    },
    /// An `{env:VAR}` reference points at an unset or empty variable.
    #[error("environment variable `{0}` is unset or empty")]
    MissingEnv(String),
}

/// Resolve an api-key value: `{env:VAR}` reads the variable, anything else
/// is returned verbatim.
pub fn resolve_api_key(raw: &str) -> Result<String, PrefsError> {
    let trimmed = raw.trim();
    let Some(var) = trimmed
        .strip_prefix("{env:")
        .and_then(|rest| rest.strip_suffix('}'))
    else {
        return Ok(trimmed.to_owned());
    };
    match std::env::var(var) {
        Ok(value) if !value.trim().is_empty() => Ok(value.trim().to_owned()),
        _ => Err(PrefsError::MissingEnv(var.to_owned())),
    }
}

/// Translate configured providers into engine specs. Entries whose key
/// cannot be resolved are skipped and reported as `(id, reason)` so callers
/// can surface them in the resolution trace instead of failing startup.
pub fn custom_specs(prefs: &CliPrefs) -> (Vec<CustomProviderSpec>, Vec<(String, String)>) {
    let mut specs = Vec::new();
    let mut skipped = Vec::new();
    for (id, config) in &prefs.providers {
        match resolve_api_key(&config.api_key) {
            Ok(api_key) => specs.push(CustomProviderSpec {
                id: id.clone(),
                base_url: config.base_url.clone(),
                api_key,
                default_model: config.default_model.clone(),
                models: config.models.iter().map(model_entry_info).collect(),
                thinking: config.thinking,
            }),
            Err(err) => skipped.push((id.clone(), err.to_string())),
        }
    }
    (specs, skipped)
}

/// Translate configured roles into engine specs. Invalid entries are skipped
/// and reported as `(name, reason)` so callers can surface them in the
/// resolution trace instead of failing engine startup (one bad role reaching
/// `RoleRegistry::with_defaults` would fail the whole engine build).
pub fn role_specs(prefs: &CliPrefs) -> (Vec<RoleSpec>, Vec<(String, String)>) {
    let builtins = RoleRegistry::with_defaults(Vec::new()).ok();
    let mut specs = Vec::new();
    let mut skipped = Vec::new();
    for (name, config) in &prefs.roles {
        if !valid_name(name) {
            skipped.push((
                name.clone(),
                "invalid name (use a-z, 0-9, -, _; max 32 chars)".to_owned(),
            ));
            continue;
        }
        // Seed from the same-name built-in so a partial override (e.g. worker
        // with only `models`) keeps its full tool access and built-in prompt —
        // the engine registry replaces same-name built-ins wholesale.
        let mut spec = builtins
            .as_ref()
            .and_then(|registry| registry.get(name).cloned())
            .unwrap_or_else(|| RoleSpec::new(name.clone()));
        if !config.models.is_empty() {
            spec.models = config
                .models
                .iter()
                .map(|model| ModelRef::from(model.as_str()))
                .collect();
        }
        if let Some(raw) = &config.tools {
            match parse_tool_profile(raw) {
                Some(profile) => spec.tools = profile,
                None => {
                    skipped.push((name.clone(), "tools is empty".to_owned()));
                    continue;
                }
            }
        }
        if let Some(prompt) = &config.prompt {
            spec.prompt = Some(prompt.clone());
        }
        if let Some(split) = config.split {
            spec.split = split;
        }
        if let Some(max_parallel) = config.max_parallel {
            spec.max_parallel = max_parallel;
        }
        if let Some(max_depth) = config.max_depth {
            spec.max_depth = max_depth;
        }
        specs.push(spec);
    }
    (specs, skipped)
}

/// `"read-only"`, `"full"`, or a comma-separated tool allow-list.
/// `None` when the value contains no tool names.
fn parse_tool_profile(raw: &str) -> Option<RoleToolProfile> {
    match raw.trim() {
        "read-only" => Some(RoleToolProfile::ReadOnly),
        "full" => Some(RoleToolProfile::Full),
        list => {
            let names: Vec<String> = list
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect();
            (!names.is_empty()).then_some(RoleToolProfile::Allow(names))
        }
    }
}

fn model_entry_info(entry: &ModelEntry) -> ModelInfo {
    ModelInfo {
        id: entry.id.clone(),
        display_name: entry.name.clone(),
        context_window: entry.context_window,
        reasoning: false,
        vision: false,
    }
}

impl CliPrefs {
    /// Load preferences from the default config path, or defaults on missing file.
    pub fn load() -> Self {
        match config_path() {
            Some(path) => Self::load_from(&path).unwrap_or_default(),
            None => Self::default(),
        }
    }

    /// Load preferences from `path`. Missing file yields defaults.
    pub fn load_from(path: &Path) -> Result<Self, PrefsError> {
        let raw = match std::fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(err) => {
                return Err(PrefsError::Io {
                    context: "read config",
                    message: err.to_string(),
                });
            }
        };
        serde_json::from_str(&raw).map_err(|err| PrefsError::Io {
            context: "parse config",
            message: err.to_string(),
        })
    }

    /// Persist preferences to the default config path.
    pub fn save(&self) -> Result<(), PrefsError> {
        let path = config_path().ok_or(PrefsError::NoConfigDir)?;
        Self::save_to(&path, self)
    }

    /// Persist preferences to `path`, creating parent directories as needed.
    pub fn save_to(path: &Path, prefs: &Self) -> Result<(), PrefsError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| PrefsError::Io {
                context: "create config directory",
                message: err.to_string(),
            })?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).map_err(
                    |err| PrefsError::Io {
                        context: "restrict config directory permissions",
                        message: err.to_string(),
                    },
                )?;
            }
        }
        let raw = serde_json::to_string_pretty(prefs).map_err(|err| PrefsError::Io {
            context: "serialize config",
            message: err.to_string(),
        })?;
        std::fs::write(path, raw).map_err(|err| PrefsError::Io {
            context: "write config",
            message: err.to_string(),
        })?;
        #[cfg(unix)]
        if path.exists() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    /// Update and persist the last selected model.
    pub fn remember_model(model: &ModelRef) -> Result<(), PrefsError> {
        let mut prefs = Self::load();
        prefs.last_model = Some(model.0.clone());
        prefs.save()
    }

    /// Update and persist the thinking budget.
    pub fn remember_thinking_budget(budget: Option<u32>) -> Result<(), PrefsError> {
        let mut prefs = Self::load();
        prefs.thinking_budget = budget;
        prefs.save()
    }

    /// Update and persist UI mode defaults.
    pub fn remember_modes(
        session_mode: &str,
        permission_mode: &str,
        thinking_visible: bool,
    ) -> Result<(), PrefsError> {
        let mut prefs = Self::load();
        prefs.session_mode = Some(session_mode.to_owned());
        prefs.permission_mode = Some(permission_mode.to_owned());
        prefs.thinking_visible = Some(thinking_visible);
        prefs.save()
    }

    /// Insert or replace a custom provider and persist.
    pub fn remember_provider(id: &str, config: ProviderConfig) -> Result<(), PrefsError> {
        let mut prefs = Self::load();
        prefs.providers.insert(id.to_owned(), config);
        prefs.save()
    }

    /// Remove a custom provider and persist. Returns whether it existed.
    pub fn forget_provider(id: &str) -> Result<bool, PrefsError> {
        let mut prefs = Self::load();
        let existed = prefs.providers.remove(id).is_some();
        prefs.save()?;
        Ok(existed)
    }
}

/// `~/.config/agentloop` (honoring `XDG_CONFIG_HOME`).
pub fn config_dir() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.trim().is_empty() {
            return Some(PathBuf::from(xdg).join("agentloop"));
        }
    }
    std::env::var("HOME")
        .ok()
        .filter(|home| !home.trim().is_empty())
        .map(|home| PathBuf::from(home).join(".config").join("agentloop"))
}

/// Default preferences file: `{config_dir}/config.json`.
pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("config.json"))
}

/// Whether `model` can be used given registered provider ids.
pub fn model_provider_available(model: &ModelRef, providers: &[String]) -> bool {
    if model.0.trim().is_empty() {
        return false;
    }
    let (provider, _) = model.split();
    match provider {
        Some(name) => providers.iter().any(|id| id == name),
        None => !providers.is_empty(),
    }
}

/// Whether `model` appears in a fetched catalog.
pub fn model_in_catalog(model: &ModelRef, catalog: &[CatalogEntry]) -> bool {
    catalog.iter().any(|entry| entry.model_ref() == *model)
}

/// Resolve a stored model string if it is still valid.
///
/// When `catalog` is non-empty, membership is required. Otherwise the provider
/// must be registered (or any provider must exist for bare refs).
pub fn resolve_stored_model(
    stored: &str,
    providers: &[String],
    catalog: Option<&[CatalogEntry]>,
) -> Option<ModelRef> {
    let model = ModelRef(stored.to_owned());
    if model.0.trim().is_empty() {
        return None;
    }
    if let Some(entries) = catalog.filter(|entries| !entries.is_empty()) {
        return model_in_catalog(&model, entries).then_some(model);
    }
    model_provider_available(&model, providers).then_some(model)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::{ModelInfo, ProviderId};

    fn catalog_entry(provider: &str, model_id: &str) -> CatalogEntry {
        CatalogEntry {
            provider: ProviderId::from(provider),
            model: ModelInfo {
                id: model_id.to_owned(),
                display_name: None,
                context_window: None,
                reasoning: false,
                vision: false,
            },
        }
    }

    #[test]
    fn save_load_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        let prefs = CliPrefs {
            last_model: Some("anthropic/claude-sonnet-4-5".to_owned()),
            ..CliPrefs::default()
        };
        CliPrefs::save_to(&path, &prefs).expect("save");
        let loaded = CliPrefs::load_from(&path).expect("load");
        assert_eq!(loaded, prefs);
    }

    #[test]
    fn load_missing_file_returns_defaults() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("missing.json");
        assert_eq!(
            CliPrefs::load_from(&path).expect("load"),
            CliPrefs::default()
        );
    }

    #[test]
    fn resolve_falls_back_when_provider_unavailable() {
        let stored = "missing-provider/some-model";
        let providers = vec!["anthropic".to_owned()];
        assert_eq!(resolve_stored_model(stored, &providers, None), None);
    }

    #[test]
    fn resolve_accepts_provider_when_registered() {
        let stored = "anthropic/claude-sonnet-4-5";
        let providers = vec!["anthropic".to_owned(), "copilot".to_owned()];
        assert_eq!(
            resolve_stored_model(stored, &providers, None),
            Some(ModelRef::from(stored))
        );
    }

    #[test]
    fn resolve_requires_catalog_membership_when_catalog_present() {
        let stored = "anthropic/claude-sonnet-4-5";
        let providers = vec!["anthropic".to_owned()];
        let catalog = vec![catalog_entry("copilot", "gpt-4.1")];
        assert_eq!(
            resolve_stored_model(stored, &providers, Some(&catalog)),
            None
        );
        let catalog = vec![catalog_entry("anthropic", "claude-sonnet-4-5")];
        assert_eq!(
            resolve_stored_model(stored, &providers, Some(&catalog)),
            Some(ModelRef::from(stored))
        );
    }

    #[test]
    fn resolve_rejects_empty_model_id() {
        assert_eq!(
            resolve_stored_model("  ", &["anthropic".to_owned()], None),
            None
        );
    }

    #[test]
    fn providers_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        let mut prefs = CliPrefs::default();
        prefs.providers.insert(
            "deepseek".to_owned(),
            ProviderConfig {
                name: Some("DeepSeek".to_owned()),
                base_url: "https://api.deepseek.com/v1".to_owned(),
                api_key: "{env:DEEPSEEK_API_KEY}".to_owned(),
                models: vec![ModelEntry {
                    id: "deepseek-chat".to_owned(),
                    name: None,
                    context_window: Some(64_000),
                }],
                default_model: Some("deepseek-chat".to_owned()),
                thinking: true,
            },
        );
        prefs.thinking_budget = Some(8192);
        CliPrefs::save_to(&path, &prefs).expect("save");
        let loaded = CliPrefs::load_from(&path).expect("load");
        assert_eq!(loaded, prefs);
    }

    #[test]
    fn unknown_top_level_keys_survive_load_mutate_save() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"last_model":"a/b","future_setting":{"nested":true}}"#,
        )
        .expect("write");
        let mut prefs = CliPrefs::load_from(&path).expect("load");
        prefs.last_model = Some("c/d".to_owned());
        CliPrefs::save_to(&path, &prefs).expect("save");
        let raw = std::fs::read_to_string(&path).expect("read");
        let value: serde_json::Value = serde_json::from_str(&raw).expect("json");
        assert_eq!(value["future_setting"]["nested"], serde_json::json!(true));
        assert_eq!(value["last_model"], serde_json::json!("c/d"));
    }

    #[test]
    fn resolve_api_key_literal_and_env() {
        assert_eq!(resolve_api_key(" sk-literal ").expect("ok"), "sk-literal");
        temp_env::with_var("PREFS_TEST_KEY", Some("sk-env"), || {
            assert_eq!(
                resolve_api_key("{env:PREFS_TEST_KEY}").expect("ok"),
                "sk-env"
            );
        });
        temp_env::with_var_unset("PREFS_TEST_MISSING", || {
            assert!(matches!(
                resolve_api_key("{env:PREFS_TEST_MISSING}"),
                Err(PrefsError::MissingEnv(var)) if var == "PREFS_TEST_MISSING"
            ));
        });
    }

    #[test]
    fn roles_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        let mut prefs = CliPrefs::default();
        prefs.roles.insert(
            "senior".to_owned(),
            RoleConfig {
                models: vec!["anthropic/claude-opus-4-5".to_owned()],
                tools: Some("full".to_owned()),
                prompt: Some("Take the hardest subtasks.".to_owned()),
                split: Some(false),
                max_parallel: Some(2),
                max_depth: Some(0),
            },
        );
        CliPrefs::save_to(&path, &prefs).expect("save");
        let loaded = CliPrefs::load_from(&path).expect("load");
        assert_eq!(loaded, prefs);
    }

    #[test]
    fn role_specs_maps_fields() {
        let mut prefs = CliPrefs::default();
        prefs.roles.insert(
            "senior".to_owned(),
            RoleConfig {
                models: vec!["a/x".to_owned(), "b/y".to_owned()],
                tools: Some("Read, Grep".to_owned()),
                prompt: Some("p".to_owned()),
                split: Some(false),
                max_parallel: Some(2),
                max_depth: Some(0),
            },
        );
        let (specs, skipped) = role_specs(&prefs);
        assert!(skipped.is_empty());
        assert_eq!(specs.len(), 1);
        let spec = &specs[0];
        assert_eq!(spec.name, "senior");
        assert_eq!(
            spec.models,
            vec![ModelRef::from("a/x"), ModelRef::from("b/y")]
        );
        assert_eq!(
            spec.tools,
            RoleToolProfile::Allow(vec!["Read".to_owned(), "Grep".to_owned()])
        );
        assert_eq!(spec.prompt.as_deref(), Some("p"));
        assert!(!spec.split);
        assert_eq!(spec.max_parallel, 2);
        assert_eq!(spec.max_depth, 0);

        prefs.roles.get_mut("senior").expect("entry").tools = Some("read-only".to_owned());
        let (specs, _) = role_specs(&prefs);
        assert_eq!(specs[0].tools, RoleToolProfile::ReadOnly);
        prefs.roles.get_mut("senior").expect("entry").tools = Some("full".to_owned());
        let (specs, _) = role_specs(&prefs);
        assert_eq!(specs[0].tools, RoleToolProfile::Full);
    }

    #[test]
    fn role_specs_reports_invalid_entries() {
        let mut prefs = CliPrefs::default();
        prefs.roles.insert("good".to_owned(), RoleConfig::default());
        prefs
            .roles
            .insert("Bad Name!".to_owned(), RoleConfig::default());
        prefs.roles.insert(
            "empty-tools".to_owned(),
            RoleConfig {
                tools: Some(" , ".to_owned()),
                ..RoleConfig::default()
            },
        );
        let (specs, skipped) = role_specs(&prefs);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "good");
        assert_eq!(skipped.len(), 2);
        assert!(skipped.iter().any(|(name, _)| name == "Bad Name!"));
        assert!(skipped.iter().any(|(name, _)| name == "empty-tools"));
    }

    #[test]
    fn role_specs_builtin_override_inherits_unset_fields() {
        let mut prefs = CliPrefs::default();
        prefs.roles.insert(
            "worker".to_owned(),
            RoleConfig {
                models: vec!["deepseek/deepseek-chat".to_owned()],
                ..RoleConfig::default()
            },
        );
        let (specs, skipped) = role_specs(&prefs);
        assert!(skipped.is_empty());
        let worker = &specs[0];
        // A models-only override must keep the built-in worker's full tool
        // access and prompt, not fall back to read-only/no-prompt defaults.
        assert_eq!(worker.tools, RoleToolProfile::Full);
        assert!(worker.prompt.is_some());
        assert_eq!(
            worker.models,
            vec![ModelRef::from("deepseek/deepseek-chat")]
        );
    }

    #[test]
    fn role_specs_passes_unknown_providers_through() {
        let mut prefs = CliPrefs::default();
        prefs.roles.insert(
            "searcher".to_owned(),
            RoleConfig {
                models: vec!["notloaded/foo".to_owned()],
                ..RoleConfig::default()
            },
        );
        let (specs, skipped) = role_specs(&prefs);
        assert!(skipped.is_empty());
        assert_eq!(specs[0].models, vec![ModelRef::from("notloaded/foo")]);
    }

    #[test]
    fn custom_specs_skips_unresolvable_entries() {
        let mut prefs = CliPrefs::default();
        prefs.providers.insert(
            "good".to_owned(),
            ProviderConfig {
                base_url: "https://api.example.com/v1".to_owned(),
                api_key: "sk-ok".to_owned(),
                ..ProviderConfig::default()
            },
        );
        prefs.providers.insert(
            "bad".to_owned(),
            ProviderConfig {
                base_url: "https://api.example.com/v1".to_owned(),
                api_key: "{env:PREFS_TEST_UNSET_VAR}".to_owned(),
                ..ProviderConfig::default()
            },
        );
        temp_env::with_var_unset("PREFS_TEST_UNSET_VAR", || {
            let (specs, skipped) = custom_specs(&prefs);
            assert_eq!(specs.len(), 1);
            assert_eq!(specs[0].id, "good");
            assert_eq!(skipped.len(), 1);
            assert_eq!(skipped[0].0, "bad");
        });
    }
}
