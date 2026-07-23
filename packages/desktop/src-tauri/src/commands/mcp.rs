
use super::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerDto {
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub secret_env: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub secret_args: Option<Vec<String>>,
    #[serde(default)]
    pub configured_secret_env: Vec<String>,
    #[serde(default)]
    pub has_secret_args: bool,
    pub enabled: bool,
}

pub(crate) fn mcp_dto_to_config(dto: &McpServerDto) -> agentloop_sdk::McpServerConfig {
    agentloop_sdk::McpServerConfig {
        name: dto.id.clone(),
        display_name: None,
        enabled: dto.enabled,
        transport: agentloop_sdk::McpServerTransport::Stdio(agentloop_sdk::StdioServerConfig {
            command: dto.command.clone(),
            args: dto.args.clone(),
            env: dto.env.clone(),
            cwd: None,
        }),
        tool_name_prefix: None,
    }
}

pub(crate) fn mcp_config_to_dto(config: agentloop_sdk::McpServerConfig) -> McpServerDto {
    let (command, args, mut env) = match config.transport {
        agentloop_sdk::McpServerTransport::Stdio(stdio) => (stdio.command, stdio.args, stdio.env),
        agentloop_sdk::McpServerTransport::StreamableHttp(http)
        | agentloop_sdk::McpServerTransport::Sse(http) => (http.url, Vec::new(), http.headers),
        _ => (String::new(), Vec::new(), std::collections::BTreeMap::new()),
    };

    let mut migrated_secrets = std::collections::BTreeMap::new();
    let secret_keys: Vec<String> = env
        .keys()
        .filter(|k| crate::config::is_likely_secret_env_name(k))
        .cloned()
        .collect();
    for key in secret_keys {
        if let Some(value) = env.remove(&key) {
            if !value.trim().is_empty() {
                migrated_secrets.insert(key, value);
            }
        }
    }
    if !migrated_secrets.is_empty() {
        match crate::config::upsert_mcp_server_secrets(&config.name, &migrated_secrets, false, None)
        {
            Ok(()) => {
                if let Some(dir) = agentloop_sdk::mcp_store::default_mcp_dir() {
                    let cleaned = agentloop_sdk::McpServerConfig {
                        name: config.name.clone(),
                        display_name: config.display_name.clone(),
                        enabled: config.enabled,
                        transport: agentloop_sdk::McpServerTransport::Stdio(
                            agentloop_sdk::StdioServerConfig {
                                command: command.clone(),
                                args: args.clone(),
                                env: env.clone(),
                                cwd: None,
                            },
                        ),
                        tool_name_prefix: config.tool_name_prefix.clone(),
                    };
                    match toml::to_string_pretty(&cleaned) {
                        Ok(content) => {
                            let path = dir.join(format!("{}.toml", config.name));
                            if let Err(err) = std::fs::write(&path, content) {
                                tracing::warn!(
                                    path = %path.display(),
                                    error = %err,
                                    "failed to rewrite MCP TOML after secret migration"
                                );
                            }
                        }
                        Err(err) => {
                            tracing::warn!(
                                server = %config.name,
                                error = %err,
                                "failed to serialize MCP TOML after secret migration"
                            );
                        }
                    }
                }
            }
            Err(err) => {
                tracing::warn!(
                    server = %config.name,
                    error = %err,
                    "failed to migrate plaintext MCP env secrets into encrypted store"
                );
                for (k, v) in migrated_secrets {
                    env.insert(k, v);
                }
            }
        }
    }

    let configured_secret_env =
        crate::config::list_mcp_configured_secret_env(&config.name).unwrap_or_default();
    let has_secret_args = crate::config::mcp_has_secret_args_suffix(&config.name).unwrap_or(false);

    McpServerDto {
        id: config.name,
        command,
        args,
        env,
        secret_env: std::collections::BTreeMap::new(),
        secret_args: None,
        configured_secret_env,
        has_secret_args,
        enabled: config.enabled,
    }
}

pub(crate) fn validate_mcp_id(id: &str) -> DesktopResult<&str> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(DesktopError::Message("server id is required".into()));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.chars().any(char::is_whitespace) {
        return Err(DesktopError::Message(
            "server id must not contain slashes or whitespace".into(),
        ));
    }
    Ok(trimmed)
}

pub(crate) fn mcp_store() -> DesktopResult<agentloop_sdk::mcp_store::FileMcpStore> {
    agentloop_sdk::mcp_store::FileMcpStore::with_default_dir()
        .ok_or_else(|| DesktopError::Message("could not resolve home directory".into()))
}

pub(crate) async fn rebuild_service_after_mcp_change(app: &AppHandle, state: &AppState) {
    let cfg = state.config.lock().await.clone();
    match crate::compose::build_service(&cfg, state.store.clone(), app.clone()) {
        Ok(service) => *state.service.lock().await = Some(service),
        Err(DesktopError::NotConfigured) => {}
        Err(err) => {
            tracing::warn!(error = %err, "failed to rebuild engine service after MCP change");
        }
    }
}

pub async fn mcp_list_internal() -> DesktopResult<Vec<McpServerDto>> {
    let store = mcp_store()?;
    let mut servers = store
        .list()
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;
    servers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(servers.into_iter().map(mcp_config_to_dto).collect())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn mcp_list() -> DesktopResult<Vec<McpServerDto>> {
    mcp_list_internal().await
}

pub async fn mcp_upsert_internal(
    app: &AppHandle,
    state: &AppState,
    server: McpServerDto,
) -> DesktopResult<()> {
    validate_mcp_id(&server.id)?;
    if server.command.trim().is_empty() {
        return Err(DesktopError::Message("command is required".into()));
    }

    let mut plaintext_env = server.env.clone();
    let mut secret_env = server.secret_env.clone();
    let leaked: Vec<String> = plaintext_env
        .keys()
        .filter(|k| crate::config::is_likely_secret_env_name(k))
        .cloned()
        .collect();
    for key in leaked {
        if let Some(value) = plaintext_env.remove(&key) {
            secret_env.entry(key).or_insert(value);
        }
    }

    let args_suffix = server.secret_args.as_deref();
    crate::config::upsert_mcp_server_secrets(
        &server.id,
        &secret_env,
 false,
        args_suffix,
    )?;

    let dto_for_toml = McpServerDto {
        env: plaintext_env,
        secret_env: std::collections::BTreeMap::new(),
        secret_args: None,
        configured_secret_env: Vec::new(),
        has_secret_args: false,
        ..server
    };
    let config = mcp_dto_to_config(&dto_for_toml);
    let store = mcp_store()?;
    store
        .upsert(config)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;
    rebuild_service_after_mcp_change(app, state).await;
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn mcp_upsert(
    app: AppHandle,
    state: State<'_, AppState>,
    server: McpServerDto,
) -> DesktopResult<()> {
    mcp_upsert_internal(&app, &state, server).await
}

pub async fn mcp_remove_internal(
    app: &AppHandle,
    state: &AppState,
    id: String,
) -> DesktopResult<()> {
    let id = validate_mcp_id(&id)?;
    let store = mcp_store()?;
    store
        .remove(id)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;
    if let Err(err) = crate::config::clear_mcp_server_secrets(id) {
        tracing::warn!(server = %id, error = %err, "failed to clear MCP secrets on remove");
    }
    rebuild_service_after_mcp_change(app, state).await;
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn mcp_remove(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> DesktopResult<()> {
    mcp_remove_internal(&app, &state, id).await
}

pub async fn mcp_test_internal(id: String) -> DesktopResult<Vec<String>> {
    let id = validate_mcp_id(&id)?;
    let store = mcp_store()?;
    let server = store
        .get(id)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?
        .ok_or_else(|| DesktopError::Message(format!("server `{id}` not found")))?;
    let server = crate::compose::resolve_mcp_server_secrets(server);

    let client = agentloop_sdk::mcp::RmcpToolClient::from_configs(std::slice::from_ref(&server));
    let tools = client
        .list_tools(&server, tokio_util::sync::CancellationToken::new())
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;
    client.shutdown().await;
    Ok(tools.into_iter().map(|tool| tool.name).collect())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn mcp_test(id: String) -> DesktopResult<Vec<String>> {
    mcp_test_internal(id).await
}
