use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use agentloop_contracts::SessionId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExecStream {
    Stdout,
    Stderr,
}

pub type ChunkSink = Arc<dyn Fn(ExecStream, &str) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum NetworkPolicy {
    #[default]
    Allowed,
    Denied,
}

#[derive(Clone)]
pub struct ExecSpec {
    pub command: String,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub timeout_ms: u64,
    pub network: NetworkPolicy,
    pub chunk_sink: Option<ChunkSink>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutcome {
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutorHealth {
    pub available: bool,
    pub detail: String,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExecError {
    #[error("execution backend unavailable: {0}")]
    Unavailable(String),
    #[error("{0}")]
    Failed(String),
    #[error("unsupported by this execution backend: {0}")]
    Unsupported(String),
    #[error("timed out after {0} ms")]
    Timeout(u64),
    #[error("cancelled")]
    Cancelled,
}

pub enum ExecOrDemoted {
    Completed(ExecOutcome),
    Demoted {
        accumulated: String,
        entry: BackgroundEntry,
    },
}

#[async_trait]
pub trait Executor: Send + Sync {
    fn id(&self) -> &'static str;

    async fn probe(&self) -> ExecutorHealth;

    async fn exec(
        &self,
        spec: ExecSpec,
        cancel: CancellationToken,
    ) -> Result<ExecOutcome, ExecError>;

    async fn exec_demotable(
        &self,
        spec: ExecSpec,
        cancel: CancellationToken,
    ) -> Result<ExecOrDemoted, ExecError> {
        self.exec(spec, cancel).await.map(ExecOrDemoted::Completed)
    }

    async fn sync_in(&self, _cwd: &Path) -> Result<(), ExecError> {
        Ok(())
    }

    async fn sync_out(&self, _cwd: &Path) -> Result<(), ExecError> {
        Ok(())
    }

    async fn exec_background(&self, _spec: ExecSpec) -> Result<BackgroundSpawn, ExecError> {
        Err(ExecError::Unsupported(
            "this execution backend does not support background processes".to_owned(),
        ))
    }
}

pub struct BackgroundSpawn {
    pub handle: Arc<dyn BackgroundProcess>,
    pub initial_output: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundStatus {
    pub running: bool,
    pub exit_code: Option<i32>,
    pub pid: Option<u32>,
}

#[async_trait]
pub trait BackgroundProcess: Send + Sync {
    fn status(&self) -> BackgroundStatus;

    fn tail_text(&self) -> String;

    async fn kill(&self) -> Result<(), ExecError>;
}

pub struct BackgroundEntry {
    pub command: String,
    pub started_at_ms: u64,
    pub handle: Arc<dyn BackgroundProcess>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundEntrySummary {
    pub id: String,
    pub command: String,
    pub running: bool,
    pub started_at_ms: u64,
    pub exit_code: Option<i32>,
}

#[derive(Default)]
pub struct BackgroundProcessRegistry {
    inner: Mutex<HashMap<SessionId, HashMap<String, BackgroundEntry>>>,
}

impl BackgroundProcessRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, session: SessionId, id: String, entry: BackgroundEntry) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(session)
            .or_default()
            .insert(id, entry);
    }

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

    pub async fn kill(&self, session: &SessionId, id: &str) -> Result<bool, ExecError> {
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

#[derive(Default)]
pub struct DemoteRegistry {
    inner: Mutex<HashMap<SessionId, HashMap<String, tokio_util::sync::CancellationToken>>>,
}

impl DemoteRegistry {
    pub fn new() -> Self {
        Self::default()
    }

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

    pub fn unregister(&self, session: &SessionId, call_id: &str) {
        let mut table = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(byid) = table.get_mut(session) {
            byid.remove(call_id);
            if byid.is_empty() {
                table.remove(session);
            }
        }
    }

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
