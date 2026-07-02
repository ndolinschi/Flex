use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::StreamExt;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{
    AgentCaps, AgentEvent, AgentInfo, Answer, AttachmentCaps, CancelSupport, ContentBlock,
    McpPassthrough, MessageId, ModelDiscovery, NewSessionParams, PermissionCaps,
    PermissionDecision, PermissionMode, PermissionRequestId, PromptInput, QuestionId,
    ResumeSupport, Role, SessionEvent, SessionId, SessionMeta, StreamingGranularity, TokenUsage,
    ToolCall, ToolCallId, ToolCallOrigin, ToolCallStatus, ToolCallTiming, ToolOutput, TurnId,
    TurnOptions, TurnStopReason, TurnSummary, now_ms,
};
use agentloop_core::{Agent, AgentError, EventStream, SessionStore};
use agentloop_delegator_common::{
    DelegatorEvent, DelegatorHostError, DelegatorProbeStatus, LineMapper, ProcessHost,
};
use agentloop_session::MemoryStore;

use crate::CLAUDE_CODE_AGENT_ID;
use crate::config::ClaudeCodeConfig;
use crate::host::TokioCommandHost;
use crate::mapper::ClaudeCodeLineMapper;

pub struct ClaudeCodeAgent<H = TokioCommandHost> {
    config: ClaudeCodeConfig,
    host: Arc<H>,
    store: Arc<dyn SessionStore>,
    sessions: Mutex<HashMap<SessionId, Arc<DelegatedSessionHandle>>>,
}

impl ClaudeCodeAgent<TokioCommandHost> {
    pub fn new(config: ClaudeCodeConfig, store: Arc<dyn SessionStore>) -> Self {
        Self::with_host(config, store, Arc::new(TokioCommandHost::new()))
    }

    pub fn ephemeral(config: ClaudeCodeConfig) -> Self {
        Self::new(config, Arc::new(MemoryStore::new()))
    }
}

impl<H> ClaudeCodeAgent<H>
where
    H: ProcessHost + 'static,
{
    pub fn with_host(config: ClaudeCodeConfig, store: Arc<dyn SessionStore>, host: Arc<H>) -> Self {
        Self {
            config,
            host,
            store,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub async fn probe(
        &self,
        cancel: CancellationToken,
    ) -> Result<DelegatorProbeStatus, DelegatorHostError> {
        self.host.probe(&self.config.probe_spec(), cancel).await
    }

    fn capabilities_static() -> AgentCaps {
        AgentCaps {
            models: ModelDiscovery::None,
            modes: Vec::new(),
            permissions: PermissionCaps {
                interactive: false,
                modes: vec![PermissionMode::Default],
                tool_scoping: false,
            },
            reasoning_visible: true,
            streaming: StreamingGranularity::TokenDeltas,
            resume: ResumeSupport::None,
            attachments: AttachmentCaps {
                images: false,
                files: false,
            },
            mcp_passthrough: McpPassthrough::None,
            subagents: false,
            cost_reporting: false,
            cancellation: CancelSupport::KillOnly,
            emits_structured_events: true,
            commands: Vec::new(),
        }
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
                    CLAUDE_CODE_AGENT_ID.to_owned(),
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

        let request = self.config.prompt_request(input.joined_text());
        let output = match self.host.run(request, cancel.clone()).await {
            Ok(output) => output,
            Err(DelegatorHostError::Cancelled) => {
                let summary = self
                    .complete_turn(&handle, &turn_id, TurnStopReason::Cancelled, 0, started_at)
                    .await?;
                return Ok(summary);
            }
            Err(err) => {
                let message = err.to_string();
                return self.fail_turn(&handle, &turn_id, started_at, message).await;
            }
        };

        if !output.status.success {
            let message = format!(
                "Claude Code process exited with status {:?}: {}",
                output.status.code,
                output.stderr.trim()
            );
            return self.fail_turn(&handle, &turn_id, started_at, message).await;
        }

        let mut mapper = ClaudeCodeLineMapper::new();
        let mut assistant_message_id = MessageId::generate();
        let mut assistant_started = false;
        let mut assistant_text = String::new();
        let mut tool_calls = HashMap::<ToolCallId, ToolCall>::new();
        let mut stop_reason = TurnStopReason::EndTurn;
        let mut mapped_error: Option<String> = None;

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
                        usage: None,
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
            )
            .await?;

        if stop_reason == TurnStopReason::Error {
            Err(AgentError::Other(mapped_error.unwrap_or_else(|| {
                "delegated Claude Code turn ended with an error".to_owned()
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
                agent_id: CLAUDE_CODE_AGENT_ID.to_owned(),
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
            .complete_turn(handle, turn_id, TurnStopReason::Error, 0, started_at)
            .await?;
        Err(AgentError::Other(message))
    }

    async fn complete_turn(
        &self,
        handle: &DelegatedSessionHandle,
        turn_id: &TurnId,
        stop_reason: TurnStopReason,
        num_tool_calls: u32,
        started_at: u64,
    ) -> Result<TurnSummary, AgentError> {
        let summary = TurnSummary {
            turn_id: turn_id.clone(),
            stop_reason,
            usage: TokenUsage::default(),
            cost_usd: None,
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
impl<H> Agent for ClaudeCodeAgent<H>
where
    H: ProcessHost + 'static,
{
    fn info(&self) -> AgentInfo {
        AgentInfo {
            id: CLAUDE_CODE_AGENT_ID.to_owned(),
            display_name: "Claude Code".to_owned(),
            version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        }
    }

    fn capabilities(&self) -> AgentCaps {
        Self::capabilities_static()
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
            agent_id: CLAUDE_CODE_AGENT_ID.to_owned(),
            parent_id: None,
            provider_session_id: None,
            cwd,
            model: params.model,
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
                    agent_id: CLAUDE_CODE_AGENT_ID.to_owned(),
                    capabilities: self.capabilities(),
                    provider_session_id: None,
                    resolution_trace: vec![format!(
                        "configured Claude Code command `{}`",
                        self.config.program
                    )],
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
            "Claude Code delegator does not expose engine-managed permission requests (pending id {id})"
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
            "Claude Code delegator does not expose engine-managed user questions (pending id {id})"
        )))
    }
}

struct DelegatedSessionHandle {
    id: SessionId,
    agent_id: String,
    store: Arc<dyn SessionStore>,
    broadcast: broadcast::Sender<SessionEvent>,
    next_seq: AtomicU64,
    turn_gate: tokio::sync::Mutex<()>,
    current_cancel: Mutex<Option<CancellationToken>>,
}

impl DelegatedSessionHandle {
    fn new(id: SessionId, agent_id: String, store: Arc<dyn SessionStore>, next_seq: u64) -> Self {
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

    async fn emit_persistent(
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

    fn emit_ephemeral(&self, turn_id: Option<&TurnId>, payload: AgentEvent) {
        let _ = self.broadcast.send(SessionEvent {
            session_id: self.id.clone(),
            seq: self.next_seq.load(Ordering::Relaxed),
            turn_id: turn_id.cloned(),
            ts_ms: now_ms(),
            payload,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use agentloop_core::SessionStore;
    use agentloop_delegator_common::{
        DelegatorExitStatus, DelegatorRunOutput, DelegatorRunRequest,
    };

    #[derive(Debug)]
    struct FakeHost {
        outputs: Mutex<Vec<Result<DelegatorRunOutput, DelegatorHostError>>>,
        requests: Mutex<Vec<DelegatorRunRequest>>,
    }

    impl FakeHost {
        fn new(outputs: Vec<Result<DelegatorRunOutput, DelegatorHostError>>) -> Self {
            Self {
                outputs: Mutex::new(outputs),
                requests: Mutex::new(Vec::new()),
            }
        }

        fn requests(&self) -> Vec<DelegatorRunRequest> {
            self.requests
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone()
        }
    }

    #[async_trait]
    impl ProcessHost for FakeHost {
        async fn probe(
            &self,
            _spec: &agentloop_delegator_common::DelegatorProcessSpec,
            _cancel: CancellationToken,
        ) -> Result<DelegatorProbeStatus, DelegatorHostError> {
            Ok(DelegatorProbeStatus::Installed {
                version: Some("fake".to_owned()),
            })
        }

        async fn run(
            &self,
            request: DelegatorRunRequest,
            _cancel: CancellationToken,
        ) -> Result<DelegatorRunOutput, DelegatorHostError> {
            self.requests
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push(request);
            let mut outputs = self.outputs.lock().unwrap_or_else(|p| p.into_inner());
            if outputs.is_empty() {
                return Err(DelegatorHostError::Io(
                    "fake host has no queued output".to_owned(),
                ));
            }
            outputs.remove(0)
        }
    }

    fn fake_output(lines: Vec<&str>) -> DelegatorRunOutput {
        DelegatorRunOutput {
            stdout_lines: lines.into_iter().map(str::to_owned).collect(),
            stderr: String::new(),
            status: DelegatorExitStatus::success(),
        }
    }

    #[tokio::test]
    async fn fake_process_output_becomes_session_events() {
        let output = fake_output(vec![
            r#"{"type":"assistant_delta","text":"hello"}"#,
            r#"{"type":"tool_call","id":"toolu_1","name":"Read","input":{"file_path":"README.md"}}"#,
            r#"{"type":"tool_result","tool_use_id":"toolu_1","content":"done"}"#,
            r#"{"type":"turn_finished","stop_reason":"end_turn"}"#,
        ]);
        let host = Arc::new(FakeHost::new(vec![Ok(output)]));
        let store = Arc::new(MemoryStore::new());
        let agent =
            ClaudeCodeAgent::with_host(ClaudeCodeConfig::default(), store.clone(), host.clone());

        let session = match agent.create_session(NewSessionParams::default()).await {
            Ok(session) => session,
            Err(err) => panic!("session should be created: {err}"),
        };
        let summary = match agent
            .prompt(
                &session,
                PromptInput::text("say hello"),
                TurnOptions::default(),
            )
            .await
        {
            Ok(summary) => summary,
            Err(err) => panic!("fake prompt should succeed: {err}"),
        };

        assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);
        assert_eq!(summary.num_tool_calls, 1);

        let events = match store.read(&session, 0).await {
            Ok(events) => events,
            Err(err) => panic!("events should read: {err}"),
        };
        assert!(events.iter().any(|(_, event)| matches!(
            event,
            AgentEvent::AssistantMessage { content, .. }
                if content == &vec![ContentBlock::markdown("hello")]
        )));
        assert!(events.iter().any(|(_, event)| matches!(
            event,
            AgentEvent::ToolCallUpdated { call }
                if call.id.as_str() == "toolu_1"
                    && call.status == ToolCallStatus::Completed
                    && call.result.as_ref().map(ToolOutput::render_text).as_deref() == Some("done")
        )));
        assert!(events.iter().any(|(_, event)| matches!(
            event,
            AgentEvent::TurnCompleted { summary, .. }
                if summary.stop_reason == TurnStopReason::EndTurn
        )));

        let requests = host.requests();
        assert!(matches!(
            requests.first(),
            Some(request)
                if request.spec.args.last().map(String::as_str) == Some("say hello")
        ));
    }

    #[tokio::test]
    async fn fake_cancelled_process_completes_turn_without_error() {
        let host = Arc::new(FakeHost::new(vec![Err(DelegatorHostError::Cancelled)]));
        let store = Arc::new(MemoryStore::new());
        let agent = ClaudeCodeAgent::with_host(ClaudeCodeConfig::default(), store, host);
        let session = match agent.create_session(NewSessionParams::default()).await {
            Ok(session) => session,
            Err(err) => panic!("session should be created: {err}"),
        };

        let summary = match agent
            .prompt(&session, PromptInput::text("stop"), TurnOptions::default())
            .await
        {
            Ok(summary) => summary,
            Err(err) => panic!("cancelled process should be a normal turn: {err}"),
        };

        assert_eq!(summary.stop_reason, TurnStopReason::Cancelled);
    }
}
