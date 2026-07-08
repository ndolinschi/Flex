//! A `Workspaces` test double: canned provisioning with call counters, so
//! loop/engine tests can assert isolation behavior without spawning `git`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;

use agentloop_contracts::{IntegrationOutcome, IsolationPolicy, SessionId};
use agentloop_core::workspace::{Workspace, WorkspaceError, WorkspaceStatus, Workspaces};

/// Configurable mock isolation backend.
pub struct MockWorkspaces {
    /// When false, `provision` falls back (`Ok(None)`) for `Optional` and errors
    /// for `Required`, simulating a non-git base directory.
    available: bool,
    /// When false, `snapshot` returns `Ok(None)`, simulating a non-git tree
    /// where per-turn snapshots are silently disabled.
    snapshots_available: bool,
    /// Where fake workspace roots are rooted (never touched on disk).
    root_prefix: PathBuf,
    /// Canned result for `integrate`.
    integrate_outcome: IntegrationOutcome,
    provision_calls: AtomicUsize,
    integrate_calls: AtomicUsize,
    discard_calls: AtomicUsize,
    snapshot_calls: AtomicUsize,
    restore_calls: AtomicUsize,
}

impl Default for MockWorkspaces {
    fn default() -> Self {
        Self {
            available: true,
            snapshots_available: true,
            root_prefix: PathBuf::from("/tmp/mock-workspaces"),
            integrate_outcome: IntegrationOutcome::Merged { files_changed: 1 },
            provision_calls: AtomicUsize::new(0),
            integrate_calls: AtomicUsize::new(0),
            discard_calls: AtomicUsize::new(0),
            snapshot_calls: AtomicUsize::new(0),
            restore_calls: AtomicUsize::new(0),
        }
    }
}

impl MockWorkspaces {
    /// A backend that provisions successfully.
    pub fn new() -> Self {
        Self::default()
    }

    /// A backend that cannot provision (as if the base weren't a git repo).
    pub fn unavailable() -> Self {
        Self {
            available: false,
            ..Self::default()
        }
    }

    /// A backend that provisions but cannot snapshot (as if the tree weren't a
    /// git repo), so per-turn snapshots are silently skipped.
    pub fn without_snapshots(mut self) -> Self {
        self.snapshots_available = false;
        self
    }

    /// Set the outcome `integrate` returns.
    pub fn with_integrate_outcome(mut self, outcome: IntegrationOutcome) -> Self {
        self.integrate_outcome = outcome;
        self
    }

    pub fn provision_calls(&self) -> usize {
        self.provision_calls.load(Ordering::SeqCst)
    }
    pub fn integrate_calls(&self) -> usize {
        self.integrate_calls.load(Ordering::SeqCst)
    }
    pub fn discard_calls(&self) -> usize {
        self.discard_calls.load(Ordering::SeqCst)
    }
    pub fn snapshot_calls(&self) -> usize {
        self.snapshot_calls.load(Ordering::SeqCst)
    }
    pub fn restore_calls(&self) -> usize {
        self.restore_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Workspaces for MockWorkspaces {
    async fn provision(
        &self,
        base: &Path,
        session: &SessionId,
        policy: IsolationPolicy,
    ) -> Result<Option<Workspace>, WorkspaceError> {
        self.provision_calls.fetch_add(1, Ordering::SeqCst);
        if !self.available {
            return if policy.is_required() {
                Err(WorkspaceError::NotAGitRepo(base.to_path_buf()))
            } else {
                Ok(None)
            };
        }
        Ok(Some(Workspace {
            id: format!("ws-{session}"),
            root: self.root_prefix.join(session.to_string()),
            base_ref: "mockbase".to_owned(),
        }))
    }

    async fn status(&self, _root: &Path) -> Result<WorkspaceStatus, WorkspaceError> {
        Ok(WorkspaceStatus {
            files_changed: 0,
            summary: "clean (mock)".to_owned(),
        })
    }

    async fn integrate(
        &self,
        _root: &Path,
        _base: &Path,
        _verify: Option<&str>,
    ) -> Result<IntegrationOutcome, WorkspaceError> {
        self.integrate_calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.integrate_outcome.clone())
    }

    async fn discard(&self, _root: &Path, _base: &Path) -> Result<(), WorkspaceError> {
        self.discard_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn snapshot(&self, _root: &Path, _label: &str) -> Result<Option<String>, WorkspaceError> {
        let n = self.snapshot_calls.fetch_add(1, Ordering::SeqCst);
        if self.snapshots_available {
            Ok(Some(format!("snap-{n}")))
        } else {
            Ok(None)
        }
    }

    async fn restore(&self, _root: &Path, _snapshot_id: &str) -> Result<(), WorkspaceError> {
        self.restore_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
