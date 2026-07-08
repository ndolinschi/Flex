//! The `Executor` trait: run shell commands through a pluggable execution
//! backend (local process, container, remote host, …).
//!
//! Like [`crate::workspace::Workspaces`], this is an edge contract: `core`
//! defines *what* command execution is; the mechanism (spawning `/bin/sh`,
//! `docker`, `ssh`, …) lives in an implementation crate. The trait is
//! deliberately **stateless** — every call carries the concrete spec it needs —
//! so backends can be shared across sessions and swapped at composition time.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

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
#[derive(Debug, Clone)]
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
}
