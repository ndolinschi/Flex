use std::path::{Path, PathBuf};

use async_trait::async_trait;

use agentloop_contracts::{IntegrationOutcome, IsolationPolicy, SessionId};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WorkspaceError {
    #[error("{0} is not inside a git repository")]
    NotAGitRepo(PathBuf),
    #[error("git is unavailable: {0}")]
    GitUnavailable(String),
    #[error("git failed: {0}")]
    GitFailed(String),
    #[error("workspace at {0} not found")]
    NotFound(PathBuf),
    #[error("workspace I/O failure: {0}")]
    Io(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub id: String,
    pub root: PathBuf,
    pub base_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceStatus {
    pub files_changed: u32,
    pub summary: String,
}

#[async_trait]
pub trait Workspaces: Send + Sync {
    async fn provision(
        &self,
        base: &Path,
        session: &SessionId,
        policy: IsolationPolicy,
    ) -> Result<Option<Workspace>, WorkspaceError>;

    async fn status(&self, root: &Path) -> Result<WorkspaceStatus, WorkspaceError>;

    async fn integrate(
        &self,
        root: &Path,
        base: &Path,
        verify: Option<&str>,
    ) -> Result<IntegrationOutcome, WorkspaceError>;

    async fn discard(&self, root: &Path, base: &Path) -> Result<(), WorkspaceError>;

    async fn snapshot(&self, root: &Path, label: &str) -> Result<Option<String>, WorkspaceError>;

    async fn restore(&self, root: &Path, snapshot_id: &str) -> Result<(), WorkspaceError>;

    async fn list(&self, base: &Path) -> Result<Vec<Workspace>, WorkspaceError>;

    async fn attach(
        &self,
        base: &Path,
        workspace_id: &str,
        session: &SessionId,
        policy: IsolationPolicy,
    ) -> Result<Option<Workspace>, WorkspaceError>;

    fn max_per_base(&self) -> usize {
        5
    }
}
