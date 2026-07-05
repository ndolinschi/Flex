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
    /// Where fake workspace roots are rooted (never touched on disk).
    root_prefix: PathBuf,
    /// Canned result for `integrate`.
    integrate_outcome: IntegrationOutcome,
    provision_calls: AtomicUsize,
    integrate_calls: AtomicUsize,
    discard_calls: AtomicUsize,
}

impl Default for MockWorkspaces {
    fn default() -> Self {
        Self {
            available: true,
            root_prefix: PathBuf::from("/tmp/mock-workspaces"),
            integrate_outcome: IntegrationOutcome::Merged { files_changed: 1 },
            provision_calls: AtomicUsize::new(0),
            integrate_calls: AtomicUsize::new(0),
            discard_calls: AtomicUsize::new(0),
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
}
