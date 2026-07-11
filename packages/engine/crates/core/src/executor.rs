//! The `Executor` trait: run shell commands through a pluggable execution
//! backend (local process, container, remote host, …).
//!
//! Like [`crate::workspace::Workspaces`], this is an edge contract: `core`
//! defines *what* command execution is; the mechanism (spawning `/bin/sh`,
//! `docker`, `ssh`, …) lives in an implementation crate. The trait is
//! deliberately **stateless** — every call carries the concrete spec it needs —
//! so backends can be shared across sessions and swapped at composition time.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use agentloop_contracts::SessionId;

/// Which stream an incremental chunk (see [`ChunkSink`]) came from. Mirrors
/// `agentloop_contracts::ExecStream`; kept as a local, wire-format-free type so
/// this crate (and `executors`, which depends on it) never needs to know about
/// `contracts`' serde/schema shape — callers in the `tools` layer map this to
/// the wire enum when emitting events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExecStream {
    Stdout,
    Stderr,
}

/// Callback for incremental output as a command runs. Invoked once per chunk
/// (not per line) with the stream it came from and a lossy-UTF8 decoded
/// fragment. Implementations must be cheap and non-blocking (typically:
/// forward into an [`crate::event_sink::EventSink`]) — it runs inline in the
/// backend's read loop.
pub type ChunkSink = Arc<dyn Fn(ExecStream, &str) + Send + Sync>;

/// Whether the executed command may reach the network. Enforcement is
/// best-effort and backend-specific: a container backend can drop the network
/// namespace, a local backend cannot and treats `Denied` as unsupported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum NetworkPolicy {
    /// The command may use the network (default).
    #[default]
    Allowed,
    /// The backend must isolate the command from the network; backends that
    /// cannot honor this fail the call with [`ExecError::Unsupported`].
    Denied,
}

/// One command to execute. Commands run with `sh -lc` semantics in `cwd`
/// (backends map the host path into their own filesystem view).
#[derive(Clone)]
pub struct ExecSpec {
    /// The shell command line.
    pub command: String,
    /// Host-side working directory of the session. Backends that execute
    /// elsewhere (container, remote host) map or sync this path.
    pub cwd: PathBuf,
    /// Extra environment variables set for the command.
    pub env: Vec<(String, String)>,
    /// Wall-clock budget for the command.
    pub timeout_ms: u64,
    /// Network posture the command must run under.
    pub network: NetworkPolicy,
    /// Optional sink for incremental stdout/stderr chunks as the command
    /// runs. `None` (the default via [`ExecSpec::new`]/construction without
    /// this field) preserves the historical behavior of only returning
    /// output once the command finishes. Backends that funnel through
    /// `agentloop_executors::run_command` honor this automatically;
    /// backends that don't (e.g. [`Executor`] impls yet to add streaming)
    /// silently ignore it rather than erroring.
    pub chunk_sink: Option<ChunkSink>,
    /// Optional demote signal for a **foreground** (non-`run_in_background`)
    /// call: when set, a backend that supports mid-run handoff (the local
    /// backend; see [`Executor::exec_demotable`]) races this token alongside
    /// its own cancel/timeout waits and, if it fires first, hands the child
    /// process off to a [`BackgroundEntry`] and returns
    /// [`ExecOrDemoted::Demoted`] instead of waiting for exit. `None`
    /// (the default) preserves plain [`Executor::exec`] semantics exactly.
    /// Backends that don't implement demote ignore this field entirely.
    pub demote: Option<tokio_util::sync::CancellationToken>,
}

impl std::fmt::Debug for ExecSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecSpec")
            .field("command", &self.command)
            .field("cwd", &self.cwd)
            .field("env", &self.env)
            .field("timeout_ms", &self.timeout_ms)
            .field("network", &self.network)
            .field("chunk_sink", &self.chunk_sink.is_some())
            .field("demote", &self.demote.is_some())
            .finish()
    }
}

/// What an executed command produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutcome {
    /// Process exit code; `None` when terminated by a signal.
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Result of probing a backend's availability, surfaced by diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutorHealth {
    /// Whether the backend can execute commands right now.
    pub available: bool,
    /// Human-readable detail (version string, missing binary, auth state, …).
    pub detail: String,
}

/// Execution failures.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExecError {
    /// The backend cannot run at all (missing binary, unreachable host).
    #[error("execution backend unavailable: {0}")]
    Unavailable(String),
    /// The command could not be started or its output could not be collected.
    #[error("{0}")]
    Failed(String),
    /// The spec asked for something this backend cannot honor (e.g. network
    /// denial on a backend without network isolation).
    #[error("unsupported by this execution backend: {0}")]
    Unsupported(String),
    #[error("timed out after {0} ms")]
    Timeout(u64),
    #[error("cancelled")]
    Cancelled,
}

/// What a demotable foreground [`Executor::exec`] call hands back: either it
/// ran to completion as usual, or it was asked (via a [`DemoteRegistry`]
/// signal) to move to the background mid-run and is handing the caller a
/// live [`BackgroundEntry`] to register plus the output accumulated before
/// the handoff.
pub enum ExecOrDemoted {
    /// Ran to completion (or failed/timed out/cancelled) without ever being
    /// asked to demote — byte-identical to the pre-demote [`Executor::exec`]
    /// contract.
    Completed(ExecOutcome),
    /// Demoted mid-run: the child process and its reader tasks now live
    /// behind `entry.handle`, already registered nowhere yet (the caller —
    /// `Bash` — inserts it into the shared [`BackgroundProcessRegistry`]
    /// under the same call id it was running under). `accumulated` is
    /// stdout+stderr interleaved in arrival order, exactly what the model
    /// saw stream past before the handoff.
    Demoted {
        accumulated: String,
        entry: BackgroundEntry,
    },
}

/// A pluggable command-execution backend. Implementations are the sanctioned
/// I/O edge for this concern (they spawn processes or talk to daemons);
/// `loop`/`tools` only call this trait.
#[async_trait]
pub trait Executor: Send + Sync {
    /// Stable backend identifier (`"local"`, `"docker"`, `"ssh"`, …), recorded
    /// in session metadata and matched by permission policy.
    fn id(&self) -> &'static str;

    /// Report whether the backend can execute commands right now. Cheap enough
    /// to call from interactive diagnostics.
    async fn probe(&self) -> ExecutorHealth;

    /// Execute one command to completion, honoring `cancel` and
    /// `spec.timeout_ms`. Implementations must be cancel-safe.
    async fn exec(
        &self,
        spec: ExecSpec,
        cancel: CancellationToken,
    ) -> Result<ExecOutcome, ExecError>;

    /// Same as [`Self::exec`], but also races `spec.demote` (if set) and, on
    /// that firing first, hands the still-running process off to a
    /// [`BackgroundEntry`] instead of waiting for it to exit — see
    /// [`ExecOrDemoted`].
    ///
    /// Default: delegates straight to [`Self::exec`] and ignores
    /// `spec.demote` entirely, i.e. always [`ExecOrDemoted::Completed`] —
    /// correct for any backend that can't detach a running process (the same
    /// backends that don't override [`Self::exec_background`]: docker, ssh,
    /// …). Callers (the `Bash` tool via [`DemoteRegistry`]) must not assume a
    /// registered demote request will actually be honored; `request_demote`
    /// returning `true` only means a *foreground local-backend* call
    /// observed the signal, and even then the race could resolve the other
    /// way (natural completion just before the signal is polled).
    async fn exec_demotable(
        &self,
        spec: ExecSpec,
        cancel: CancellationToken,
    ) -> Result<ExecOrDemoted, ExecError> {
        self.exec(spec, cancel).await.map(ExecOrDemoted::Completed)
    }

    /// Push the session's working tree to the backend before a turn's shell
    /// commands run. No-op for backends that see the host filesystem.
    async fn sync_in(&self, _cwd: &Path) -> Result<(), ExecError> {
        Ok(())
    }

    /// Pull changes the backend made back to the host after a turn's shell
    /// commands ran. No-op for backends that see the host filesystem.
    async fn sync_out(&self, _cwd: &Path) -> Result<(), ExecError> {
        Ok(())
    }

    /// Start `spec` and return immediately once initial output has been
    /// collected (the backend decides how long to wait — see
    /// `agentloop_executors::local` for the local backend's deterministic
    /// rule: up to ~3s, or sooner once a quiet gap follows first output),
    /// rather than waiting for the command to exit. `spec.chunk_sink`, if
    /// set, keeps receiving stdout/stderr chunks for the process's entire
    /// lifetime, not just the initial window covered by
    /// [`BackgroundSpawn::initial_output`].
    ///
    /// Default: unsupported. Backends that can't detach a process (or that
    /// have no meaningful notion of "still running", e.g. a stateless
    /// serverless-function backend) simply don't override this; callers
    /// (the `Bash` tool) surface [`ExecError::Unsupported`] to the model as a
    /// normal tool error rather than a hard failure.
    async fn exec_background(&self, _spec: ExecSpec) -> Result<BackgroundSpawn, ExecError> {
        Err(ExecError::Unsupported(
            "this execution backend does not support background processes".to_owned(),
        ))
    }
}

/// What starting a background process hands back: the live handle plus
/// whatever text arrived during the initial-output window.
pub struct BackgroundSpawn {
    pub handle: Arc<dyn BackgroundProcess>,
    pub initial_output: String,
}

/// A live snapshot of a background process's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundStatus {
    /// `None` while still running; `Some(code)` once exited (mirrors
    /// [`ExecOutcome::exit_code`] — `None` inside `Some` would mean
    /// terminated-by-signal, which is why this is `Option<Option<i32>>`
    /// conceptually, but we keep it flat: `running` disambiguates).
    pub running: bool,
    pub exit_code: Option<i32>,
    /// OS process id, when known (some backends, e.g. remote ones, may not
    /// expose one).
    pub pid: Option<u32>,
}

/// Handle to a still-or-formerly-running background process, returned by
/// [`Executor::exec_background`]. Implementations must be cheap to poll and
/// safe to call from any task — the [`crate::BackgroundProcessRegistry`]
/// holds these across the process's whole lifetime, independent of the tool
/// call that started it.
#[async_trait]
pub trait BackgroundProcess: Send + Sync {
    /// Current status. Must reflect reality even after the process exits on
    /// its own (not just when killed through [`Self::kill`]).
    fn status(&self) -> BackgroundStatus;

    /// A lossy-UTF8 snapshot of the most recent output (both streams
    /// interleaved in arrival order), capped by the backend to a small
    /// bounded size — enough for `Bash`'s `status` action to show "what's it
    /// doing" without re-streaming the whole history.
    fn tail_text(&self) -> String;

    /// Terminate the process if still running. Idempotent: killing an
    /// already-exited process is a no-op success.
    async fn kill(&self) -> Result<(), ExecError>;
}

/// One tracked background process: the live handle plus the bookkeeping the
/// `Bash` tool's `status`/`kill` control surface and session teardown need.
/// Pure bookkeeping — no I/O of its own; all I/O is behind
/// [`BackgroundProcess`]. The handle is `Arc`-shared (not boxed) so
/// [`BackgroundProcessRegistry::kill`] can clone it out and call the async
/// `kill` without holding the registry's sync lock across an await point.
pub struct BackgroundEntry {
    pub command: String,
    pub started_at_ms: u64,
    pub handle: Arc<dyn BackgroundProcess>,
}

/// Snapshot of one tracked background process, as returned by
/// [`BackgroundProcessRegistry::list`] — everything a "background processes"
/// panel needs to render a row without a separate `status` call per id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundEntrySummary {
    /// The caller-chosen id it was [`BackgroundProcessRegistry::insert`]-ed
    /// under (the originating tool call id, in practice).
    pub id: String,
    pub command: String,
    pub running: bool,
    pub started_at_ms: u64,
    /// `Some(code)` once exited; `None` while still running or if it was
    /// killed via a signal (mirrors [`BackgroundStatus::exit_code`]).
    pub exit_code: Option<i32>,
}

/// Per-session table of background processes started via `Bash`'s
/// `run_in_background`, keyed by a caller-chosen process id (the originating
/// tool call id, in practice). Lives for the lifetime of the composition
/// root that owns it (one instance shared by the `Bash` tool and whatever
/// owns session teardown) — session-scoped only in the sense that entries
/// are grouped and killed by [`SessionId`].
#[derive(Default)]
pub struct BackgroundProcessRegistry {
    inner: Mutex<HashMap<SessionId, HashMap<String, BackgroundEntry>>>,
}

impl BackgroundProcessRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Track a freshly started background process under `session`/`id`.
    pub fn insert(&self, session: SessionId, id: String, entry: BackgroundEntry) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(session)
            .or_default()
            .insert(id, entry);
    }

    /// Current status, command line, and tail output for `id` in `session`,
    /// if known.
    pub fn status(
        &self,
        session: &SessionId,
        id: &str,
    ) -> Option<(BackgroundStatus, String, String)> {
        let table = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        table.get(session)?.get(id).map(|entry| {
            (
                entry.handle.status(),
                entry.command.clone(),
                entry.handle.tail_text(),
            )
        })
    }

    /// Enumerate every background process tracked for `session`, most
    /// recently started or not — order is whatever the underlying map
    /// yields, callers sort if they need a stable order. Returns an empty
    /// `Vec` for a session with no entries (never `None`); this is a listing
    /// operation, not a lookup, so "no processes" and "unknown session" are
    /// the same observable outcome.
    pub fn list(&self, session: &SessionId) -> Vec<BackgroundEntrySummary> {
        let table = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        table
            .get(session)
            .map(|byid| {
                byid.iter()
                    .map(|(id, entry)| {
                        let status = entry.handle.status();
                        BackgroundEntrySummary {
                            id: id.clone(),
                            command: entry.command.clone(),
                            running: status.running,
                            started_at_ms: entry.started_at_ms,
                            exit_code: status.exit_code,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Kill one process by id. Returns `false` if `session`/`id` is unknown
    /// (already reaped or never existed — not an error, the model can be
    /// told plainly).
    pub async fn kill(&self, session: &SessionId, id: &str) -> Result<bool, ExecError> {
        // Clone the `Arc` handle out from under the lock (kill is async; the
        // registry lock is sync), then drop the lock before awaiting.
        let handle = {
            let table = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            table
                .get(session)
                .and_then(|byid| byid.get(id))
                .map(|entry| entry.handle.clone())
        };
        match handle {
            Some(handle) => handle.kill().await.map(|()| true),
            None => Ok(false),
        }
    }

    /// Kill every background process tracked for `session` and drop them
    /// from the table. Used on session delete; never called on cancel
    /// (cancel aborts the in-flight turn, not processes it started —
    /// documented on `Bash`'s `run_in_background`).
    pub async fn kill_session(&self, session: &SessionId) {
        let entries = {
            let mut table = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            table.remove(session)
        };
        let Some(entries) = entries else {
            return;
        };
        for (_, entry) in entries {
            let _ = entry.handle.kill().await;
        }
    }

    /// Kill everything tracked, across every session. Used on engine drop.
    pub async fn kill_all(&self) {
        let sessions: Vec<SessionId> = {
            let table = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            table.keys().cloned().collect()
        };
        for session in sessions {
            self.kill_session(&session).await;
        }
    }
}

/// Per-call demote signal: lets an out-of-band caller (the `background_demote`
/// Tauri command, ultimately) ask a still-running **foreground** `Bash` call
/// to hand its child process off to the [`BackgroundProcessRegistry`] and
/// return early, instead of blocking to completion. Deliberately separate
/// from [`BackgroundProcessRegistry`] (which tracks processes *after* they're
/// backgrounded): this table only ever holds entries for the brief window
/// between "the foreground exec started" and "it noticed the demote (or
/// exited naturally, whichever comes first)" — entries are removed the
/// instant either happens.
///
/// Only backends that implement handoff (the local backend) register
/// anything here; requesting a demote for a call some other backend is
/// running (or a call unknown to the table) simply returns `false` — the
/// caller treats this identically to "already finished" (no error, nothing to
/// undo).
#[derive(Default)]
pub struct DemoteRegistry {
    inner: Mutex<HashMap<SessionId, HashMap<String, tokio_util::sync::CancellationToken>>>,
}

impl DemoteRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a demote handle for a call that's about to start its
    /// blocking exec. Returns the token the exec's select loop watches
    /// alongside cancel/timeout; the entry is removed by
    /// [`Self::unregister`] once the call finishes one way or the other
    /// (demoted or naturally completed) so a stale entry can never outlive
    /// its call.
    pub fn register(
        &self,
        session: SessionId,
        call_id: String,
    ) -> tokio_util::sync::CancellationToken {
        let token = tokio_util::sync::CancellationToken::new();
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(session)
            .or_default()
            .insert(call_id, token.clone());
        token
    }

    /// Drop the bookkeeping entry for a call that just finished (demoted or
    /// not). Idempotent.
    pub fn unregister(&self, session: &SessionId, call_id: &str) {
        let mut table = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(byid) = table.get_mut(session) {
            byid.remove(call_id);
            if byid.is_empty() {
                table.remove(session);
            }
        }
    }

    /// Ask a running call to demote itself. Returns `true` if a live
    /// registration was found and signalled (the exec loop will observe it on
    /// its next poll and hand off); `false` if there's nothing to signal —
    /// unknown call, backend that never registers (docker/ssh), or the call
    /// already finished naturally. `false` is not an error: the model/UI
    /// should treat it as "nothing to do."
    pub fn request_demote(&self, session: &SessionId, call_id: &str) -> bool {
        let token = {
            let table = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            table
                .get(session)
                .and_then(|byid| byid.get(call_id))
                .cloned()
        };
        match token {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    /// Minimal in-memory [`BackgroundProcess`] for exercising the registry
    /// without spawning a real child process — `kill` just flips a flag that
    /// `status` reflects afterward.
    struct FakeProcess {
        running: AtomicBool,
    }

    #[async_trait]
    impl BackgroundProcess for FakeProcess {
        fn status(&self) -> BackgroundStatus {
            let running = self.running.load(Ordering::SeqCst);
            BackgroundStatus {
                running,
                exit_code: if running { None } else { Some(0) },
                pid: None,
            }
        }

        fn tail_text(&self) -> String {
            String::new()
        }

        async fn kill(&self) -> Result<(), ExecError> {
            self.running.store(false, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn list_reflects_running_then_killed_state() {
        let registry = BackgroundProcessRegistry::new();
        let session = SessionId::from("s1");
        registry.insert(
            session.clone(),
            "call-1".to_string(),
            BackgroundEntry {
                command: "sleep 100".to_string(),
                started_at_ms: 1_000,
                handle: Arc::new(FakeProcess {
                    running: AtomicBool::new(true),
                }),
            },
        );

        let entries = registry.list(&session);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "call-1");
        assert_eq!(entries[0].command, "sleep 100");
        assert_eq!(entries[0].started_at_ms, 1_000);
        assert!(entries[0].running);
        assert_eq!(entries[0].exit_code, None);

        let killed = registry.kill(&session, "call-1").await.unwrap();
        assert!(killed);

        let entries = registry.list(&session);
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].running);
        assert_eq!(entries[0].exit_code, Some(0));
    }

    #[tokio::test]
    async fn list_empty_for_unknown_session() {
        let registry = BackgroundProcessRegistry::new();
        let session = SessionId::from("unknown");
        assert!(registry.list(&session).is_empty());
    }

    #[test]
    fn demote_registry_signals_a_registered_call_exactly_once() {
        let registry = DemoteRegistry::new();
        let session = SessionId::from("s1");
        let token = registry.register(session.clone(), "call-1".to_string());
        assert!(!token.is_cancelled());

        assert!(registry.request_demote(&session, "call-1"));
        assert!(token.is_cancelled());

        // Unregistering (as the caller does once the call finishes one way
        // or the other) makes a further request a no-op.
        registry.unregister(&session, "call-1");
        assert!(!registry.request_demote(&session, "call-1"));
    }

    #[test]
    fn demote_registry_unknown_call_is_a_noop() {
        let registry = DemoteRegistry::new();
        let session = SessionId::from("s1");
        assert!(!registry.request_demote(&session, "no-such-call"));
    }
}
