//! CLI preferences persisted under the XDG config directory.

use std::path::{Path, PathBuf};

use agentloop_contracts::ModelRef;
use serde::{Deserialize, Serialize};

use crate::catalog::CatalogEntry;

/// On-disk CLI preferences.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CliPrefs {
    /// Last successfully selected model (`provider/model` or bare id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_model: Option<String>,
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
}
