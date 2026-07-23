use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;

use agentloop_contracts::{IntegrationOutcome, IsolationPolicy, SessionId};
use agentloop_core::workspace::{Workspace, WorkspaceError, WorkspaceStatus, Workspaces};

pub struct MockWorkspaces {
    available: bool,
    snapshots_available: bool,
    root_prefix: PathBuf,
    integrate_outcome: IntegrationOutcome,
    provisioned: Mutex<Vec<(PathBuf, Workspace)>>,
    max_per_base: usize,
    provision_calls: AtomicUsize,
    integrate_calls: AtomicUsize,
    discard_calls: AtomicUsize,
    snapshot_calls: AtomicUsize,
    restore_calls: AtomicUsize,
    attach_calls: AtomicUsize,
    list_calls: AtomicUsize,
}

impl Default for MockWorkspaces {
    fn default() -> Self {
        Self {
            available: true,
            snapshots_available: true,
            root_prefix: PathBuf::from("/tmp/mock-workspaces"),
            integrate_outcome: IntegrationOutcome::Merged { files_changed: 1 },
            provisioned: Mutex::new(Vec::new()),
            max_per_base: 5,
            provision_calls: AtomicUsize::new(0),
            integrate_calls: AtomicUsize::new(0),
            discard_calls: AtomicUsize::new(0),
            snapshot_calls: AtomicUsize::new(0),
            restore_calls: AtomicUsize::new(0),
            attach_calls: AtomicUsize::new(0),
            list_calls: AtomicUsize::new(0),
        }
    }
}

impl MockWorkspaces {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn unavailable() -> Self {
        Self {
            available: false,
            ..Self::default()
        }
    }

    pub fn without_snapshots(mut self) -> Self {
        self.snapshots_available = false;
        self
    }

    pub fn with_integrate_outcome(mut self, outcome: IntegrationOutcome) -> Self {
        self.integrate_outcome = outcome;
        self
    }

    pub fn with_max_per_base(mut self, cap: usize) -> Self {
        self.max_per_base = cap.max(1);
        self
    }

    pub fn seed_workspace(self, base: impl Into<PathBuf>, workspace: Workspace) -> Self {
        self.provisioned
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((base.into(), workspace));
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
    pub fn attach_calls(&self) -> usize {
        self.attach_calls.load(Ordering::SeqCst)
    }
    pub fn list_calls(&self) -> usize {
        self.list_calls.load(Ordering::SeqCst)
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
        let workspace = Workspace {
            id: format!("ws-{session}"),
            root: self.root_prefix.join(session.to_string()),
            base_ref: "mockbase".to_owned(),
        };
        self.provisioned
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((base.to_path_buf(), workspace.clone()));
        Ok(Some(workspace))
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

    async fn list(&self, base: &Path) -> Result<Vec<Workspace>, WorkspaceError> {
        self.list_calls.fetch_add(1, Ordering::SeqCst);
        let provisioned = self
            .provisioned
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        Ok(provisioned
            .into_iter()
            .filter_map(|(b, w)| (b == base).then_some(w))
            .collect())
    }

    async fn attach(
        &self,
        base: &Path,
        workspace_id: &str,
        _session: &SessionId,
        policy: IsolationPolicy,
    ) -> Result<Option<Workspace>, WorkspaceError> {
        self.attach_calls.fetch_add(1, Ordering::SeqCst);
        if !self.available {
            return if policy.is_required() {
                Err(WorkspaceError::NotAGitRepo(base.to_path_buf()))
            } else {
                Ok(None)
            };
        }
        let found = self
            .provisioned
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .find(|(b, w)| b == base && w.id == workspace_id)
            .map(|(_, w)| w.clone());
        match found {
            Some(w) => Ok(Some(w)),
            None if policy.is_required() => Err(WorkspaceError::NotFound(
                self.root_prefix.join(workspace_id),
            )),
            None => Ok(None),
        }
    }

    fn max_per_base(&self) -> usize {
        self.max_per_base
    }
}
