//! MCP server installation and persistence for the CLI.
//!
//! Servers are stored in `{config_dir}/mcp.json` (mode 0600). GitHub clones
//! land under `{config_dir}/mcp-servers/`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use agentloop_mcp::{
    McpBridgeConfig, McpServerConfig, McpServerTransport, StdioServerConfig, StreamableHttpConfig,
};
use serde::{Deserialize, Serialize};

use crate::prefs::config_dir;

const REGISTRY_JSON: &str = include_str!("mcp_registry.json");

/// On-disk MCP configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpStore {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<InstalledMcpServer>,
}

/// One installed MCP server: engine config plus install metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledMcpServer {
    #[serde(flatten)]
    pub config: McpServerConfig,
    #[serde(flatten)]
    pub meta: InstalledMeta,
}

/// How a server was installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "install_source", rename_all = "snake_case")]
#[non_exhaustive]
pub enum McpInstallSource {
    Npm { package: String },
    GitHub { repo: String, rev: Option<String> },
    Imported { path: PathBuf },
    Registry { package: String },
}

/// Install metadata persisted alongside [`McpServerConfig`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledMeta {
    pub installed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_dir: Option<PathBuf>,
    #[serde(flatten)]
    pub source: McpInstallSource,
}

/// One entry in the curated npm registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpRegistryEntry {
    pub name: String,
    pub label: String,
    pub description: String,
    pub npm: String,
}

/// Failure reading, writing, or installing MCP servers.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum McpStoreError {
    #[error("cannot locate config directory: set XDG_CONFIG_HOME or HOME")]
    NoConfigDir,
    #[error("{context}: {message}")]
    Io {
        context: &'static str,
        message: String,
    },
    #[error("MCP server `{name}` already installed")]
    Duplicate { name: String },
    #[error("MCP server `{name}` not found")]
    NotFound { name: String },
    #[error("invalid MCP config file: {message}")]
    InvalidFile { message: String },
    #[error("could not detect launch command for `{repo}` — edit mcp.json manually")]
    UndetectedLaunch { repo: String },
    #[error("git clone failed for `{repo}`: {message}")]
    GitClone { repo: String, message: String },
    #[error(transparent)]
    Config(#[from] agentloop_mcp::McpConfigError),
}

impl McpStore {
    /// Load from the default `mcp.json`, or an empty store when missing.
    pub fn load() -> Self {
        match mcp_path() {
            Some(path) => Self::load_from(&path).unwrap_or_default(),
            None => Self::default(),
        }
    }

    /// Load from `path`. Missing file yields an empty store.
    pub fn load_from(path: &Path) -> Result<Self, McpStoreError> {
        let raw = match std::fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(err) => {
                return Err(McpStoreError::Io {
                    context: "read mcp.json",
                    message: err.to_string(),
                });
            }
        };
        let mut store: Self =
            serde_json::from_str(&raw).map_err(|err| McpStoreError::InvalidFile {
                message: err.to_string(),
            })?;
        if store.repair_known_misconfigs() {
            let _ = store.save_to(path);
        }
        Ok(store)
    }

    /// Persist to the default `mcp.json` (0600).
    pub fn save(&self) -> Result<(), McpStoreError> {
        let path = mcp_path().ok_or(McpStoreError::NoConfigDir)?;
        self.save_to(&path)
    }

    /// Persist to `path` (0600).
    pub fn save_to(&self, path: &Path) -> Result<(), McpStoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| McpStoreError::Io {
                context: "create config directory",
                message: err.to_string(),
            })?;
        }
        let raw = serde_json::to_string_pretty(self).map_err(|err| McpStoreError::Io {
            context: "serialize mcp.json",
            message: err.to_string(),
        })?;
        std::fs::write(path, raw).map_err(|err| McpStoreError::Io {
            context: "write mcp.json",
            message: err.to_string(),
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(
                |err| McpStoreError::Io {
                    context: "chmod mcp.json",
                    message: err.to_string(),
                },
            )?;
        }
        Ok(())
    }

    /// Enabled server count (for the status bar).
    pub fn enabled_count(&self) -> usize {
        self.servers
            .iter()
            .filter(|server| server.config.enabled)
            .count()
    }

    /// Load a project's declared integrations from `.agent/mcp.json`, or an
    /// empty store when the project has none. This is a project-local
    /// snapshot meant to be committed alongside the repo (`/init` writes it,
    /// `/mcp-import` reads it) — it is never the live source of truth for a
    /// running session, which always reads/writes the global store, so
    /// merely having this file present never auto-executes anything.
    pub fn load_project(project_dir: &Path) -> Result<Self, McpStoreError> {
        Self::load_from(&project_path(project_dir))
    }

    /// Snapshot this (global) store's servers into the project's
    /// `.agent/mcp.json`, so the integrations this project uses travel with
    /// it (e.g. committed to source control for teammates or future runs).
    pub fn export_to_project(&self, project_dir: &Path) -> Result<(), McpStoreError> {
        self.save_to(&project_path(project_dir))
    }

    /// Merge `incoming` servers whose name isn't already present, returning
    /// the names actually added. Existing servers are never overwritten —
    /// review happens before this is called (the caller confirms with the
    /// user first, since these can point at arbitrary launch commands).
    pub fn import_missing(&mut self, incoming: Vec<InstalledMcpServer>) -> Vec<String> {
        let existing: std::collections::HashSet<String> = self
            .servers
            .iter()
            .map(|server| server.config.name.clone())
            .collect();
        let mut added = Vec::new();
        for server in incoming {
            if existing.contains(&server.config.name) {
                continue;
            }
            added.push(server.config.name.clone());
            self.servers.push(server);
        }
        added
    }

    /// Convert to the engine bridge config (all servers, respecting `enabled`).
    pub fn to_bridge_config(&self) -> McpBridgeConfig {
        McpBridgeConfig {
            servers: self
                .servers
                .iter()
                .map(|server| server.config.clone())
                .collect(),
        }
    }

    /// Toggle `enabled` for `name`. Returns the new state.
    pub fn toggle_enabled(&mut self, name: &str) -> Result<bool, McpStoreError> {
        let server = self
            .servers
            .iter_mut()
            .find(|server| server.config.name == name)
            .ok_or_else(|| McpStoreError::NotFound {
                name: name.to_owned(),
            })?;
        server.config.enabled = !server.config.enabled;
        Ok(server.config.enabled)
    }

    /// Enable `name` and return whether it changed.
    pub fn enable(&mut self, name: &str) -> Result<bool, McpStoreError> {
        let server = self
            .servers
            .iter_mut()
            .find(|server| server.config.name == name)
            .ok_or_else(|| McpStoreError::NotFound {
                name: name.to_owned(),
            })?;
        let changed = !server.config.enabled;
        server.config.enabled = true;
        Ok(changed)
    }

    /// Fix known bad installs (e.g. bare `playwright` npm package).
    pub fn repair_known_misconfigs(&mut self) -> bool {
        let mut repaired = false;
        for server in &mut self.servers {
            if !is_wrong_playwright_install(server) {
                continue;
            }
            if let McpServerTransport::Stdio(ref mut stdio) = server.config.transport {
                stdio.args = vec!["-y".to_owned(), "@playwright/mcp".to_owned()];
            }
            if let McpInstallSource::Npm { ref mut package } = server.meta.source {
                *package = "@playwright/mcp".to_owned();
            }
            repaired = true;
        }
        repaired
    }

    /// Remove `name` from the store.
    pub fn remove(&mut self, name: &str) -> Result<InstalledMcpServer, McpStoreError> {
        let idx = self
            .servers
            .iter()
            .position(|server| server.config.name == name)
            .ok_or_else(|| McpStoreError::NotFound {
                name: name.to_owned(),
            })?;
        Ok(self.servers.remove(idx))
    }

    /// Install an npm package as an stdio MCP server via `npx -y`.
    pub fn install_npm(
        &mut self,
        package: &str,
        name: Option<&str>,
    ) -> Result<String, McpStoreError> {
        let package = normalize_npm_package(package);
        let name = name
            .map(str::to_owned)
            .unwrap_or_else(|| npm_server_name(&package));
        if self.servers.iter().any(|s| s.config.name == name) {
            return Err(McpStoreError::Duplicate { name });
        }
        let config = stdio_npm_config(&name, &package, Vec::new());
        self.push_server(config, McpInstallSource::Npm { package }, None)?;
        Ok(name)
    }

    /// Install from the curated registry by stable id.
    pub fn install_registry(&mut self, id: &str) -> Result<String, McpStoreError> {
        let entry = registry()
            .into_iter()
            .find(|entry| entry.name == id)
            .ok_or_else(|| McpStoreError::NotFound {
                name: id.to_owned(),
            })?;
        if self.servers.iter().any(|s| s.config.name == entry.name) {
            return Err(McpStoreError::Duplicate {
                name: entry.name.clone(),
            });
        }
        let config = stdio_npm_config(&entry.name, &entry.npm, Vec::new());
        self.push_server(
            config,
            McpInstallSource::Registry { package: entry.npm },
            None,
        )?;
        Ok(entry.name)
    }

    /// Shallow-clone a GitHub repo and detect its launch command.
    pub fn install_github(&mut self, repo: &str) -> Result<String, McpStoreError> {
        let repo = normalize_github_repo(repo);
        let name = github_server_name(&repo);
        if self.servers.iter().any(|s| s.config.name == name) {
            return Err(McpStoreError::Duplicate { name: name.clone() });
        }
        let install_dir = mcp_servers_dir()
            .ok_or(McpStoreError::NoConfigDir)?
            .join(&name);
        if install_dir.exists() {
            std::fs::remove_dir_all(&install_dir).map_err(|err| McpStoreError::Io {
                context: "remove stale mcp clone",
                message: err.to_string(),
            })?;
        }
        let status = Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                &format!("https://github.com/{repo}.git"),
                install_dir.to_str().ok_or_else(|| McpStoreError::Io {
                    context: "clone path",
                    message: "non-utf8 path".to_owned(),
                })?,
            ])
            .status()
            .map_err(|err| McpStoreError::GitClone {
                repo: repo.clone(),
                message: err.to_string(),
            })?;
        if !status.success() {
            return Err(McpStoreError::GitClone {
                repo: repo.clone(),
                message: format!("git exited with {status}"),
            });
        }
        let config = detect_github_config(&install_dir, &name)
            .ok_or_else(|| McpStoreError::UndetectedLaunch { repo: repo.clone() })?;
        self.push_server(
            config,
            McpInstallSource::GitHub {
                repo: repo.clone(),
                rev: None,
            },
            Some(install_dir),
        )?;
        Ok(name)
    }

    /// Merge servers from a Cursor/Claude `mcpServers` JSON file.
    pub fn import_from_file(&mut self, path: &Path) -> Result<Vec<String>, McpStoreError> {
        let raw = std::fs::read_to_string(path).map_err(|err| McpStoreError::Io {
            context: "read import file",
            message: err.to_string(),
        })?;
        let value: serde_json::Value =
            serde_json::from_str(&raw).map_err(|err| McpStoreError::InvalidFile {
                message: err.to_string(),
            })?;
        let servers = parse_mcp_servers_object(&value)
            .map_err(|message| McpStoreError::InvalidFile { message })?;
        let mut added = Vec::new();
        for (name, config) in servers {
            if self.servers.iter().any(|s| s.config.name == name) {
                continue;
            }
            self.push_server(
                config,
                McpInstallSource::Imported {
                    path: path.to_path_buf(),
                },
                None,
            )?;
            added.push(name);
        }
        Ok(added)
    }

    fn push_server(
        &mut self,
        config: McpServerConfig,
        source: McpInstallSource,
        install_dir: Option<PathBuf>,
    ) -> Result<(), McpStoreError> {
        config.validate()?;
        if self
            .servers
            .iter()
            .any(|server| server.config.name == config.name)
        {
            return Err(McpStoreError::Duplicate {
                name: config.name.clone(),
            });
        }
        self.servers.push(InstalledMcpServer {
            config,
            meta: InstalledMeta {
                installed_at: iso_now(),
                install_dir,
                source,
            },
        });
        Ok(())
    }
}

/// Curated registry entries embedded at compile time.
pub fn registry() -> Vec<McpRegistryEntry> {
    serde_json::from_str(REGISTRY_JSON).unwrap_or_default()
}

/// Default path: `~/.config/agentloop/mcp.json`.
pub fn mcp_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("mcp.json"))
}

/// A project's declared-integrations path: `<project>/.agent/mcp.json`.
pub fn project_path(project_dir: &Path) -> PathBuf {
    project_dir.join(".agent").join("mcp.json")
}

/// Clone root: `~/.config/agentloop/mcp-servers/`.
pub fn mcp_servers_dir() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("mcp-servers"))
}

/// Parse `owner/repo`, `github.com/owner/repo`, or an npm package name.
pub fn parse_install_target(input: &str) -> InstallTarget {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return InstallTarget::Unknown;
    }
    if trimmed.starts_with('@') || trimmed.starts_with("npm:") {
        return InstallTarget::Npm(normalize_npm_package(trimmed.trim_start_matches("npm:")));
    }
    if let Some(repo) = parse_github_repo(trimmed) {
        return InstallTarget::GitHub(repo);
    }
    InstallTarget::Npm(normalize_npm_package(trimmed))
}

/// What a one-shot `/mcp-install` argument refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallTarget {
    GitHub(String),
    Npm(String),
    Unknown,
}

fn stdio_npm_config(name: &str, package: &str, extra_args: Vec<String>) -> McpServerConfig {
    let mut args = vec!["-y".to_owned(), package.to_owned()];
    args.extend(extra_args);
    McpServerConfig {
        name: name.to_owned(),
        display_name: None,
        enabled: false,
        transport: McpServerTransport::Stdio(StdioServerConfig {
            command: "npx".to_owned(),
            args,
            env: BTreeMap::new(),
            cwd: None,
        }),
        tool_name_prefix: None,
    }
}

fn normalize_npm_package(input: &str) -> String {
    match input.trim() {
        "playwright" => "@playwright/mcp".to_owned(),
        other => other.to_owned(),
    }
}

fn is_wrong_playwright_install(server: &InstalledMcpServer) -> bool {
    let McpServerTransport::Stdio(stdio) = &server.config.transport else {
        return false;
    };
    if stdio.command != "npx" {
        return false;
    }
    let args = &stdio.args;
    args.first().map(String::as_str) == Some("-y")
        && args.get(1).map(String::as_str) == Some("playwright")
        && !args.iter().any(|arg| arg.contains("@playwright/mcp"))
}

fn npm_server_name(package: &str) -> String {
    package
        .rsplit('/')
        .next()
        .unwrap_or(package)
        .trim_start_matches('@')
        .replace('.', "-")
}

fn normalize_github_repo(input: &str) -> String {
    parse_github_repo(input).unwrap_or_else(|| input.trim().trim_matches('/').to_owned())
}

fn parse_github_repo(input: &str) -> Option<String> {
    let trimmed = input.trim().trim_end_matches('/');
    let trimmed = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
        .or_else(|| trimmed.strip_prefix("github.com/"))
        .unwrap_or(trimmed);
    let parts: Vec<&str> = trimmed.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() >= 2 {
        Some(format!("{}/{}", parts[0], parts[1]))
    } else {
        None
    }
}

fn github_server_name(repo: &str) -> String {
    repo.split('/').nth(1).unwrap_or(repo).replace('.', "-")
}

fn detect_github_config(dir: &Path, name: &str) -> Option<McpServerConfig> {
    for rel in [".cursor/mcp.json", "mcp.json"] {
        let path = dir.join(rel);
        if path.is_file() {
            if let Ok(raw) = std::fs::read_to_string(&path) {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if let Ok(servers) = parse_mcp_servers_object(&value) {
                        if let Some((_, config)) = servers.into_iter().next() {
                            let mut config = config;
                            config.name = name.to_owned();
                            return Some(config);
                        }
                    }
                }
            }
        }
    }
    let package_json = dir.join("package.json");
    if package_json.is_file() {
        if let Ok(raw) = std::fs::read_to_string(&package_json) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(command) = detect_package_launch(&value) {
                    return Some(McpServerConfig {
                        name: name.to_owned(),
                        display_name: None,
                        enabled: false,
                        transport: McpServerTransport::Stdio(StdioServerConfig {
                            command,
                            args: Vec::new(),
                            cwd: Some(dir.to_path_buf()),
                            env: BTreeMap::new(),
                        }),
                        tool_name_prefix: None,
                    });
                }
            }
        }
    }
    None
}

fn detect_package_launch(value: &serde_json::Value) -> Option<String> {
    if let Some(scripts) = value.get("scripts").and_then(|v| v.as_object()) {
        for key in ["mcp", "start:mcp", "serve"] {
            if scripts.contains_key(key) {
                return Some("npm".to_owned());
            }
        }
    }
    if let Some(bin) = value.get("bin") {
        if bin.is_string() || bin.as_object().is_some() {
            return Some("npx".to_owned());
        }
    }
    None
}

/// Parse a top-level or nested `mcpServers` object into configs.
pub fn parse_mcp_servers_object(
    value: &serde_json::Value,
) -> Result<Vec<(String, McpServerConfig)>, String> {
    let servers_value = value
        .get("mcpServers")
        .or_else(|| value.get("servers"))
        .unwrap_or(value);
    let Some(map) = servers_value.as_object() else {
        return Err("expected mcpServers object".to_owned());
    };
    let mut out = Vec::new();
    for (name, entry) in map {
        let config = parse_server_entry(name, entry)?;
        out.push((name.clone(), config));
    }
    Ok(out)
}

fn parse_server_entry(name: &str, entry: &serde_json::Value) -> Result<McpServerConfig, String> {
    if let Some(url) = entry.get("url").and_then(|v| v.as_str()) {
        let transport = if entry
            .get("transport")
            .and_then(|v| v.as_str())
            .is_some_and(|t| t.eq_ignore_ascii_case("sse"))
        {
            McpServerTransport::Sse(StreamableHttpConfig {
                url: url.to_owned(),
                headers: parse_headers(entry),
            })
        } else {
            McpServerTransport::StreamableHttp(StreamableHttpConfig {
                url: url.to_owned(),
                headers: parse_headers(entry),
            })
        };
        return Ok(McpServerConfig {
            name: name.to_owned(),
            display_name: entry
                .get("display_name")
                .or_else(|| entry.get("displayName"))
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            enabled: entry
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            transport,
            tool_name_prefix: entry
                .get("tool_name_prefix")
                .or_else(|| entry.get("toolNamePrefix"))
                .and_then(|v| v.as_str())
                .map(str::to_owned),
        });
    }
    let command = entry
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("server `{name}` missing command or url"))?;
    let args = entry
        .get("args")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let env = entry
        .get("env")
        .and_then(|v| v.as_object())
        .map(|map| {
            map.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let cwd = entry.get("cwd").and_then(|v| v.as_str()).map(PathBuf::from);
    Ok(McpServerConfig {
        name: name.to_owned(),
        display_name: entry
            .get("display_name")
            .or_else(|| entry.get("displayName"))
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        enabled: entry
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        transport: McpServerTransport::Stdio(StdioServerConfig {
            command: command.to_owned(),
            args,
            env,
            cwd,
        }),
        tool_name_prefix: entry
            .get("tool_name_prefix")
            .or_else(|| entry.get("toolNamePrefix"))
            .and_then(|v| v.as_str())
            .map(str::to_owned),
    })
}

fn parse_headers(entry: &serde_json::Value) -> BTreeMap<String, String> {
    entry
        .get("headers")
        .and_then(|v| v.as_object())
        .map(|map| {
            map.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}

fn iso_now() -> String {
    let days = agentloop_contracts::now_ms() / 86_400_000;
    format!("day-{days}")
}

impl InstalledMcpServer {
    /// Short source label for picker UIs.
    pub fn source_label(&self) -> String {
        match &self.meta.source {
            McpInstallSource::Npm { package } => format!("npm:{package}"),
            McpInstallSource::Registry { package } => format!("registry:{package}"),
            McpInstallSource::GitHub { repo, .. } => format!("github:{repo}"),
            McpInstallSource::Imported { path } => {
                format!(
                    "import:{}",
                    path.file_name().and_then(|n| n.to_str()).unwrap_or("file")
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mcp.json");
        let mut store = McpStore::default();
        store
            .install_npm("@modelcontextprotocol/server-fetch", Some("fetch"))
            .expect("install");
        store.save_to(&path).expect("save");
        let loaded = McpStore::load_from(&path).expect("load");
        assert_eq!(loaded.servers.len(), 1);
        assert_eq!(loaded.servers[0].config.name, "fetch");
        assert!(!loaded.servers[0].config.enabled);
    }

    #[test]
    fn toggle_enabled_flips_state() {
        let mut store = McpStore::default();
        store
            .install_npm("server-memory", Some("memory"))
            .expect("install");
        assert!(!store.servers[0].config.enabled);
        let enabled = store.toggle_enabled("memory").expect("toggle");
        assert!(enabled);
        let disabled = store.toggle_enabled("memory").expect("toggle");
        assert!(!disabled);
    }

    #[test]
    fn export_to_project_then_load_project_round_trips() {
        let project = tempfile::tempdir().expect("tempdir");
        let mut store = McpStore::default();
        store
            .install_npm("@modelcontextprotocol/server-fetch", Some("fetch"))
            .expect("install");

        store
            .export_to_project(project.path())
            .expect("export to project");
        assert!(project_path(project.path()).exists());

        let loaded = McpStore::load_project(project.path()).expect("load project");
        assert_eq!(loaded.servers.len(), 1);
        assert_eq!(loaded.servers[0].config.name, "fetch");
    }

    #[test]
    fn load_project_with_no_file_is_empty() {
        let project = tempfile::tempdir().expect("tempdir");
        let loaded = McpStore::load_project(project.path()).expect("load project");
        assert!(loaded.servers.is_empty());
    }

    #[test]
    fn import_missing_skips_existing_names() {
        let mut store = McpStore::default();
        store
            .install_npm("server-memory", Some("memory"))
            .expect("install");

        let mut incoming = McpStore::default();
        incoming
            .install_npm("server-memory", Some("memory"))
            .expect("install");
        incoming
            .install_npm("@modelcontextprotocol/server-fetch", Some("fetch"))
            .expect("install");

        let added = store.import_missing(incoming.servers);
        assert_eq!(added, vec!["fetch".to_owned()]);
        assert_eq!(store.servers.len(), 2);
    }

    #[test]
    fn import_parses_cursor_stdio_shape() {
        let raw = r#"{
            "mcpServers": {
                "fs": {
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
                }
            }
        }"#;
        let value: serde_json::Value = serde_json::from_str(raw).expect("json");
        let servers = parse_mcp_servers_object(&value).expect("parse");
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].0, "fs");
        match &servers[0].1.transport {
            McpServerTransport::Stdio(cfg) => {
                assert_eq!(cfg.command, "npx");
                assert_eq!(cfg.args[1], "@modelcontextprotocol/server-filesystem");
            }
            _ => panic!("expected stdio"),
        }
    }

    #[test]
    fn import_parses_http_url() {
        let raw = r#"{"mcpServers":{"remote":{"url":"https://example.test/mcp"}}}"#;
        let value: serde_json::Value = serde_json::from_str(raw).expect("json");
        let servers = parse_mcp_servers_object(&value).expect("parse");
        match &servers[0].1.transport {
            McpServerTransport::StreamableHttp(cfg) => {
                assert_eq!(cfg.url, "https://example.test/mcp");
            }
            _ => panic!("expected http"),
        }
    }

    #[test]
    fn parse_install_target_github_and_npm() {
        assert_eq!(
            parse_install_target("octo/repo"),
            InstallTarget::GitHub("octo/repo".to_owned())
        );
        assert_eq!(
            parse_install_target("https://github.com/octo/repo"),
            InstallTarget::GitHub("octo/repo".to_owned())
        );
        assert_eq!(
            parse_install_target("@scope/pkg"),
            InstallTarget::Npm("@scope/pkg".to_owned())
        );
    }

    #[test]
    fn bridge_config_respects_enabled_flag() {
        let mut store = McpStore::default();
        store
            .install_npm("server-memory", Some("mem"))
            .expect("install");
        store.enable("mem").expect("enable");
        let config = store.to_bridge_config();
        assert_eq!(config.enabled_servers().count(), 1);
    }

    #[test]
    fn registry_has_entries() {
        assert!(registry().len() >= 10);
    }

    #[test]
    fn playwright_install_resolves_to_mcp_package() {
        assert_eq!(
            parse_install_target("playwright"),
            InstallTarget::Npm("@playwright/mcp".to_owned())
        );
        let mut store = McpStore::default();
        store.install_npm("playwright", None).expect("install");
        match &store.servers[0].config.transport {
            McpServerTransport::Stdio(cfg) => {
                assert_eq!(cfg.args[1], "@playwright/mcp");
            }
            other => panic!("expected stdio transport, got {other:?}"),
        }
    }

    #[test]
    fn repair_fixes_wrong_playwright_npm_install() {
        let mut store = McpStore::default();
        store.install_npm("playwright", None).expect("install");
        if let McpServerTransport::Stdio(ref mut stdio) = store.servers[0].config.transport {
            stdio.args[1] = "playwright".to_owned();
        }
        if let McpInstallSource::Npm { ref mut package } = store.servers[0].meta.source {
            *package = "playwright".to_owned();
        }
        assert!(store.repair_known_misconfigs());
        match &store.servers[0].config.transport {
            McpServerTransport::Stdio(cfg) => {
                assert_eq!(cfg.args[1], "@playwright/mcp");
            }
            other => panic!("expected stdio transport, got {other:?}"),
        }
    }
}
