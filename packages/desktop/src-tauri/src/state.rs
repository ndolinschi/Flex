//! Shared Tauri app state.

use std::collections::HashMap;
use std::sync::Arc;
// Plain (non-async) Mutex for the terminal registry: PTY I/O on the writer
// and resize calls are blocking, so a guard must never be held across an
// `.await` point. Aliased to avoid clashing with the existing tokio Mutex
// import used everywhere else in this struct.
use std::sync::Mutex as SyncMutex;

use agentloop_sdk::providers::copilot::DeviceAuthorization;
use agentloop_sdk::EngineService;
use agentloop_session::JsonlStore;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::config::ProviderConfig;

/// In-flight GitHub Copilot device-code sign-in. The full
/// [`DeviceAuthorization`] (including the private `device_code`) stays on
/// this side of the IPC boundary — the frontend only sees the session id
/// plus the public user code / verification URI.
pub struct PendingCopilotAuth {
    pub auth: DeviceAuthorization,
    pub cancel: CancellationToken,
}

/// A live PTY-backed terminal session.
pub struct TerminalHandle {
    pub writer: Box<dyn std::io::Write + Send>,
    pub master: Box<dyn portable_pty::MasterPty + Send>,
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
    pub cwd: String,
    pub created_at_ms: u64,
}

/// Snapshot of a non-isolated session's repo state at the moment the session
/// started (or was first, lazily, backfilled for a legacy pre-persistence
/// session), used to scope the Changes panel to files this session actually
/// touched rather than the whole repo's pre-existing dirty state.
///
/// Persisted to disk (see [`load_session_baselines`]/[`save_session_baselines`])
/// so it survives app restart — the in-memory `AppState::session_baselines`
/// map is just a hot cache over the same data. This is the fix for the
/// original "changes vanish when you reopen the chat" bug: previously this
/// map was in-memory only, so every app restart lost it, and `resume_session`
/// would re-capture a fresh baseline from the already-dirty tree, silently
/// swallowing the session's own prior edits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBaseline {
    /// `git rev-parse HEAD` in the session's `cwd` at capture time; empty
    /// string if the repo has no HEAD yet (e.g. freshly initialized).
    pub head_sha: String,
    /// Dirty paths (from `git status --porcelain`) at capture time, mapped
    /// to a `git hash-object` content hash. Deleted paths are recorded with
    /// the sentinel `"deleted"`, and pre-existing untracked directories with
    /// the sentinel `"dir"` (see `capture_session_baseline` in `commands.rs`
    /// for why the "dir" sentinel matters).
    pub files: HashMap<String, String>,
}

/// File name for the persisted baseline map, stored alongside the sessions
/// store (`sessions_dir()`'s parent, i.e. `<data_dir>/agentloop/desktop/`).
const SESSION_BASELINES_FILE: &str = "session_baselines.json";

fn session_baselines_path() -> Option<std::path::PathBuf> {
    let dir = dirs::data_dir()?.join("agentloop").join("desktop");
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }
    Some(dir.join(SESSION_BASELINES_FILE))
}

/// Load the persisted session-baseline map from disk, if present. Any
/// failure (missing file, unreadable, corrupt JSON) yields an empty map
/// rather than an error — a missing/corrupt baseline file just means every
/// session lazily re-captures on next resume, which is safe (see
/// `resume_session`'s doc comment).
pub fn load_session_baselines() -> HashMap<String, SessionBaseline> {
    let Some(path) = session_baselines_path() else {
        return HashMap::new();
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

/// Persist the session-baseline map to disk. Best-effort: any failure is
/// logged and swallowed, since losing a baseline write only degrades the
/// Changes panel's scoping (falls back to lazy re-capture on resume), it
/// never corrupts session data itself.
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
    /// Last logical bounds (x, y, w, h) requested by the frontend for the
    /// browser webview. Re-applied natively on window resize: on macOS the
    /// child NSView is anchored to the window's *bottom-left* (non-flipped
    /// contentView coords, no autoresizing), so height changes slide it up
    /// over the React toolbar until bounds are re-asserted. Sync mutex — read
    /// from the (synchronous) window-event handler.
    pub browser_bounds: SyncMutex<Option<(f64, f64, f64, f64)>>,
    /// Per-session repo baselines for non-isolated sessions, keyed by
    /// session id string. See [`SessionBaseline`]. Hot in-memory cache over
    /// the file persisted via [`save_session_baselines`]; loaded from disk
    /// once at startup in [`AppState::new`] so it survives app restart.
    pub session_baselines: Mutex<HashMap<String, SessionBaseline>>,
    /// Pending Copilot device-flow sessions keyed by opaque session id.
    pub pending_copilot_auth: Mutex<HashMap<String, PendingCopilotAuth>>,
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
            session_baselines: Mutex::new(load_session_baselines()),
            pending_copilot_auth: Mutex::new(HashMap::new()),
        }
    }
}
