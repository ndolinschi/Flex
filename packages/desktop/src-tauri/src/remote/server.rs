
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::middleware;
use axum::routing::get;
use axum::Router;
use tauri::AppHandle;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tower_http::trace::TraceLayer;

use super::api::{v1_router, RemoteApiState};
use super::auth::{require_bearer_token, AuthToken};
use super::config::{
    ensure_remote_token, load_remote_config, save_remote_config, RemoteAccessConfig,
};
use super::methods::{build_methods, ConnectionMethod, MethodContext};
use super::pairing::{pairing_qr_svg, PairingEndpoint, PairingInfo};
use crate::error::{DesktopError, DesktopResult};

pub struct RemoteServer {
    config: Arc<RwLock<RemoteAccessConfig>>,
    token: Arc<RwLock<AuthToken>>,
    methods: Mutex<Vec<Box<dyn ConnectionMethod>>>,
    cancel: Mutex<Option<CancellationToken>>,
    http_task: Mutex<Option<JoinHandle<()>>>,
    bind_addr: Mutex<Option<SocketAddr>>,
}

impl RemoteServer {
    pub fn new(config: RemoteAccessConfig, token: AuthToken) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            token: Arc::new(RwLock::new(token)),
            methods: Mutex::new(Vec::new()),
            cancel: Mutex::new(None),
            http_task: Mutex::new(None),
            bind_addr: Mutex::new(None),
        }
    }

    pub async fn snapshot_config(&self) -> RemoteAccessConfig {
        self.config.read().await.clone()
    }

    pub async fn update_config(&self, cfg: RemoteAccessConfig) -> DesktopResult<()> {
        save_remote_config(&cfg)?;
        *self.config.write().await = cfg;
        Ok(())
    }

    pub async fn set_token(&self, token: AuthToken) {
        *self.token.write().await = token;
    }

    pub async fn token_str(&self) -> String {
        self.token.read().await.as_str().to_owned()
    }

    pub async fn is_running(&self) -> bool {
        self.cancel.lock().await.is_some()
    }

    pub async fn bind_addr(&self) -> Option<SocketAddr> {
        *self.bind_addr.lock().await
    }

    pub async fn stop(&self) -> DesktopResult<()> {
        if let Some(token) = self.cancel.lock().await.take() {
            token.cancel();
        }
        if let Some(handle) = self.http_task.lock().await.take() {
            handle.abort();
        }
        let mut methods = self.methods.lock().await;
        for method in methods.iter_mut() {
            let _ = method.stop().await;
        }
        methods.clear();
        *self.bind_addr.lock().await = None;
        Ok(())
    }

    pub async fn start(&self, app: AppHandle) -> DesktopResult<()> {
        self.stop().await?;

        let cfg = self.config.read().await.clone();
        if !cfg.enabled || !cfg.wants_http_listener() {
            return Ok(());
        }

        let token_str = ensure_remote_token()?;
        self.set_token(AuthToken::new(token_str.clone())).await;

        let bind_ip = if cfg.needs_non_loopback() {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        } else {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        };
        let addr = SocketAddr::new(bind_ip, cfg.port);
        let listener = TcpListener::bind(addr).await.map_err(|e| {
            DesktopError::Message(format!("remote access: failed to bind {addr}: {e}"))
        })?;
        let bound = listener
            .local_addr()
            .map_err(|e| DesktopError::Message(format!("remote access: local_addr failed: {e}")))?;
        *self.bind_addr.lock().await = Some(bound);

        let api_state = RemoteApiState {
            app: app.clone(),
            config: self.config.clone(),
        };
        let auth_token = AuthToken::new(token_str);
        let protected = v1_router()
            .layer(middleware::from_fn_with_state(
                auth_token.clone(),
                require_bearer_token,
            ))
            .with_state(api_state);

        let app_router = Router::new()
            .route("/health", get(|| async { StatusCode::OK }))
            .nest("/v1", protected)
            .layer(TraceLayer::new_for_http());

        let cancel = CancellationToken::new();
        *self.cancel.lock().await = Some(cancel.clone());

        let task = tokio::spawn(async move {
            let server = axum::serve(listener, app_router).with_graceful_shutdown(async move {
                cancel.cancelled().await;
            });
            if let Err(err) = server.await {
                tracing::error!(error = %err, "remote access HTTP server exited with error");
            }
        });
        *self.http_task.lock().await = Some(task);

        let mut methods = build_methods(&cfg);
        let ctx = MethodContext {
            config: &cfg,
            listen_port: bound.port(),
            bind_addr: bound.to_string(),
        };
        for method in methods.iter_mut() {
            if let Err(err) = method.start(&ctx).await {
                tracing::warn!(
                    method = method.id(),
                    error = %err,
                    "remote access connection method failed to start"
                );
            }
        }
        *self.methods.lock().await = methods;

        tracing::info!(%bound, "remote access HTTP listener started");
        Ok(())
    }

    pub async fn restart(&self, app: AppHandle) -> DesktopResult<()> {
        let cfg = self.config.read().await.clone();
        if cfg.enabled && cfg.wants_http_listener() {
            self.start(app).await
        } else {
            self.stop().await
        }
    }

    pub async fn pairing_endpoints(&self) -> Vec<PairingEndpoint> {
        let cfg = self.config.read().await.clone();
        let methods = self.methods.lock().await;
        let mut endpoints = Vec::new();
        for method in methods.iter() {
            endpoints.extend(method.pairing_endpoints(&cfg));
        }
        if endpoints.is_empty() && cfg.enabled {
            for method in build_methods(&cfg) {
                endpoints.extend(method.pairing_endpoints(&cfg));
            }
        }
        endpoints
    }

    pub async fn pairing_info(&self) -> DesktopResult<PairingInfo> {
        let cfg = self.config.read().await.clone();
        let token = self.token_str().await;
        let endpoints = self.pairing_endpoints().await;
        Ok(PairingInfo::build(
            &cfg,
            &token,
            endpoints,
            env!("CARGO_PKG_VERSION"),
        ))
    }

    pub async fn pairing_payload(&self) -> DesktopResult<(PairingInfo, String, Option<String>)> {
        let info = self.pairing_info().await?;
        let json = info
            .to_pairing_json()
            .map_err(|e| DesktopError::Message(e.to_string()))?;
        let qr = pairing_qr_svg(&json).ok();
        Ok((info, json, qr))
    }
}

use axum::http::StatusCode;

pub type RemoteServerHandle = Arc<RemoteServer>;

pub fn init_remote_server() -> DesktopResult<RemoteServerHandle> {
    let cfg = load_remote_config()?;
    let token = match super::config::load_remote_token()? {
        Some(t) if !t.is_empty() => AuthToken::new(t),
        _ => AuthToken::generate(),
    };
    Ok(Arc::new(RemoteServer::new(cfg, token)))
}
