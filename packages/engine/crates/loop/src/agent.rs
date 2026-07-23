use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::StreamExt;
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{
    AgentCaps, AgentEvent, AgentInfo, Answer, AttachmentCaps, CancelSupport, CommandInfo,
    CompactionSummary, EngineError, ErrorCode, McpPassthrough, ModeSwitchId, ModelDiscovery,
    NewSessionParams, PermissionCaps, PermissionDecision, PermissionMode, PermissionRequestId,
    PromptInput, QuestionId, ResumeSupport, SessionEvent, SessionId, SessionMeta, SessionMetaPatch,
    StreamingGranularity, TurnOptions, TurnSummary, now_ms,
};
use agentloop_core::{Agent, AgentError, EventStream};

use crate::compaction::compact_session;
use crate::deps::TurnDeps;
use crate::session_handle::SessionHandle;
use crate::turn;

pub struct NativeAgent {
    pub(crate) deps: Arc<TurnDeps>,
    pub(crate) command_infos: Vec<CommandInfo>,
    pub(crate) sessions: Mutex<HashMap<SessionId, Arc<SessionHandle>>>,
}

impl NativeAgent {
    fn handle(&self, id: &SessionId) -> Result<Arc<SessionHandle>, AgentError> {
        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(id)
            .cloned()
            .ok_or_else(|| AgentError::SessionNotFound(id.clone()))
    }

    pub(crate) fn live_handle(&self, id: &SessionId) -> Option<Arc<SessionHandle>> {
        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(id)
            .cloned()
    }

    pub(crate) fn install_child_handle(&self, id: &SessionId) -> Arc<SessionHandle> {
        self.install_handle(id, 0)
    }

    #[doc(hidden)]
    pub async fn ensure_workspace_for_test(&self, id: &SessionId) -> Result<(), AgentError> {
        let handle = self.handle(id)?;
        let meta = self.deps.store.get_meta(id).await?;
        crate::workspace_ensure::ensure_root_workspace(&self.deps, &handle, meta).await?;
        Ok(())
    }

    pub(crate) fn spawn_relay(
        &self,
        child: &Arc<SessionHandle>,
        parent: Arc<SessionHandle>,
        child_id: &SessionId,
        stop: &tokio_util::sync::CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        let mut rx = child.broadcast.subscribe();
        let child_id = child_id.clone();
        let stop = stop.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = stop.cancelled() => break,
                    item = rx.recv() => match item {
                        Ok(event) if event.payload.is_persistent() => {
                            parent
                                .emit_ephemeral(
                                    None,
                                    AgentEvent::SubagentEvent {
                                        child_session: child_id.clone(),
                                        event: Box::new(event.payload),
                                    },
                                );
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    },
                }
            }
        })
    }

    fn install_handle(&self, id: &SessionId, next_seq: u64) -> Arc<SessionHandle> {
        let mut sessions = self.sessions.lock().unwrap_or_else(|p| p.into_inner());
        sessions
            .entry(id.clone())
            .or_insert_with(|| {
                Arc::new(SessionHandle::new(
                    id.clone(),
                    self.deps.agent_id.clone(),
                    self.deps.store.clone(),
                    next_seq,
                ))
            })
            .clone()
    }
}

#[async_trait]
impl Agent for NativeAgent {
    fn info(&self) -> AgentInfo {
        AgentInfo {
            id: self.deps.agent_id.clone(),
            display_name: "Native loop".to_owned(),
            version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        }
    }

    fn capabilities(&self) -> AgentCaps {
        AgentCaps {
            models: ModelDiscovery::Dynamic,
            modes: Vec::new(),
            permissions: PermissionCaps {
                interactive: true,
                modes: vec![
                    PermissionMode::Default,
                    PermissionMode::AcceptEdits,
                    PermissionMode::Plan,
                    PermissionMode::DontAsk,
                    PermissionMode::BypassPermissions,
                ],
                tool_scoping: true,
            },
            reasoning_visible: true,
            streaming: StreamingGranularity::TokenDeltas,
            resume: ResumeSupport::Native,
            attachments: AttachmentCaps {
                images: true,
                files: true,
            },
            mcp_passthrough: McpPassthrough::None,
            subagents: true,
            cost_reporting: false,
            cancellation: CancelSupport::Graceful,
            emits_structured_events: true,
            commands: self.command_infos.clone(),
        }
    }

    async fn create_session(&self, params: NewSessionParams) -> Result<SessionId, AgentError> {
        let id = SessionId::generate();
        let base_cwd = match params.cwd {
            Some(cwd) => cwd,
            None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        };
        let now = now_ms();

        let role = params.role;
        if let Some(name) = &role {
            if self.deps.roles.get(name).is_none() {
                return Err(AgentError::Other(format!("unknown role `{name}`")));
            }
        }

        let policy = params
            .isolation
            .unwrap_or_else(|| self.deps.roles.isolation(role.as_deref()));

        let isolation = policy.wants_isolation().then_some(policy);
        let reuse_workspace_id = if policy.wants_isolation() {
            params.reuse_workspace_id
        } else {
            None
        };
        if policy.is_required() && self.deps.workspace.is_none() {
            return Err(AgentError::Other(
                "isolation required but no workspace backend is configured".to_owned(),
            ));
        }

        let meta = SessionMeta {
            id: id.clone(),
            title: params.title,
            agent_id: self.deps.agent_id.clone(),
            parent_id: None,
            depth: 0,
            provider_session_id: None,
            cwd: base_cwd.clone(),
            model: params
                .model
                .or_else(|| self.deps.roles.chain(role.as_deref()).first().cloned())
                .or_else(|| self.deps.default_model.clone()),
            fallback_models: if params.fallback_models.is_empty() {
                self.deps.default_fallback_models.clone()
            } else {
                params.fallback_models
            },
            role,
            mode: params.mode,
            isolation,
            workspace_id: None,
            executor: self.deps.executor_id.clone(),
            base_cwd: None,
            reuse_workspace_id,
            created_at_ms: now,
            updated_at_ms: now,
        };
        self.deps.store.create(meta.clone()).await?;
        let handle = self.install_handle(&id, 0);
        handle
            .emit_persistent(None, AgentEvent::SessionCreated { meta })
            .await?;
        handle
            .emit_persistent(
                None,
                AgentEvent::EngineInfo {
                    agent_id: self.deps.agent_id.clone(),
                    capabilities: self.capabilities(),
                    provider_session_id: None,
                    resolution_trace: Vec::new(),
                },
            )
            .await?;
        Ok(id)
    }

    async fn resume_session(&self, id: &SessionId) -> Result<(), AgentError> {
        let meta = self.deps.store.get_meta(id).await?;
        if meta.workspace_id.is_some() {
            if let Some(base) = meta.base_cwd.filter(|_| !meta.cwd.exists()) {
                self.deps
                    .store
                    .update_meta(
                        id,
                        SessionMetaPatch {
                            cwd: Some(base),
                            ..Default::default()
                        },
                    )
                    .await?;
                tracing::info!(
                    target: "workspace", session = %id,
                    "isolated workspace is gone; resuming in the base directory"
                );
            }
        }
        let events = self.deps.store.read(id, 0).await?;
        let next_seq = events.last().map(|stored| stored.seq + 1).unwrap_or(0);
        self.install_handle(id, next_seq);
        Ok(())
    }

    async fn list_sessions(&self) -> Result<Vec<SessionMeta>, AgentError> {
        Ok(self.deps.store.list().await?)
    }

    fn events(&self, session: &SessionId) -> Result<EventStream, AgentError> {
        let handle = self.handle(session).map_err(|_| {
            AgentError::Other(format!(
                "session {session} has no live handle; call resume_session first"
            ))
        })?;
        let session_id = session.clone();
        let rx = handle.broadcast.subscribe();
        let stream = tokio_stream::wrappers::BroadcastStream::new(rx).map(move |item| match item {
            Ok(event) => event,
            Err(_) => SessionEvent {
                session_id: session_id.clone(),
                seq: 0,
                turn_id: None,
                ts_ms: now_ms(),
                payload: AgentEvent::Gap { from_seq: 0 },
            },
        });
        Ok(Box::pin(stream))
    }

    async fn prompt(
        &self,
        session: &SessionId,
        input: PromptInput,
        opts: TurnOptions,
    ) -> Result<TurnSummary, AgentError> {
        let handle = self.handle(session)?;
        let gate = handle.clone();
        let guard = gate
            .turn_gate
            .try_lock()
            .map_err(|_| AgentError::TurnInProgress(session.clone()))?;
        let deps = self.deps.clone();
        let turn_handle = handle.clone();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let joined_handle =
            tokio::spawn(
                async move { turn::run_turn(&deps, turn_handle, input, opts, done_tx).await },
            );
        let _ = done_rx.await;
        *handle
            .current_cancel
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
        drop(guard);
        let joined = joined_handle.await;
        match joined {
            Ok(result) => result,
            Err(join_err) => {
                let message = if join_err.is_panic() {
                    format!("turn task panicked: {join_err}")
                } else {
                    format!("turn task was aborted: {join_err}")
                };
                let _ = handle
                    .emit_persistent(
                        None,
                        AgentEvent::SessionError {
                            error: EngineError::engine(ErrorCode::Unknown, message.clone()),
                        },
                    )
                    .await;
                Err(AgentError::Other(message))
            }
        }
    }

    async fn cancel(&self, session: &SessionId) -> Result<(), AgentError> {
        let handle = self.handle(session)?;
        let token = handle
            .current_cancel
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        if let Some(token) = token {
            tracing::info!(target: "turn", session_id = %session, "cancel requested");
            token.cancel();
        }
        Ok(())
    }

    fn set_turn_permission_mode(
        &self,
        session: &SessionId,
        mode: Option<PermissionMode>,
    ) -> Result<(), AgentError> {
        let handle = self.handle(session)?;
        handle.set_turn_permission_mode(mode);
        Ok(())
    }

    async fn respond_permission(
        &self,
        session: &SessionId,
        id: PermissionRequestId,
        decision: PermissionDecision,
    ) -> Result<(), AgentError> {
        let _ = self.handle(session)?;
        if self.deps.pending_permissions.resolve(&id, decision) {
            Ok(())
        } else {
            Err(AgentError::UnknownPermissionRequest(id))
        }
    }

    async fn respond_question(
        &self,
        session: &SessionId,
        id: QuestionId,
        answers: Vec<Answer>,
    ) -> Result<(), AgentError> {
        let _ = self.handle(session)?;
        if self.deps.pending_questions.resolve(&id, answers) {
            Ok(())
        } else {
            Err(AgentError::Other(format!(
                "no pending question with id {id}"
            )))
        }
    }

    async fn respond_mode_switch(
        &self,
        session: &SessionId,
        id: ModeSwitchId,
        allow: bool,
    ) -> Result<(), AgentError> {
        let _ = self.handle(session)?;
        if self.deps.pending_mode_switches.resolve(&id, allow) {
            Ok(())
        } else {
            Err(AgentError::Other(format!(
                "no pending mode-switch proposal with id {id}"
            )))
        }
    }

    async fn compact(
        &self,
        session: &SessionId,
        opts: TurnOptions,
    ) -> Result<CompactionSummary, AgentError> {
        let handle = self.handle(session)?;
        let gate = handle.clone();
        let _guard = gate
            .turn_gate
            .try_lock()
            .map_err(|_| AgentError::TurnInProgress(session.clone()))?;
        let cancel = CancellationToken::new();
        *handle
            .current_cancel
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(cancel.clone());
        let result = compact_session(
            &self.deps,
            handle.clone(),
            opts,
            cancel,
            crate::context_budget::MANUAL_COMPACT_STRATEGY,
        )
        .await;
        *handle
            .current_cancel
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
        result
    }
}
