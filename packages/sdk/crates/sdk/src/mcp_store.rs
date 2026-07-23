use std::path::PathBuf;

use agentloop_mcp::{McpBridgeConfig, McpServerConfig};

pub fn default_mcp_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("agentloop")
            .join("mcp")
    })
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum McpStoreError {
    #[error("MCP server `{0}` not found")]
    NotFound(String),
    #[error("storage error: {0}")]
    Storage(String),
}

pub struct FileMcpStore {
    dir: PathBuf,
}

impl FileMcpStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn with_default_dir() -> Option<Self> {
        default_mcp_dir().map(Self::new)
    }

    fn spec_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.toml"))
    }

    pub async fn list(&self) -> Result<Vec<McpServerConfig>, McpStoreError> {
        let dir = self.dir.clone();
        tokio::task::spawn_blocking(move || {
            let entries = match std::fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
                Err(err) => return Err(io_err(err)),
            };
            let mut specs = Vec::new();
            for entry in entries {
                let path = entry.map_err(io_err)?.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
                    continue;
                }
                let content = std::fs::read_to_string(&path).map_err(io_err)?;
                specs.push(toml::from_str(&content).map_err(io_err)?);
            }
            Ok(specs)
        })
        .await
        .map_err(io_err)?
    }

    pub async fn enabled_bridge_config(&self) -> McpBridgeConfig {
        let servers = self.list().await.unwrap_or_default();
        McpBridgeConfig {
            servers: servers.into_iter().filter(|s| s.enabled).collect(),
        }
    }

    pub async fn get(&self, id: &str) -> Result<Option<McpServerConfig>, McpStoreError> {
        let path = self.spec_path(id);
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => Ok(Some(toml::from_str(&content).map_err(io_err)?)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(io_err(err)),
        }
    }

    pub async fn upsert(&self, spec: McpServerConfig) -> Result<(), McpStoreError> {
        spec.validate()
            .map_err(|err| McpStoreError::Storage(err.to_string()))?;
        tokio::fs::create_dir_all(&self.dir).await.map_err(io_err)?;
        let content = toml::to_string_pretty(&spec).map_err(io_err)?;
        tokio::fs::write(self.spec_path(&spec.name), content)
            .await
            .map_err(io_err)
    }

    pub async fn remove(&self, id: &str) -> Result<(), McpStoreError> {
        match tokio::fs::remove_file(self.spec_path(id)).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Err(McpStoreError::NotFound(id.to_owned()))
            }
            Err(err) => Err(io_err(err)),
        }
    }
}

fn io_err(err: impl std::fmt::Display) -> McpStoreError {
    McpStoreError::Storage(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str) -> McpServerConfig {
        McpServerConfig::stdio(id, "mcp-server-binary")
    }

    #[tokio::test]
    async fn round_trips_a_server_spec() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = FileMcpStore::new(dir.path());

        assert!(store.list().await.expect("list").is_empty());
        assert!(store.get("s1").await.expect("get").is_none());

        store.upsert(sample("s1")).await.expect("upsert");
        let fetched = store.get("s1").await.expect("get").expect("present");
        assert_eq!(fetched.name, "s1");
        assert_eq!(store.list().await.expect("list").len(), 1);

        store.remove("s1").await.expect("remove");
        assert!(store.get("s1").await.expect("get").is_none());
        assert!(matches!(
            store.remove("s1").await,
            Err(McpStoreError::NotFound(id)) if id == "s1"
        ));
    }

    #[tokio::test]
    async fn enabled_bridge_config_filters_disabled_servers() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = FileMcpStore::new(dir.path());

        let mut disabled = sample("off");
        disabled.enabled = false;
        store.upsert(sample("on")).await.expect("upsert enabled");
        store.upsert(disabled).await.expect("upsert disabled");

        let config = store.enabled_bridge_config().await;
        assert_eq!(config.servers.len(), 1);
        assert_eq!(config.servers[0].name, "on");
    }

    #[tokio::test]
    async fn upsert_rejects_invalid_spec() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = FileMcpStore::new(dir.path());
        let mut bad = sample("bad");
        bad.name = String::new();

        assert!(matches!(
            store.upsert(bad).await,
            Err(McpStoreError::Storage(_))
        ));
    }
}
