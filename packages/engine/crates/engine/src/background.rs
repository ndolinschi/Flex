//! Background-process panel: list, kill, demote, shutdown.

use agentloop_contracts::SessionId;
use agentloop_core::BackgroundEntrySummary;

use crate::EngineResult;
use crate::service::EngineService;

impl EngineService {
    /// Kill every background process started by any session through
    /// `Bash`'s `run_in_background`, across the whole service. Call this
    /// during process shutdown (the runner binary's signal handler, the
    /// headless HTTP transport's graceful-shutdown path, `EOF` on stdio) —
    /// a spawned child process is owned by a detached task that outlives
    /// any `Arc` clone being dropped (background processes must keep
    /// streaming after their starting tool call returns, by design), so
    /// there is no `Drop` impl that can kill them for you; this must be
    /// called explicitly. No-op for a headless service (no `cwd`, so
    /// `Bash` was never registered).
    pub async fn shutdown(&self) {
        if let Some(registry) = &self.background_processes {
            registry.kill_all().await;
        }
    }

    /// List background processes (started via `Bash`'s `run_in_background`)
    /// tracked for `session`, for a "background processes" panel. Empty for
    /// a headless service (no registry — `Bash` was never registered) or a
    /// session with none running.
    pub fn background_list(&self, session: &SessionId) -> Vec<BackgroundEntrySummary> {
        match &self.background_processes {
            Some(registry) => registry.list(session),
            None => Vec::new(),
        }
    }

    /// Kill one background process by id. Returns `false` if `id` is unknown
    /// for `session` (already reaped or never existed) or if this service
    /// has no background-process registry at all.
    pub async fn background_kill(&self, session: &SessionId, id: &str) -> EngineResult<bool> {
        match &self.background_processes {
            Some(registry) => Ok(registry.kill(session, id).await?),
            None => Ok(false),
        }
    }

    /// Ask a still-running **foreground** `Bash` call to move to the
    /// background (see `MOVE-TO-BACKGROUND`): the tool call returns early
    /// ("moved to background…") and the process keeps running as a tracked
    /// background entry, reachable afterward through
    /// [`Self::background_list`]/[`Self::background_kill`] under the same
    /// `id`. Returns `false` — not an error — when there's nothing to do:
    /// unknown id, the call already finished naturally, or the session's
    /// execution backend doesn't support demote at all (docker, ssh, …;
    /// only the local backend does). Callers (the desktop UI) should treat
    /// `false` as "no visible effect," not surface it as a failure.
    pub fn background_demote(&self, session: &SessionId, id: &str) -> bool {
        match &self.demote_processes {
            Some(registry) => registry.request_demote(session, id),
            None => false,
        }
    }
}
