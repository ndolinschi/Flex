//! Shared Tauri app state.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
// Plain (non-async) Mutex for the terminal registry: PTY I/O on the writer
// and resize calls are blocking, so a guard must never be held across an
// `.await` point. Aliased to avoid clashing with the existing tokio Mutex
// import used everywhere else in this struct.
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

/// Soft TTL for the in-process workspace path cache used by `list_files`.
/// Long enough to cover a burst of keystrokes; short enough that new files
/// appear without an explicit invalidate after a few idle seconds.
pub const WORKSPACE_PATH_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// One entry in the warm path list (`list_files` reuses this instead of
/// re-walking the tree on every keystroke).
#[derive(Debug, Clone)]
pub struct CachedPathEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
}

/// Process-level cache of a workspace walk, keyed by
/// `(canonical_root, include_ignored)`.
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

/// In-flight GitHub Copilot device-code sign-in. The full
/// [`DeviceAuthorization`] (including the private `device_code`) stays on
/// this side of the IPC boundary — the frontend only sees the session id
/// plus the public user code / verification URI.
pub struct PendingCopilotAuth {
    pub auth: DeviceAuthorization,
    pub cancel: CancellationToken,
}

/// In-flight ChatGPT Plus/Pro headless device-code sign-in.
pub struct PendingChatgptAuth {
    pub start: OpenAiOAuthStart,
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
    /// browser webview. Used to re-assert position immediately before
    /// `browser_set_visible(true)` so reveal never flashes at a stale frame.
    /// Window resize itself is handled by wry's rate-based child autoresize
    /// (fed by atomic `set_bounds`) — do not re-apply these absolute pixels
    /// from `WindowEvent::Resized` or the page freezes at the old height.
    pub browser_bounds: SyncMutex<Option<(f64, f64, f64, f64)>>,
    /// Whether Design Mode (element picker) is active in the embedded browser.
    /// Sync mutex so `on_page_load` / `on_navigation` callbacks can read it
    /// without awaiting. When true, Finished re-injects the picker script.
    pub browser_design_mode: SyncMutex<bool>,
    /// Per-session repo baselines for non-isolated sessions, keyed by
    /// session id string. See [`SessionBaseline`]. Hot in-memory cache over
    /// the file persisted via [`save_session_baselines`]; loaded from disk
    /// once at startup in [`AppState::new`] so it survives app restart.
    pub session_baselines: Mutex<HashMap<String, SessionBaseline>>,
    /// Pending Copilot device-flow sessions keyed by opaque session id.
    pub pending_copilot_auth: Mutex<HashMap<String, PendingCopilotAuth>>,
    /// Pending ChatGPT subscription OAuth sessions keyed by opaque session id.
    pub pending_chatgpt_auth: Mutex<HashMap<String, PendingChatgptAuth>>,
    /// Database UI plugin — saved connection specs + live handles.
    pub db_plugin: Mutex<crate::db_plugin::DbPluginState>,
    /// Desktop Remote Access HTTP server + connection-method adapters.
    pub remote: Mutex<Option<crate::remote::RemoteServerHandle>>,
    /// Warm path lists for `list_files` (Files search / composer `@`).
    /// Sync mutex: filled and read from `spawn_blocking` / short critical
    /// sections — never held across an `.await`.
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
            pending_copilot_auth: Mutex::new(HashMap::new()),
            pending_chatgpt_auth: Mutex::new(HashMap::new()),
            db_plugin: Mutex::new(crate::db_plugin::DbPluginState::load()),
            remote: Mutex::new(crate::remote::init_remote_server().ok()),
            workspace_path_cache: SyncMutex::new(HashMap::new()),
        }
    }
}
