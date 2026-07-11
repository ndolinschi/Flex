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

/// Snapshot of a non-isolated session's repo state at the moment the session
/// started (or was first resumed post-restart), used to scope the Changes
/// panel to files this session actually touched rather than the whole
/// repo's pre-existing dirty state.
pub struct SessionBaseline {
    /// `git rev-parse HEAD` in the session's `cwd` at capture time; empty
    /// string if the repo has no HEAD yet (e.g. freshly initialized).
    pub head_sha: String,
    /// Dirty paths (from `git status --porcelain`) at capture time, mapped
    /// to a `git hash-object` content hash. Deleted paths are recorded with
    /// the sentinel `"deleted"` since they have no blob to hash.
    pub files: HashMap<String, String>,
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
    /// Per-session repo baselines for non-isolated sessions, keyed by
    /// session id string. See [`SessionBaseline`].
    pub session_baselines: Mutex<HashMap<String, SessionBaseline>>,
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
            session_baselines: Mutex::new(HashMap::new()),
        }
    }
}
