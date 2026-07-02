//! `NativeAgent`: the engine's own agent-loop implementation of the
//! [`Agent`] trait, over any [`Provider`] + tool registry + session store.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::StreamExt;

use agentloop_contracts::{
    AgentCaps, AgentEvent, AgentInfo, Answer, AttachmentCaps, CancelSupport, CommandInfo,
    McpPassthrough, ModelDiscovery, ModelRef, NewSessionParams, PermissionCaps, PermissionDecision,
    PermissionMode, PermissionRequestId, PromptInput, QuestionId, ResumeSupport, SessionEvent,
    SessionId, SessionMeta, StreamingGranularity, TurnOptions, TurnSummary, now_ms,
};
use agentloop_core::{
    Agent, AgentError, EventStream, Hook, PendingMap, ProviderRegistry, SessionStore, ToolRegistry,
};

use crate::builder::LoopLimits;
use crate::permission::PermissionPolicy;
use crate::session_handle::SessionHandle;
use crate::turn;

/// The native agent loop. Construct with [`NativeAgentBuilder`].
pub struct NativeAgent {
    pub(crate) agent_id: String,
    pub(crate) providers: ProviderRegistry,
    pub(crate) tools: ToolRegistry,
    pub(crate) store: Arc<dyn SessionStore>,
    pub(crate) hooks: Vec<Arc<dyn Hook>>,
    pub(crate) policy: PermissionPolicy,
    pub(crate) limits: LoopLimits,
    pub(crate) system_prompt: String,
    pub(crate) default_model: Option<ModelRef>,
    pub(crate) command_infos: Vec<CommandInfo>,
    pub(crate) sessions: Mutex<HashMap<SessionId, Arc<SessionHandle>>>,
    pub(crate) pending_permissions: PendingMap<PermissionRequestId, PermissionDecision>,
    pub(crate) pending_questions: Arc<PendingMap<QuestionId, Vec<Answer>>>,
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

    fn install_handle(&self, id: &SessionId, next_seq: u64) -> Arc<SessionHandle> {
        let mut sessions = self.sessions.lock().unwrap_or_else(|p| p.into_inner());
        sessions
            .entry(id.clone())
            .or_insert_with(|| {
                Arc::new(SessionHandle::new(
                    id.clone(),
                    self.agent_id.clone(),
                    self.store.clone(),
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
            id: self.agent_id.clone(),
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
            subagents: false,
            cost_reporting: false,
            cancellation: CancelSupport::Graceful,
            emits_structured_events: true,
            commands: self.command_infos.clone(),
        }
    }

    async fn create_session(&self, params: NewSessionParams) -> Result<SessionId, AgentError> {
        let id = SessionId::generate();
        let cwd = match params.cwd {
            Some(cwd) => cwd,
            None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        };
        let now = now_ms();
        let meta = SessionMeta {
            id: id.clone(),
            title: params.title,
            agent_id: self.agent_id.clone(),
            parent_id: None,
            provider_session_id: None,
            cwd,
            model: params.model.or_else(|| self.default_model.clone()),
            mode: params.mode,
            created_at_ms: now,
            updated_at_ms: now,
        };
        self.store.create(meta.clone()).await?;
        let handle = self.install_handle(&id, 0);
        handle
            .emit_persistent(None, AgentEvent::SessionCreated { meta })
            .await?;
        handle
            .emit_persistent(
                None,
                AgentEvent::EngineInfo {
                    agent_id: self.agent_id.clone(),
                    capabilities: self.capabilities(),
                    provider_session_id: None,
                    resolution_trace: Vec::new(),
                },
            )
            .await?;
        Ok(id)
    }

    async fn resume_session(&self, id: &SessionId) -> Result<(), AgentError> {
        // The log is the ground truth: resuming is just re-attaching a
        // handle at the right sequence number.
        let _meta = self.store.get_meta(id).await?;
        let events = self.store.read(id, 0).await?;
        let next_seq = events.last().map(|(seq, _)| seq + 1).unwrap_or(0);
        self.install_handle(id, next_seq);
        Ok(())
    }

    async fn list_sessions(&self) -> Result<Vec<SessionMeta>, AgentError> {
        Ok(self.store.list().await?)
    }

    fn events(&self, session: &SessionId) -> Result<EventStream, AgentError> {
        let handle = self.handle(session).map_err(|_| {
            AgentError::Other(format!(
                "session {session} has no live handle; call resume_session first"
            ))
        })?;
        let session_id = session.clone();
        let rx = handle.broadcast.subscribe();
        let stream = tokio_stream::wrappers::BroadcastStream::new(rx).map(move |item| {
            match item {
                Ok(event) => event,
                // Lagged: tell the subscriber to re-sync from the store.
                Err(_) => SessionEvent {
                    session_id: session_id.clone(),
                    seq: 0,
                    turn_id: None,
                    ts_ms: now_ms(),
                    payload: AgentEvent::Gap { from_seq: 0 },
                },
            }
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
        let _guard = gate
            .turn_gate
            .try_lock()
            .map_err(|_| AgentError::TurnInProgress(session.clone()))?;
        let result = turn::run_turn(self, handle.clone(), input, opts).await;
        *handle
            .current_cancel
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
        result
    }

    async fn cancel(&self, session: &SessionId) -> Result<(), AgentError> {
        let handle = self.handle(session)?;
        let token = handle
            .current_cancel
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        if let Some(token) = token {
            token.cancel();
        }
        Ok(())
    }

    async fn respond_permission(
        &self,
        session: &SessionId,
        id: PermissionRequestId,
        decision: PermissionDecision,
    ) -> Result<(), AgentError> {
        let _ = self.handle(session)?;
        if self.pending_permissions.resolve(&id, decision) {
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
        if self.pending_questions.resolve(&id, answers) {
            Ok(())
        } else {
            Err(AgentError::Other(format!(
                "no pending question with id {id}"
            )))
        }
    }
}
