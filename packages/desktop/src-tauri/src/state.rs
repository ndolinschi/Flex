//! Shared Tauri app state.

use std::collections::HashMap;
use std::sync::Arc;
// Plain (non-async) Mutex for the terminal registry: PTY I/O on the writer
// and resize calls are blocking, so a guard must never be held across an
// `.await` point. Aliased to avoid clashing with the existing tokio Mutex
// import used everywhere else in this struct.
use std::sync::Mutex as SyncMutex;

use agentloop_sdk::EngineService;
use agentloop_session::JsonlStore;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::config::ProviderConfig;

/// A live PTY-backed terminal session.
pub struct TerminalHandle {
    pub writer: Box<dyn std::io::Write + Send>,
    pub master: Box<dyn portable_pty::MasterPty + Send>,
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
    pub cwd: String,
    pub created_at_ms: u64,
}

pub struct AppState {
    pub service: Mutex<Option<EngineService>>,
    pub config: Mutex<ProviderConfig>,
    pub store: Arc<JsonlStore>,
    /// Active event-forwarding tasks keyed by session id string.
    pub subscriptions: Mutex<HashMap<String, JoinHandle<()>>>,
    /// Cancels the currently running routines cron-poll loop, if any — reset
    /// whenever the engine is rebuilt (e.g. after `save_provider_config`).
    pub routine_cancel: Mutex<Option<CancellationToken>>,
    /// Live terminal panel sessions keyed by terminal id (`term-{n}`). Uses a
    /// std (blocking) Mutex, not a tokio one: all PTY I/O (writer writes,
    /// resize, kill) is synchronous, and holding a tokio Mutex guard across
    /// blocking syscalls would risk stalling the async runtime. Commands
    /// must lock, do the blocking call, and drop the guard without ever
    /// `.await`-ing while it's held.
    pub terminals: SyncMutex<HashMap<String, TerminalHandle>>,
    /// Monotonic counter used to mint terminal ids (`term-{n}`).
    pub next_terminal_seq: SyncMutex<u64>,
    /// The single browser-panel webview, if the user has opened it.
    pub browser_webview: Mutex<Option<tauri::Webview>>,
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
        }
    }
}
