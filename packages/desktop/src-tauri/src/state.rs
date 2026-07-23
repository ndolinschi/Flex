
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use std::sync::Mutex as SyncMutex;

use agentloop_sdk::providers::copilot::DeviceAuthorization;
use agentloop_sdk::providers::openai::OpenAiOAuthStart;
use agentloop_sdk::EngineService;
use agentloop_session::JsonlStore;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::config::ProviderConfig;

pub const WORKSPACE_PATH_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct CachedPathEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone)]
pub struct WorkspacePathCache {
    pub entries: Vec<CachedPathEntry>,
    pub built_at: Instant,
}

impl WorkspacePathCache {
    pub fn fresh(&self) -> bool {
        self.built_at.elapsed() < WORKSPACE_PATH_CACHE_TTL
    }
}

pub struct PendingCopilotAuth {
    pub auth: DeviceAuthorization,
    pub cancel: CancellationToken,
}

pub struct PendingChatgptAuth {
    pub start: OpenAiOAuthStart,
    pub cancel: CancellationToken,
}

pub struct TerminalHandle {
    pub writer: Box<dyn std::io::Write + Send>,
    pub master: Box<dyn portable_pty::MasterPty + Send>,
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
    pub cwd: String,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBaseline {
    pub head_sha: String,
    pub files: HashMap<String, String>,
}

const SESSION_BASELINES_FILE: &str = "session_baselines.json";

fn session_baselines_path() -> Option<std::path::PathBuf> {
    let dir = dirs::data_dir()?.join("agentloop").join("desktop");
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }
    Some(dir.join(SESSION_BASELINES_FILE))
}

pub fn load_session_baselines() -> HashMap<String, SessionBaseline> {
    let Some(path) = session_baselines_path() else {
        return HashMap::new();
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save_session_baselines(baselines: &HashMap<String, SessionBaseline>) {
    let Some(path) = session_baselines_path() else {
        tracing::warn!("could not resolve session baselines path; not persisting");
        return;
    };
    match serde_json::to_string_pretty(baselines) {
        Ok(raw) => {
            if let Err(err) = std::fs::write(&path, raw) {
                tracing::warn!(error = %err, path = %path.display(), "failed to persist session baselines");
            }
        }
        Err(err) => {
            tracing::warn!(error = %err, "failed to serialize session baselines");
        }
    }
}

pub struct AppState {
    pub service: Mutex<Option<EngineService>>,
    pub config: Mutex<ProviderConfig>,
    pub store: Arc<JsonlStore>,
    pub subscriptions: Mutex<HashMap<String, JoinHandle<()>>>,
    pub routine_cancel: Mutex<Option<CancellationToken>>,
    pub terminals: SyncMutex<HashMap<String, TerminalHandle>>,
    pub next_terminal_seq: SyncMutex<u64>,
    pub browser_webview: Mutex<Option<tauri::Webview>>,
    pub browser_bounds: SyncMutex<Option<(f64, f64, f64, f64)>>,
    pub browser_design_mode: SyncMutex<bool>,
    pub session_baselines: Mutex<HashMap<String, SessionBaseline>>,
    pub baseline_inflight: Mutex<HashSet<String>>,
    pub pending_copilot_auth: Mutex<HashMap<String, PendingCopilotAuth>>,
    pub pending_chatgpt_auth: Mutex<HashMap<String, PendingChatgptAuth>>,
    pub db_plugin: Mutex<crate::db_plugin::DbPluginState>,
    pub remote: Mutex<Option<crate::remote::RemoteServerHandle>>,
    pub workspace_path_cache: SyncMutex<HashMap<(String, bool), WorkspacePathCache>>,
}

impl AppState {
    pub fn new(
        store: Arc<JsonlStore>,
        config: ProviderConfig,
        service: Option<EngineService>,
    ) -> Self {
        Self {
            service: Mutex::new(service),
            config: Mutex::new(config),
            store,
            subscriptions: Mutex::new(HashMap::new()),
            routine_cancel: Mutex::new(None),
            terminals: SyncMutex::new(HashMap::new()),
            next_terminal_seq: SyncMutex::new(0),
            browser_webview: Mutex::new(None),
            browser_bounds: SyncMutex::new(None),
            browser_design_mode: SyncMutex::new(false),
            session_baselines: Mutex::new(load_session_baselines()),
            baseline_inflight: Mutex::new(HashSet::new()),
            pending_copilot_auth: Mutex::new(HashMap::new()),
            pending_chatgpt_auth: Mutex::new(HashMap::new()),
            db_plugin: Mutex::new(crate::db_plugin::DbPluginState::load()),
            remote: Mutex::new(crate::remote::init_remote_server().ok()),
            workspace_path_cache: SyncMutex::new(HashMap::new()),
        }
    }
}
