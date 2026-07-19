//! MCP server config CRUD and connectivity test.

use super::prelude::*;

// ---------------------------------------------------------------------------
// MCP servers: user-configured Model Context Protocol servers whose tools are
// bridged into the native tool registry (`agentloop_mcp::McpManager`, folded
// in by `EngineService::native` from `EngineConfig.mcp`). Specs persist via
// `agentloop_sdk::mcp_store::FileMcpStore` (one `<id>.toml` under
// `~/.config/agentloop/mcp`, mirroring `FileRoutineStore`). Unlike routines,
// saving/removing a server must rebuild the engine service (mirrors
// `save_provider_config`) since `EngineConfig.mcp` is only read at
// composition time in `compose::build_service` — there is no hot-reload of a
// running session, hence the UI copy pointing users at restarting sessions.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerDto {
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Non-secret environment variables (persisted in the MCP TOML file).
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    /// Secret environment values to write into the encrypted secrets store.
    /// On upsert: non-empty values overwrite; empty string keeps the existing
    /// secret (configure dialog). On list responses this map is always empty
    /// — see [`configured_secret_env`].
    #[serde(default)]
    pub secret_env: std::collections::BTreeMap<String, String>,
    /// Secret positional-arg values appended after `args` at resolve time
    /// (e.g. a Postgres connection string). Same keep-if-empty semantics as
    /// `secret_env` when `has_secret_args` is already true; omitted/`None` on
    /// list. Frontend sends these only for catalog installs that declare
    /// secret `argKeys`.
    #[serde(default)]
    pub secret_args: Option<Vec<String>>,
    /// Env key names that currently have a stored secret (values never
    /// returned). Populated on list; ignored on upsert.
    #[serde(default)]
    pub configured_secret_env: Vec<String>,
    /// Whether a secret positional-args suffix is stored for this server.
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
            // Only non-secret env lands in the TOML file. Secrets are merged
            // back at resolve time (compose / mcp_test) from the encrypted store.
            env: dto.env.clone(),
            cwd: None,
        }),
        tool_name_prefix: None,
    }
}

pub(crate) fn mcp_config_to_dto(config: agentloop_sdk::McpServerConfig) -> McpServerDto {
    let (command, args, mut env) = match config.transport {
        agentloop_sdk::McpServerTransport::Stdio(stdio) => (stdio.command, stdio.args, stdio.env),
        // The desktop UI only manages stdio servers (MVP scope); any other
        // transport a `.toml` file might carry (hand-edited, or added by a
        // future UI) still round-trips through list/remove, just with an
        // empty command shown — `mcp_upsert` always writes Stdio, so this
        // path shouldn't normally be hit for desktop-managed servers.
        agentloop_sdk::McpServerTransport::StreamableHttp(http)
        | agentloop_sdk::McpServerTransport::Sse(http) => (http.url, Vec::new(), http.headers),
        _ => (String::new(), Vec::new(), std::collections::BTreeMap::new()),
    };

    // Migrate any plaintext credential-looking env vars still sitting in the
    // TOML (installed before secret storage landed) into the encrypted store,
    // then strip them from the DTO so the renderer never sees the values.
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
                // Rewrite the TOML without the migrated secrets so they don't
                // linger on disk in plaintext (sync write — avoids nesting
                // `block_on` inside the async `mcp_list` path).
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
                // Put them back so the server still works this session even if
                // the secrets write failed.
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

/// Rebuild the engine service from the current provider config so a saved
/// MCP server change takes effect on the next session (existing sessions
/// keep running against the service they already captured). Errors are
/// swallowed on purpose: MCP servers are additive and the provider might not
/// be configured yet (`DesktopError::NotConfigured`), which must not block
/// saving/removing a server spec.
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

/// Shared with the desktop Remote Access HTTP API.
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

/// Shared with the desktop Remote Access HTTP API.
pub async fn mcp_upsert_internal(
    app: &AppHandle,
    state: &AppState,
    server: McpServerDto,
) -> DesktopResult<()> {
    validate_mcp_id(&server.id)?;
    if server.command.trim().is_empty() {
        return Err(DesktopError::Message("command is required".into()));
    }

    // Split credential-looking keys out of the plaintext `env` map as a
    // safety net (manual "Add server" form, or a catalog client that forgot
    // to use `secretEnv`). Catalog installs should already send them in
    // `secret_env`.
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
        /* replace_env */ false,
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

/// Shared with the desktop Remote Access HTTP API.
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

/// Connect to a saved server and list its tools — the "Test" button in the
/// UI. Talks to the MCP client directly (not through `McpManager`, which
/// keys server lookups off its own config snapshot) and never touches
/// `state.service`, so testing never disturbs the live engine or requires a
/// provider to be configured yet.
///
/// Shared with the desktop Remote Access HTTP API.
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
