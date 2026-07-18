//! Desktop-owned Remote Access transport.
//!
//! In-process HTTP/SSE API plus pluggable connection-method adapters
//! (manual, LAN, Bonjour, public port, Cloudflare stub, Bluetooth stub).
//! Clients talk to this surface — not to `flex serve` / the engine transport.

pub mod api;
pub mod auth;
pub mod config;
pub mod methods;
pub mod pairing;
pub mod server;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::error::{DesktopError, DesktopResult};
use crate::state::AppState;

use self::auth::AuthToken;
use self::config::{
    ensure_remote_token, load_remote_config, rotate_remote_token, save_remote_config, MethodPrefs,
    RemoteAccessConfig,
};
use self::pairing::PairingInfo;
pub use self::server::{init_remote_server, RemoteServerHandle};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAccessStatus {
    pub config: RemoteAccessConfig,
    pub running: bool,
    pub bind_addr: Option<String>,
    /// Bearer token (for Settings reveal/copy). Always present once remote
    /// has been enabled at least once.
    pub token: Option<String>,
    pub pairing: Option<PairingInfo>,
    pub pairing_json: Option<String>,
    /// SVG markup for a QR of `pairing_json`, when encodable.
    pub pairing_qr_svg: Option<String>,
    pub method_notes: Vec<MethodNote>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodNote {
    pub id: String,
    pub status: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRemoteAccessInput {
    pub enabled: bool,
    pub device_name: Option<String>,
    pub port: Option<u16>,
    pub methods: Option<MethodPrefs>,
}

#[tauri::command]
pub async fn remote_access_get(state: State<'_, AppState>) -> DesktopResult<RemoteAccessStatus> {
    let server = state.remote.lock().await;
    let Some(server) = server.as_ref() else {
        let cfg = load_remote_config()?;
        return Ok(RemoteAccessStatus {
            config: cfg,
            running: false,
            bind_addr: None,
            token: None,
            pairing: None,
            pairing_json: None,
            pairing_qr_svg: None,
            method_notes: Vec::new(),
        });
    };

    let config = server.snapshot_config().await;
    let running = server.is_running().await;
    let bind_addr = server.bind_addr().await.map(|a| a.to_string());
    let token = if config.enabled || running {
        Some(server.token_str().await)
    } else {
        config::load_remote_token()?
    };

    let (pairing, pairing_json, pairing_qr_svg) = if config.enabled {
        match server.pairing_payload().await {
            Ok((info, json, qr)) => (Some(info), Some(json), qr),
            Err(err) => {
                tracing::warn!(error = %err, "failed to build pairing payload");
                (None, None, None)
            }
        }
    } else {
        (None, None, None)
    };

    let method_notes = server
        .pairing_endpoints()
        .await
        .into_iter()
        .map(|ep| MethodNote {
            id: ep.method,
            status: ep.status.unwrap_or_else(|| "unknown".into()),
            note: ep.note,
        })
        .collect();

    Ok(RemoteAccessStatus {
        config,
        running,
        bind_addr,
        token,
        pairing,
        pairing_json,
        pairing_qr_svg,
        method_notes,
    })
}

#[tauri::command]
pub async fn remote_access_save(
    app: AppHandle,
    state: State<'_, AppState>,
    input: SaveRemoteAccessInput,
) -> DesktopResult<RemoteAccessStatus> {
    let mut cfg = load_remote_config()?;
    cfg.enabled = input.enabled;
    if let Some(name) = input.device_name {
        let trimmed = name.trim().to_owned();
        if !trimmed.is_empty() {
            cfg.device_name = trimmed;
        }
    }
    if let Some(port) = input.port {
        if port == 0 {
            return Err(DesktopError::Message("port must be non-zero".into()));
        }
        cfg.port = port;
    }
    if let Some(methods) = input.methods {
        cfg.methods = methods;
    }

    // Enabling remote access requires a persisted explicit token (needed for
    // non-loopback methods; also used for loopback-only manual pairing).
    if cfg.enabled {
        let _ = ensure_remote_token()?;
    }

    save_remote_config(&cfg)?;

    {
        let mut guard = state.remote.lock().await;
        if guard.is_none() {
            *guard = Some(init_remote_server()?);
        }
        if let Some(server) = guard.as_ref() {
            server.update_config(cfg).await?;
            let token = ensure_remote_token()?;
            server.set_token(AuthToken::new(token)).await;
            server.restart(app.clone()).await?;
        }
    }

    remote_access_get(state).await
}

#[tauri::command]
pub async fn remote_access_rotate_token(
    app: AppHandle,
    state: State<'_, AppState>,
) -> DesktopResult<RemoteAccessStatus> {
    let token = rotate_remote_token()?;
    {
        let mut guard = state.remote.lock().await;
        if guard.is_none() {
            *guard = Some(init_remote_server()?);
        }
        if let Some(server) = guard.as_ref() {
            server.set_token(AuthToken::new(token)).await;
            if server.snapshot_config().await.enabled {
                server.restart(app.clone()).await?;
            }
        }
    }
    remote_access_get(state).await
}

#[tauri::command]
pub async fn remote_access_restart(
    app: AppHandle,
    state: State<'_, AppState>,
) -> DesktopResult<RemoteAccessStatus> {
    {
        let mut guard = state.remote.lock().await;
        if guard.is_none() {
            *guard = Some(init_remote_server()?);
        }
        if let Some(server) = guard.as_ref() {
            server.restart(app.clone()).await?;
        }
    }
    remote_access_get(state).await
}
