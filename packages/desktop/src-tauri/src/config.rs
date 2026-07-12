//! Non-secret provider preferences + keychain-backed API keys.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use keyring::Entry;
use serde::{Deserialize, Serialize};

use crate::error::{DesktopError, DesktopResult};
use crate::secrets::{SecretStorageMode, SecretsStore, resolve_mode, set_configured_mode};

const SERVICE: &str = "agentloop.desktop";
/// Keychain account that *used to* hold the legacy single-provider key map
/// (`provider id -> API key`) directly. Every read/write of this Keychain
/// item could prompt the user, and dev rebuilds change the binary's ad-hoc
/// signature so "Always Allow" never sticks — hence constant Keychain
/// prompts. Superseded by [`crate::secrets::SecretsStore`] (one master key
/// read once per process, secrets held in an encrypted local file); this
/// constant is now only used as the source for one-time migration (see
/// `migrate_legacy_provider_keys_blob_into`).
const KEYS_ACCOUNT: &str = "provider_keys";
/// Keychain account that *used to* hold the per-profile key map
/// (`profile id -> API key`) directly. Every read/write of this Keychain
/// item could prompt the user, and dev rebuilds change the binary's ad-hoc
/// signature so "Always Allow" never sticks — hence constant Keychain
/// prompts. Superseded by [`crate::secrets::SecretsStore`] (one master key
/// read once per process, secrets held in an encrypted local file); this
/// constant is now only used as the source for one-time migration (see
/// `migrate_legacy_profile_keys_blob_into`).
const PROFILE_KEYS_ACCOUNT: &str = "profile_keys";
/// Key-id prefix under which legacy single-provider keys (formerly the
/// `KEYS_ACCOUNT` keychain blob) live inside the shared `secrets.enc` store,
/// so they can't collide with profile ids (which are `"default"` or
/// `"profile-<suffix>"` — see `commands::new_profile_id`).
const LEGACY_KEY_PREFIX: &str = "legacy:";
/// Namespace for MCP server secrets (env vars + positional arg values) inside
/// the shared `secrets.enc` store. Format:
/// - env: `mcp:{server_id}:{ENV_NAME}` → value
/// - positional args suffix (JSON array): `mcp:{server_id}:__args_suffix__`
///
/// Kept out of [`ProviderConfig::profile_keys`] so provider persistence can't
/// clobber or mis-attribute them (see [`strip_profile_keys`] /
/// [`persist_config`]).
pub const MCP_SECRET_PREFIX: &str = "mcp:";
/// Meta key (under [`MCP_SECRET_PREFIX`]`{server_id}:`) holding a JSON array
/// of secret positional-arg values appended after the TOML `args` at resolve
/// time (e.g. postgres connection string).
const MCP_ARGS_SUFFIX_META: &str = "__args_suffix__";
/// The synthesized id given to the one profile created by migrating a legacy
/// single-provider config on first load.
const DEFAULT_PROFILE_ID: &str = "default";

/// Which built-in plugins are folded into the engine at composition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPrefs {
    #[serde(default = "default_true")]
    pub search: bool,
    /// `SearchCode`/`FindSymbol`/`RepoMap` code-index tools (`agentloop-index`'s
    /// `IndexPlugin`). Defaults on like `search`: both are read-only,
    /// `needs_permission: Never` tool bundles safe to hand the model by
    /// default. The on-disk index itself never lives in the repo — it's
    /// keyed under the platform app-data dir
    /// (`~/Library/Application Support/agentloop/index/<repo-hash>` on
    /// macOS; `$XDG_DATA_HOME` / `~/.local/share` elsewhere).
    #[serde(default = "default_true")]
    pub index: bool,
    /// When the index plugin is enabled, inject top-k hybrid-search hits
    /// into the first user message of each turn (`AutoContextHook`).
    /// Default **off** — opt in from Settings → Indexing or
    /// `AGENTLOOP_AUTO_CONTEXT=1`.
    #[serde(default)]
    pub auto_context: bool,
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
            index: true,
            auto_context: false,
            learning: false,
            verifier: false,
        }
    }
}

/// A named provider connection ("profile") — e.g. "AWS work" (Bedrock, key A,
/// us-east-1) vs. "AWS personal" (Bedrock, key B, eu-west-1). The API key
/// itself never lives on this struct once persisted: it's stored in the OS
/// keychain keyed by `id` (see [`PROFILE_KEYS_ACCOUNT`]) and threaded through
/// at composition/validation time, mirroring the legacy single-config
/// `ProviderConfig::keys` map.
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

/// Persisted (non-secret) provider preferences on disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPrefs {
    /// Preferred provider id (e.g. `anthropic`, `openai`). Legacy single-config
    /// field: kept for back-compat reads (migrated into a profile on load) but
    /// no longer written once profiles exist.
    pub preferred_provider: Option<String>,
    /// Optional base URL / host override for the preferred provider. Legacy;
    /// see `preferred_provider`.
    pub base_url: Option<String>,
    /// Optional region override for a region-scoped preferred provider
    /// (currently only Amazon Bedrock; e.g. `us-east-1`). Legacy; see
    /// `preferred_provider`.
    #[serde(default)]
    pub region: Option<String>,
    /// Default model id (optionally `provider/`-qualified). Legacy; see
    /// `preferred_provider`.
    pub default_model: Option<String>,
    /// Working directory for new sessions.
    pub cwd: Option<String>,
    /// Built-in plugins enabled at composition.
    #[serde(default)]
    pub plugins: PluginPrefs,
    /// Engine-wide fallback model chain (`provider/model` ids). Legacy; see
    /// `preferred_provider`.
    #[serde(default)]
    pub fallback_models: Vec<String>,
    /// Default isolation for newly created sessions. Legacy; see
    /// `preferred_provider` (profiles carry their own `default_isolation` too,
    /// but this top-level one still governs session creation defaults).
    #[serde(default)]
    pub default_isolation: Option<String>,
    /// Named provider connections. Populated by migrating the legacy fields
    /// above on first load if empty (see `ProviderConfig::migrate`).
    #[serde(default)]
    pub profiles: Vec<ProviderProfile>,
    /// The currently active profile id, or `None` if no profile has been
    /// activated yet.
    #[serde(default)]
    pub active_profile_id: Option<String>,
    /// Explicit secret storage backend choice: `"file"` or `"keychain"`.
    /// `None` means "no explicit choice yet" — resolved by
    /// `secrets::resolve_mode` (new installs default to `file`; existing
    /// installs with a pre-existing Keychain master key stay on
    /// `keychain` so nothing switches silently underneath them). See
    /// `secrets.rs` module docs.
    #[serde(default)]
    pub secret_storage: Option<String>,
}

/// Full runtime config: prefs + secrets loaded from the OS keychain.
#[derive(Debug, Clone, Default)]
pub struct ProviderConfig {
    pub prefs: ProviderPrefs,
    /// Legacy single-config keys: provider id → API key. Still read (for
    /// migration + as the old thin-adapter path) but no longer written once
    /// profiles exist.
    pub keys: BTreeMap<String, String>,
    /// Per-profile keys: profile id → API key.
    pub profile_keys: BTreeMap<String, String>,
}

/// Safe view returned to the frontend (keys masked).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigView {
    pub preferred_provider: Option<String>,
    pub base_url: Option<String>,
    pub region: Option<String>,
    pub default_model: Option<String>,
    pub cwd: Option<String>,
    /// Provider ids that have a stored API key (values never returned).
    pub configured_providers: Vec<String>,
    pub has_any_key: bool,
    pub plugins: PluginPrefs,
    pub fallback_models: Vec<String>,
    pub default_isolation: Option<String>,
    /// Effective secret storage backend (`"file"` | `"keychain"`), resolved
    /// via `secrets::resolve_mode` — see `config::current_secret_storage_mode`.
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

/// Safe view of one profile returned to the frontend (`has_key` only, never
/// the key itself).
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

/// Create/update input for one profile. `api_key: None` (or empty) means
/// "keep the existing stored key" on update; a brand-new profile with no key
/// simply has no stored key (fine for providers like Ollama).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfileInput {
    /// Empty string (or omitted client-side) means "create a new profile" —
    /// the backend mints an id. Present + matching an existing profile means
    /// "update that profile".
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

/// Split the legacy single-provider key map (namespaced by
/// [`LEGACY_KEY_PREFIX`]) out of the combined encrypted-store map, stripping
/// the prefix. Does not touch the Keychain — migration of the old blob
/// happens once, up front, in `load_config`.
fn strip_legacy_prefix(secrets: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    secrets
        .iter()
        .filter_map(|(k, v)| {
            k.strip_prefix(LEGACY_KEY_PREFIX)
                .map(|id| (id.to_owned(), v.clone()))
        })
        .collect()
}

/// One-time migration of the old whole-map Keychain entry (`SERVICE`/
/// `KEYS_ACCOUNT`), analogous to `migrate_legacy_profile_keys_blob_into`
/// below but for the legacy single-provider key map. Reads the old entry,
/// merges any provider ids not already present (under [`LEGACY_KEY_PREFIX`])
/// into `secrets`, and deletes the old Keychain item so it stops prompting
/// on every future launch. Does **not** persist `secrets` itself — the
/// caller ([`load_combined_secrets`]) saves once after running every
/// migration, so two migrations can never race each other's writes.
/// Skipped entirely (no Keychain touch at all) once the old item has
/// already been deleted, since `get_password` on a missing entry is itself
/// cheap and returns `NoEntry` without prompting. Failures are logged,
/// never fatal.
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

/// Directory the encrypted secrets file lives in — the same directory
/// `prefs_path` resolves to, so `secrets.enc` sits next to
/// `provider_prefs.json`.
fn secrets_dir() -> DesktopResult<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| DesktopError::Config("no config directory".into()))?
        .join("agentloop")
        .join("desktop");
    fs::create_dir_all(&dir).map_err(|e| DesktopError::Config(e.to_string()))?;
    Ok(dir)
}

/// One-time migration of the old whole-map Keychain entry (`SERVICE`/
/// `PROFILE_KEYS_ACCOUNT`), which doesn't fit the `SecretsStore::load_all`
/// single-key-id migration shape (it's a *map* under one entry, not one
/// entry per key). Reads the old entry, merges any ids not already present
/// into `secrets` (profile ids live unprefixed, alongside the
/// `LEGACY_KEY_PREFIX`-namespaced provider keys), and deletes the old
/// Keychain item so it stops prompting. Does **not** persist `secrets`
/// itself — see [`migrate_legacy_provider_keys_blob_into`] for why saving is
/// the caller's job. Failures are logged, not fatal.
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

/// Provider display labels for synthesizing a migrated profile's name —
/// mirrors `commands::list_builtin_providers`' labels (kept independent since
/// that list is UI-facing and could grow provider entries this doesn't need).
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
        other => other,
    }
    .to_owned()
}

/// Load every stored secret from the encrypted store, running both
/// whole-map legacy migrations (the old `KEYS_ACCOUNT` and
/// `PROFILE_KEYS_ACCOUNT` Keychain blobs) against a *single* in-memory copy
/// of the combined map before persisting at most once. Splitting the two
/// migrations into separate load/save round-trips would risk one silently
/// clobbering the other's write; doing both against one map and saving once
/// (only if anything actually changed) avoids that and keeps the Keychain
/// touches down to "read+delete each legacy item at most once, ever."
fn load_combined_secrets() -> DesktopResult<BTreeMap<String, String>> {
    let dir = secrets_dir()?;
    let mut secrets = SecretsStore::load_all(&dir, &[])?;
    // Both whole-map migrations delete the legacy Keychain item once they've
    // run, so re-running them is harmless (a `NoEntry` no-op) — but it still
    // costs a Keychain round-trip per launch forever. A marker file next to
    // `secrets.enc` remembers "already migrated" so steady-state launches
    // (the overwhelming common case) skip both migration functions
    // entirely and never touch the Keychain beyond the single master-key
    // read.
    let marker = legacy_migration_marker_path(&dir);
    if !marker.exists() {
        let before = secrets.clone();
        migrate_legacy_provider_keys_blob_into(&mut secrets);
        migrate_legacy_profile_keys_blob_into(&mut secrets);
        if secrets != before {
            SecretsStore::save_all(&dir, &secrets)?;
        }
        // Best-effort: if this write fails we'll just re-attempt (harmless,
        // if slightly slower) migration next launch.
        let _ = fs::write(&marker, b"");
    }
    Ok(secrets)
}

/// Path to the marker file recording that the one-time whole-map legacy
/// Keychain migrations ([`migrate_legacy_provider_keys_blob_into`],
/// [`migrate_legacy_profile_keys_blob_into`]) have already run, so future
/// launches can skip them without any Keychain touch.
fn legacy_migration_marker_path(dir: &std::path::Path) -> PathBuf {
    dir.join(".legacy_keys_migrated")
}

/// Non-legacy (profile-keyed) entries of the combined secrets map: anything
/// *not* namespaced under [`LEGACY_KEY_PREFIX`] or [`MCP_SECRET_PREFIX`].
fn strip_profile_keys(secrets: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    secrets
        .iter()
        .filter(|(k, _)| {
            !k.starts_with(LEGACY_KEY_PREFIX) && !k.starts_with(MCP_SECRET_PREFIX)
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Re-merge the legacy (`keys`) and per-profile (`profile_keys`) maps back
/// into one combined map suitable for `SecretsStore::save_all`, namespacing
/// the legacy side under [`LEGACY_KEY_PREFIX`] so the two id spaces can't
/// collide. MCP secrets are *not* included here — callers that replace the
/// whole store must re-merge them via [`preserve_mcp_secrets`].
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

/// Copy every `mcp:*` entry from `existing` into `combined` so a provider
/// `persist_config` never wipes MCP tokens/connection strings.
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

/// Whether an env var *name* looks like a credential (used to split manual
/// form env lines and to migrate plaintext secrets out of MCP TOML files).
/// Workspace IDs / channel allowlists (`SLACK_TEAM_ID`, `SLACK_CHANNEL_IDS`)
/// intentionally do **not** match.
pub fn is_likely_secret_env_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    if upper.ends_with("_TEAM_ID") || upper.ends_with("_CHANNEL_IDS") || upper.ends_with("_CHANNEL_ID")
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

/// Load every secret belonging to one MCP server: env map + optional
/// positional-args suffix (values appended after the TOML `args` at resolve).
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
            // Unknown meta / nested key — ignore rather than treat as env.
            continue;
        }
        env.insert(rest.to_owned(), v);
    }
    Ok((env, args_suffix))
}

/// Env key names that currently have a stored secret for `server_id`
/// (values never returned to the frontend).
pub fn list_mcp_configured_secret_env(server_id: &str) -> DesktopResult<Vec<String>> {
    let (env, _) = load_mcp_server_secrets(server_id)?;
    Ok(env.into_keys().collect())
}

/// Whether this server has any stored secret positional-arg suffix.
pub fn mcp_has_secret_args_suffix(server_id: &str) -> DesktopResult<bool> {
    let (_, suffix) = load_mcp_server_secrets(server_id)?;
    Ok(!suffix.is_empty())
}

/// Upsert MCP secrets for one server.
///
/// - `secret_env`: non-empty values overwrite; empty/missing keys that already
///   exist are **kept** (mirrors provider profile `api_key` semantics). Pass
///   `replace_env: true` to drop any previously stored env keys not present
///   in `secret_env` (used when the caller sends the full desired set).
/// - `args_suffix`: `Some(vec)` replaces the suffix; `None` keeps existing;
///   `Some(empty)` clears it.
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
                k.strip_prefix(&prefix)
                    .is_some_and(|rest| rest != MCP_ARGS_SUFFIX_META && !rest.is_empty() && !rest.contains(':'))
            })
            .cloned()
            .collect();
        for k in stale {
            // Keep keys the caller is about to set (possibly empty = keep).
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
            // Empty = keep existing (configure dialog "leave blank to keep").
            continue;
        }
        all.insert(key, trimmed.to_owned());
    }

    if let Some(suffix) = args_suffix {
        let key = mcp_args_suffix_key(server_id);
        if suffix.is_empty() {
            all.remove(&key);
        } else {
            let encoded = serde_json::to_string(suffix)
                .map_err(|e| DesktopError::Config(e.to_string()))?;
            all.insert(key, encoded);
        }
    }

    SecretsStore::save_all(&dir, &all)?;
    Ok(())
}

/// Remove every secret belonging to one MCP server (called from `mcp_remove`).
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
    // Resolve + latch the secret storage mode for the rest of the process
    // *before* touching any secrets, so `SecretsStore`/`master_key` see the
    // right backend on this and every later call. `load_config` runs once
    // at startup, so this is race-free.
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
    // Provider keys replace their own namespace, but MCP secrets live in the
    // same `secrets.enc` blob — re-merge them so saving a provider profile
    // never deletes Slack/GitHub/Brave tokens.
    let existing = SecretsStore::load_all(&dir, &[])?;
    let mut combined = merge_combined_secrets(&cfg.keys, &cfg.profile_keys);
    preserve_mcp_secrets(&mut combined, &existing);
    SecretsStore::save_all(&dir, &combined)?;
    Ok(())
}

/// Current effective secret storage mode as a string (`"file"` |
/// `"keychain"`), for the frontend's settings display.
pub fn current_secret_storage_mode(prefs: &ProviderPrefs) -> &'static str {
    resolve_mode(prefs.secret_storage.as_deref()).as_str()
}

/// Change the secret storage backend: migrates the master key from the
/// current backend to `target` (see `SecretsStore::switch_mode`), then
/// persists the explicit choice in prefs so future launches honor it
/// without re-deriving from Keychain-item presence. On failure, the pref
/// and process-wide mode are left as they were (the migration itself is
/// all-or-nothing — see `switch_mode`'s doc comment).
pub fn set_secret_storage(
    cfg: &mut ProviderConfig,
    target: SecretStorageMode,
) -> DesktopResult<()> {
    // Keychain mode is macOS-only for now (see `secrets::resolve_mode`'s doc
    // comment) — reject the switch outright on other platforms with a clear
    // error rather than silently no-op'ing or falling back to File.
    if target == SecretStorageMode::Keychain && !cfg!(target_os = "macos") {
        return Err(DesktopError::Message(
            "Keychain secret storage is only available on macOS".into(),
        ));
    }
    let current = resolve_mode(cfg.prefs.secret_storage.as_deref());
    if current == target {
        // Still persist the explicit choice even if it matches the resolved
        // default, so a fresh install that happens to land on "file" by
        // default doesn't silently flip to "keychain" later just because a
        // stray Keychain item appears.
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
    /// One-time migration: if the legacy single-provider fields are set and
    /// no profile exists yet, wrap them into one profile (id `"default"`,
    /// label = the provider's display name) and activate it. The legacy
    /// fields are left untouched on `prefs` (read-only back-compat — see
    /// `ProviderPrefs` field docs); only `profiles`/`active_profile_id`/
    /// `profile_keys` gain the migrated data. Idempotent: a no-op once
    /// `profiles` is non-empty.
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

    /// The active profile, if any.
    pub fn active_profile(&self) -> Option<&ProviderProfile> {
        let id = self.prefs.active_profile_id.as_deref()?;
        self.prefs.profiles.iter().find(|p| p.id == id)
    }

    /// The active profile's stored API key, if any.
    pub fn active_profile_key(&self) -> Option<&String> {
        let id = self.prefs.active_profile_id.as_deref()?;
        self.profile_keys.get(id)
    }

    pub fn view(&self) -> ProviderConfigView {
        // Prefer the active profile once profiles exist; fall back to the
        // legacy top-level fields so a not-yet-migrated (or profile-less)
        // config still reports something sensible.
        if let Some(profile) = self.active_profile() {
            let has_key = self.profile_keys.contains_key(&profile.id);
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
            // Copilot is ready after a device-flow / editor sign-in
            // (`apps.json`) or when a GitHub token was pasted into the
            // profile key map.
            if profile.provider == "copilot" {
                return self.profile_keys.contains_key(&profile.id)
                    || agentloop_sdk::providers::copilot::CopilotConfig::discoverable();
            }
            return self.profile_keys.contains_key(&profile.id);
        }
        let Some(preferred) = self.prefs.preferred_provider.as_deref() else {
            return false;
        };
        // Ollama needs a host, not an API key.
        if preferred == "ollama" {
            return true;
        }
        if preferred == "copilot" {
            return self.keys.contains_key(preferred)
                || agentloop_sdk::providers::copilot::CopilotConfig::discoverable();
        }
        self.keys.contains_key(preferred)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_plugin_defaults_on_like_search() {
        let prefs = PluginPrefs::default();
        assert!(prefs.index, "IndexPlugin must default on for M1 live path");
        assert!(prefs.search);
        assert!(
            !prefs.auto_context,
            "auto-context must default off (opt-in via Settings or AGENTLOOP_AUTO_CONTEXT)"
        );
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
