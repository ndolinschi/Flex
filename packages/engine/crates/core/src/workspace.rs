//! The `Workspaces` trait: provision an isolated working copy for a session,
//! then integrate its changes back or discard them.
//!
//! Like [`crate::store::SessionStore`], this is an edge contract: `core`
//! defines *what* isolation is; the mechanism (spawning `git`, moving files)
//! lives in an implementation crate. The trait is deliberately **stateless** —
//! every method takes the concrete paths it needs rather than an in-memory
//! handle — so a fresh process can integrate or discard a workspace it did not
//! create (e.g. after a resume).

use std::path::{Path, PathBuf};

use async_trait::async_trait;

use agentloop_contracts::{IntegrationOutcome, IsolationPolicy, SessionId};

/// Isolation failures.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WorkspaceError {
    /// The base directory is not inside a usable git repository.
    #[error("{0} is not inside a git repository")]
    NotAGitRepo(PathBuf),
    /// `git` could not be run at all.
    #[error("git is unavailable: {0}")]
    GitUnavailable(String),
    /// A git invocation exited non-zero.
    #[error("git failed: {0}")]
    GitFailed(String),
    /// The named workspace could not be located on disk.
    #[error("workspace at {0} not found")]
    NotFound(PathBuf),
    /// Filesystem or process I/O failure.
    #[error("workspace I/O failure: {0}")]
    Io(String),
}

/// A provisioned isolated working copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    /// Stable identifier, persisted in `SessionMeta.workspace_id`.
    pub id: String,
    /// Root directory the session's tools operate in (`SessionMeta.cwd`).
    pub root: PathBuf,
    /// The base commit/ref the workspace was branched from.
    pub base_ref: String,
}

/// Snapshot of what a workspace currently contains, for the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceStatus {
    /// Number of files changed relative to the base.
    pub files_changed: u32,
    /// Human-readable summary (e.g. a `git diff --stat` tail).
    pub summary: String,
}

/// Provisions and integrates isolated working copies. Implementations are the
/// sanctioned I/O edge for this concern (they spawn `git`); `loop`/`engine`
/// only call this trait.
#[async_trait]
pub trait Workspaces: Send + Sync {
    /// Provision an isolated working copy of `base` for `session`.
    ///
    /// Returns `Ok(Some(_))` on success. Returns `Ok(None)` when `base` cannot
    /// be isolated (e.g. not a git repo) and `policy` permits falling back
    /// ([`IsolationPolicy::Optional`]). Returns `Err` when isolation cannot be
    /// provisioned and `policy` is [`IsolationPolicy::Required`], or on an
    /// unexpected failure.
    async fn provision(
        &self,
        base: &Path,
        session: &SessionId,
        policy: IsolationPolicy,
    ) -> Result<Option<Workspace>, WorkspaceError>;

    /// Report the pending changes in the workspace rooted at `root`.
    async fn status(&self, root: &Path) -> Result<WorkspaceStatus, WorkspaceError>;

    /// Commit the workspace's changes, run `verify` (if given) inside it, and
    /// on success integrate them back into `base`, tearing the workspace down.
    /// On verify failure or divergence the workspace is left in place and the
    /// outcome describes what happened.
    async fn integrate(
        &self,
        root: &Path,
        base: &Path,
        verify: Option<&str>,
    ) -> Result<IntegrationOutcome, WorkspaceError>;

    /// Discard the workspace rooted at `root` without integrating. Idempotent:
    /// succeeds even if the workspace is already gone.
    async fn discard(&self, root: &Path, base: &Path) -> Result<(), WorkspaceError>;

    /// Capture the current working-tree state at `root` as a restorable
    /// snapshot, without disturbing `HEAD`, the index, or any branch. Returns
    /// `Ok(Some(id))` with an opaque snapshot id, or `Ok(None)` when snapshots
    /// are unavailable (e.g. `root` is not a git repo, or has no commit yet) so
    /// the caller can silently skip — the availability gate for per-turn
    /// snapshots. Restorable via [`Workspaces::restore`].
    ///
    /// Snapshots cover tracked files (and staged changes); brand-new untracked
    /// files are not captured and so are never removed by a later restore.
    async fn snapshot(&self, root: &Path, label: &str) -> Result<Option<String>, WorkspaceError>;

    /// Restore the working tree at `root` to a snapshot previously returned by
    /// [`Workspaces::snapshot`]. Overwrites tracked files (and drops files that
    /// became tracked after the snapshot) to match it, **without** moving `HEAD`
    /// or any branch — the restored state simply appears as pending changes.
    /// Fails with [`WorkspaceError::NotFound`] if the snapshot id is unknown.
    async fn restore(&self, root: &Path, snapshot_id: &str) -> Result<(), WorkspaceError>;

    /// List provisioned workspaces that belong to `base` and are still on
    /// disk. Empty when the backend cannot enumerate (`base` not a git repo,
    /// no worktrees provisioned yet); errors are reserved for hard I/O
    /// failures. Used by the UI's reuse-workspace picker.
    async fn list(&self, base: &Path) -> Result<Vec<Workspace>, WorkspaceError>;

    /// Attach an existing workspace by `workspace_id` to `session` instead of
    /// provisioning a fresh one. Semantics mirror [`Self::provision`]:
    /// `Ok(Some(_))` when the named workspace exists and can be attached,
    /// `Ok(None)` when it's missing and `policy` is
    /// [`IsolationPolicy::Optional`], `Err` when it's missing (or attachment
    /// fails) and `policy` is [`IsolationPolicy::Required`].
    async fn attach(
        &self,
        base: &Path,
        workspace_id: &str,
        session: &SessionId,
        policy: IsolationPolicy,
    ) -> Result<Option<Workspace>, WorkspaceError>;

    /// Maximum live worktrees allowed per base project before
    /// [`Self::provision`] refuses to create a new one. Default: 5. The
    /// caller-visible signal on hitting the cap is a
    /// [`WorkspaceError::GitFailed`] whose message names the cap and
    /// suggests reusing or discarding an existing workspace.
    fn max_per_base(&self) -> usize {
        5
    }
}
