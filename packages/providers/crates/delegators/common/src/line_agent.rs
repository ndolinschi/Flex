use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::StreamExt;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{
    AgentCaps, AgentEvent, AgentInfo, Answer, ContentBlock, MessageId, NewSessionParams,
    PermissionDecision, PermissionRequestId, PromptInput, QuestionId, Role, SessionEvent,
    SessionId, SessionMeta, TokenUsage, ToolCall, ToolCallId, ToolCallOrigin, ToolCallStatus,
    ToolCallTiming, ToolOutput, TurnId, TurnOptions, TurnStopReason, TurnSummary, now_ms,
};
use agentloop_core::{Agent, AgentError, EventStream, SessionStore};

use crate::{
    DelegatorEvent, DelegatorHostError, DelegatorProbeStatus, DelegatorProcessSpec,
    DelegatorRunRequest, LineMapper, ProcessHost,
};

/// Everything one external agent contributes to the shared line-oriented
/// delegator runtime: identity, capabilities, how to probe and launch its
/// CLI, and the mapper that normalizes its output into canonical events.
pub trait DelegatorProfile: Send + Sync + 'static {
    type Mapper: LineMapper + Send;

    /// Stable agent key ("claude-code", "copilot", ...).
    fn agent_id(&self) -> &str;

    fn display_name(&self) -> &str;

    fn capabilities(&self) -> AgentCaps;

    /// How to check the CLI is installed (e.g. `--version`).
    fn probe_spec(&self) -> DelegatorProcessSpec;

    /// Build the one-shot run for a prompt.
    fn prompt_request(&self, prompt: String) -> DelegatorRunRequest;

    /// A fresh mapper for one turn's output.
    fn mapper(&self) -> Self::Mapper;

    /// Lines recorded in `EngineInfo.resolution_trace` at session creation.
    fn resolution_note(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Shared runtime for delegators that run an external CLI once per turn and
/// map its line-oriented output into canonical events. Parameterized by a
/// [`DelegatorProfile`] (identity + launch + mapper) and a [`ProcessHost`]
/// (real tokio commands in production, fakes in tests).
pub struct LineDelegatorAgent<P, H> {
    profile: P,
    host: Arc<H>,
    store: Arc<dyn SessionStore>,
    sessions: Mutex<HashMap<SessionId, Arc<DelegatedSessionHandle>>>,
}

impl<P, H> LineDelegatorAgent<P, H>
where
    P: DelegatorProfile,
    H: ProcessHost + 'static,
{
    pub fn new(profile: P, store: Arc<dyn SessionStore>, host: Arc<H>) -> Self {
        Self {
            profile,
            host,
            store,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn profile(&self) -> &P {
        &self.profile
    }

    pub async fn probe(
        &self,
        cancel: CancellationToken,
    ) -> Result<DelegatorProbeStatus, DelegatorHostError> {
        self.host.probe(&self.profile.probe_spec(), cancel).await
    }

    fn handle(&self, id: &SessionId) -> Result<Arc<DelegatedSessionHandle>, AgentError> {
        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(id)
            .cloned()
            .ok_or_else(|| AgentError::SessionNotFound(id.clone()))
    }

    fn install_handle(&self, id: &SessionId, next_seq: u64) -> Arc<DelegatedSessionHandle> {
        let mut sessions = self.sessions.lock().unwrap_or_else(|p| p.into_inner());
        sessions
            .entry(id.clone())
            .or_insert_with(|| {
                Arc::new(DelegatedSessionHandle::new(
                    id.clone(),
                    self.profile.agent_id().to_owned(),
                    self.store.clone(),
                    next_seq,
                ))
            })
            .clone()
    }

    async fn run_turn(
        &self,
        handle: Arc<DelegatedSessionHandle>,
        input: PromptInput,
        _opts: TurnOptions,
    ) -> Result<TurnSummary, AgentError> {
        let turn_id = TurnId::generate();
        let cancel = CancellationToken::new();
        *handle
            .current_cancel
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(cancel.clone());
        let started_at = now_ms();

        handle
            .emit_persistent(
                Some(&turn_id),
                AgentEvent::TurnStarted {
                    turn_id: turn_id.clone(),
                },
            )
            .await?;
        if let Some(command) = &input.command {
            handle
                .emit_persistent(
                    Some(&turn_id),
                    AgentEvent::CommandExpanded {
                        name: command.name.clone(),
                        args: command.args.clone(),
                    },
                )
                .await?;
        }
        handle
            .emit_persistent(
                Some(&turn_id),
                AgentEvent::UserMessage {
                    message_id: MessageId::generate(),
                    content: input.parts.clone(),
                },
            )
            .await?;

        let request = self.profile.prompt_request(input.joined_text());
        let output = match self.host.run(request, cancel.clone()).await {
            Ok(output) => output,
            Err(DelegatorHostError::Cancelled) => {
                let summary = self
                    .complete_turn(
                        &handle,
                        &turn_id,
                        TurnStopReason::Cancelled,
                        0,
                        started_at,
                        TokenUsage::default(),
                        None,
                    )
                    .await?;
                return Ok(summary);
            }
            Err(err) => {
                let message = err.to_string();
                return self.fail_turn(&handle, &turn_id, started_at, message).await;
            }
        };

        if !output.status.success {
            let stderr = output.stderr.trim();
            let code = output
                .status
                .code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "signal".to_owned());
            let detail = if stderr.is_empty() {
                "no error output — check that the CLI is installed and signed in".to_owned()
            } else {
                stderr.to_owned()
            };
            let message = format!(
                "{} exited with status {code}: {detail}",
                self.profile.display_name(),
            );
            return self.fail_turn(&handle, &turn_id, started_at, message).await;
        }

        let mut mapper = self.profile.mapper();
        let mut assistant_message_id = MessageId::generate();
        let mut assistant_started = false;
        let mut assistant_text = String::new();
        let mut tool_calls = HashMap::<ToolCallId, ToolCall>::new();
        let mut stop_reason = TurnStopReason::EndTurn;
        let mut mapped_error: Option<String> = None;
        let mut turn_usage = TokenUsage::default();
        let mut turn_cost: Option<f64> = None;

        for line in output.stdout_lines {
            match mapper.map_line(&line) {
                Ok(events) => {
                    for event in events {
                        match event {
                            DelegatorEvent::AssistantDelta { text } => {
                                if !assistant_started {
                                    assistant_started = true;
                                    assistant_message_id = MessageId::generate();
                                    handle.emit_ephemeral(
                                        Some(&turn_id),
                                        AgentEvent::MessageStarted {
                                            message_id: assistant_message_id.clone(),
                                            role: Role::Assistant,
                                        },
                                    );
                                }
                                assistant_text.push_str(&text);
                                handle.emit_ephemeral(
                                    Some(&turn_id),
                                    AgentEvent::MarkdownDelta {
                                        message_id: assistant_message_id.clone(),
                                        text,
                                    },
                                );
                            }
                            DelegatorEvent::ToolCall {
                                call_id,
                                name,
                                args,
                            } => {
                                self.emit_external_tool_call(
                                    &handle,
                                    &turn_id,
                                    &assistant_message_id,
                                    call_id,
                                    name,
                                    args,
                                    &mut tool_calls,
                                )
                                .await?;
                            }
                            DelegatorEvent::ToolResult { call_id, output } => {
                                self.emit_external_tool_result(
                                    &handle,
                                    &turn_id,
                                    &call_id,
                                    output,
                                    &mut tool_calls,
                                )
                                .await?;
                            }
                            DelegatorEvent::Usage { usage, cost_usd } => {
                                turn_usage = usage;
                                if cost_usd.is_some() {
                                    turn_cost = cost_usd;
                                }
                            }
                            DelegatorEvent::TurnFinished { stop_reason: stop } => {
                                stop_reason = stop;
                            }
                            DelegatorEvent::Error { message } => {
                                mapped_error = Some(message.clone());
                                stop_reason = TurnStopReason::Error;
                                handle
                                    .emit_persistent(
                                        Some(&turn_id),
                                        AgentEvent::SessionError {
                                            error: AgentError::Other(message).to_engine_error(),
                                        },
                                    )
                                    .await?;
                            }
                            DelegatorEvent::Unknown { .. } => {}
                        }
                    }
                }
                Err(err) => {
                    let message = format!("failed to map Claude Code output: {err}");
                    mapped_error = Some(message.clone());
                    stop_reason = TurnStopReason::Error;
                    handle
                        .emit_persistent(
                            Some(&turn_id),
                            AgentEvent::SessionError {
                                error: AgentError::Other(message).to_engine_error(),
                            },
                        )
                        .await?;
                }
            }
        }

        if assistant_started {
            handle
                .emit_persistent(
                    Some(&turn_id),
                    AgentEvent::AssistantMessage {
                        message_id: assistant_message_id,
                        content: vec![ContentBlock::markdown(assistant_text)],
                        model: None,
                        usage: (turn_usage != TokenUsage::default()).then_some(turn_usage),
                    },
                )
                .await?;
        }

        self.finish_external_tools(&handle, &turn_id, stop_reason, &mut tool_calls)
            .await?;
        let summary = self
            .complete_turn(
                &handle,
                &turn_id,
                stop_reason,
                tool_calls.len() as u32,
                started_at,
                turn_usage,
                turn_cost,
            )
            .await?;

        if stop_reason == TurnStopReason::Error {
            Err(AgentError::Other(mapped_error.unwrap_or_else(|| {
                format!(
                    "delegated {} turn ended with an error",
                    self.profile.display_name()
                )
            })))
        } else {
            Ok(summary)
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn emit_external_tool_call(
        &self,
        handle: &DelegatedSessionHandle,
        turn_id: &TurnId,
        message_id: &MessageId,
        call_id: ToolCallId,
        name: String,
        args: serde_json::Value,
        tool_calls: &mut HashMap<ToolCallId, ToolCall>,
    ) -> Result<(), AgentError> {
        let queued_at_ms = now_ms();
        let mut call = ToolCall {
            id: call_id.clone(),
            session_id: handle.id.clone(),
            turn_id: turn_id.clone(),
            message_id: message_id.clone(),
            tool_name: name,
            input: args,
            read_only: false,
            origin: ToolCallOrigin::External {
                agent_id: handle.agent_id.clone(),
            },
            status: ToolCallStatus::Pending,
            timing: ToolCallTiming {
                queued_at_ms,
                ..ToolCallTiming::default()
            },
            result: None,
        };
        handle
            .emit_persistent(
                Some(turn_id),
                AgentEvent::ToolCallUpdated { call: call.clone() },
            )
            .await?;
        call.status = ToolCallStatus::Running;
        call.timing.started_at_ms = Some(now_ms());
        handle
            .emit_persistent(
                Some(turn_id),
                AgentEvent::ToolCallUpdated { call: call.clone() },
            )
            .await?;
        tool_calls.insert(call_id, call);
        Ok(())
    }

    async fn emit_external_tool_result(
        &self,
        handle: &DelegatedSessionHandle,
        turn_id: &TurnId,
        call_id: &ToolCallId,
        output: ToolOutput,
        tool_calls: &mut HashMap<ToolCallId, ToolCall>,
    ) -> Result<(), AgentError> {
        let Some(call) = tool_calls.get_mut(call_id) else {
            return Ok(());
        };
        if !call.status.can_transition_to(&ToolCallStatus::Completed) {
            return Ok(());
        }
        call.status = ToolCallStatus::Completed;
        call.timing.finished_at_ms = Some(now_ms());
        call.result = Some(output);
        handle
            .emit_persistent(
                Some(turn_id),
                AgentEvent::ToolCallUpdated { call: call.clone() },
            )
            .await?;
        Ok(())
    }

    async fn finish_external_tools(
        &self,
        handle: &DelegatedSessionHandle,
        turn_id: &TurnId,
        stop_reason: TurnStopReason,
        tool_calls: &mut HashMap<ToolCallId, ToolCall>,
    ) -> Result<(), AgentError> {
        for call in tool_calls.values_mut() {
            if call.status.is_terminal() {
                continue;
            }
            let next = match stop_reason {
                TurnStopReason::Cancelled => ToolCallStatus::Cancelled,
                TurnStopReason::Error => ToolCallStatus::Failed {
                    error: "delegated agent turn failed".to_owned(),
                },
                _ => ToolCallStatus::Completed,
            };
            if call.status.can_transition_to(&next) {
                call.status = next;
                call.timing.finished_at_ms = Some(now_ms());
                handle
                    .emit_persistent(
                        Some(turn_id),
                        AgentEvent::ToolCallUpdated { call: call.clone() },
                    )
                    .await?;
            }
        }
        Ok(())
    }

    async fn fail_turn(
        &self,
        handle: &DelegatedSessionHandle,
        turn_id: &TurnId,
        started_at: u64,
        message: String,
    ) -> Result<TurnSummary, AgentError> {
        let err = AgentError::Other(message.clone());
        handle
            .emit_persistent(
                Some(turn_id),
                AgentEvent::SessionError {
                    error: err.to_engine_error(),
                },
            )
            .await?;
        let _summary = self
            .complete_turn(
                handle,
                turn_id,
                TurnStopReason::Error,
                0,
                started_at,
                TokenUsage::default(),
                None,
            )
            .await?;
        Err(AgentError::Other(message))
    }

    #[allow(clippy::too_many_arguments)]
    async fn complete_turn(
        &self,
        handle: &DelegatedSessionHandle,
        turn_id: &TurnId,
        stop_reason: TurnStopReason,
        num_tool_calls: u32,
        started_at: u64,
        usage: TokenUsage,
        cost_usd: Option<f64>,
    ) -> Result<TurnSummary, AgentError> {
        let summary = TurnSummary {
            turn_id: turn_id.clone(),
            stop_reason,
            usage,
            cost_usd,
            num_model_calls: 1,
            num_tool_calls,
            duration_ms: now_ms().saturating_sub(started_at),
        };
        handle
            .emit_persistent(
                Some(turn_id),
                AgentEvent::TurnCompleted {
                    turn_id: turn_id.clone(),
                    summary: summary.clone(),
                },
            )
            .await?;
        Ok(summary)
    }
}

#[async_trait]
impl<P, H> Agent for LineDelegatorAgent<P, H>
where
    P: DelegatorProfile,
    H: ProcessHost + 'static,
{
    fn info(&self) -> AgentInfo {
        AgentInfo {
            id: self.profile.agent_id().to_owned(),
            display_name: self.profile.display_name().to_owned(),
            version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        }
    }

    fn capabilities(&self) -> AgentCaps {
        self.profile.capabilities()
    }

    async fn create_session(&self, params: NewSessionParams) -> Result<SessionId, AgentError> {
        let id = SessionId::generate();
        let cwd = params
            .cwd
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let now = now_ms();
        let meta = SessionMeta {
            id: id.clone(),
            title: params.title,
            agent_id: self.profile.agent_id().to_owned(),
            parent_id: None,
            role: None,
            depth: 0,
            provider_session_id: None,
            cwd,
            model: params.model,
            mode: params.mode,
            isolation: None,
            workspace_id: None,
            executor: None,
            base_cwd: None,
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
                    agent_id: self.profile.agent_id().to_owned(),
                    capabilities: self.capabilities(),
                    provider_session_id: None,
                    resolution_trace: self.profile.resolution_note(),
                },
            )
            .await?;
        Ok(id)
    }

    async fn resume_session(&self, id: &SessionId) -> Result<(), AgentError> {
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
        let _guard = gate
            .turn_gate
            .try_lock()
            .map_err(|_| AgentError::TurnInProgress(session.clone()))?;
        let result = self.run_turn(handle.clone(), input, opts).await;
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
        _decision: PermissionDecision,
    ) -> Result<(), AgentError> {
        let _ = self.handle(session)?;
        Err(AgentError::Other(format!(
            "the {} delegator does not expose engine-managed permission requests (pending id {})",
            self.profile.display_name(),
            id
        )))
    }

    async fn respond_question(
        &self,
        session: &SessionId,
        id: QuestionId,
        _answers: Vec<Answer>,
    ) -> Result<(), AgentError> {
        let _ = self.handle(session)?;
        Err(AgentError::Other(format!(
            "the {} delegator does not expose engine-managed user questions (pending id {})",
            self.profile.display_name(),
            id
        )))
    }
}

/// Per-session emit machinery shared by delegator runtimes: append-then-
/// broadcast persistence, ephemeral streaming, turn gating, cancellation.
/// Public so protocol delegators (ACP) that cannot ride [`LineDelegatorAgent`]
/// still reuse the exact same event path.
pub struct DelegatedSessionHandle {
    id: SessionId,
    agent_id: String,
    store: Arc<dyn SessionStore>,
    broadcast: broadcast::Sender<SessionEvent>,
    next_seq: AtomicU64,
    pub(crate) turn_gate: tokio::sync::Mutex<()>,
    pub(crate) current_cancel: Mutex<Option<CancellationToken>>,
}

impl DelegatedSessionHandle {
    pub fn new(
        id: SessionId,
        agent_id: String,
        store: Arc<dyn SessionStore>,
        next_seq: u64,
    ) -> Self {
        let (broadcast, _) = broadcast::channel(1024);
        Self {
            id,
            agent_id,
            store,
            broadcast,
            next_seq: AtomicU64::new(next_seq),
            turn_gate: tokio::sync::Mutex::new(()),
            current_cancel: Mutex::new(None),
        }
    }

    pub fn session_id(&self) -> &SessionId {
        &self.id
    }

    /// Subscribe to this session's live event stream.
    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.broadcast.subscribe()
    }

    pub async fn emit_persistent(
        &self,
        turn_id: Option<&TurnId>,
        payload: AgentEvent,
    ) -> Result<u64, agentloop_core::StoreError> {
        let seq = self
            .store
            .append(&self.id, std::slice::from_ref(&payload))
            .await?;
        self.next_seq.store(seq + 1, Ordering::Relaxed);
        agentloop_core::observe::record_event_metrics(&self.agent_id, &payload);
        let _ = self.broadcast.send(SessionEvent {
            session_id: self.id.clone(),
            seq,
            turn_id: turn_id.cloned(),
            ts_ms: now_ms(),
            payload,
        });
        Ok(seq)
    }

    pub fn emit_ephemeral(&self, turn_id: Option<&TurnId>, payload: AgentEvent) {
        let _ = self.broadcast.send(SessionEvent {
            session_id: self.id.clone(),
            seq: self.next_seq.load(Ordering::Relaxed),
            turn_id: turn_id.cloned(),
            ts_ms: now_ms(),
            payload,
        });
    }
}
