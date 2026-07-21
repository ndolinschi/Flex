//! `AcpAgent`: drives an ACP-speaking agent (JSON-RPC over the child's stdio)
//! behind the engine's `Agent` interface.
//!
//! Unlike the one-shot line delegators, the child is long-lived: one spawn +
//! `initialize` + `session/new` handshake per session, then one
//! `session/prompt` per turn while `session/update` notifications stream in.
//! Client-side requests (permissions) are auto-answered by policy in v1.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{
    AgentCaps, AgentEvent, AgentInfo, Answer, ContentBlock, MessageId, NewSessionParams,
    PermissionDecision, PermissionRequestId, PromptInput, QuestionId, Role, SessionEvent,
    SessionId, SessionMeta, SessionMetaPatch, TokenUsage, TurnId, TurnOptions, TurnStopReason,
    TurnSummary, now_ms,
};
use agentloop_core::{Agent, AgentError, EventStream, SessionStore};
use agentloop_delegator_common::{
    DelegatedSessionHandle, DelegatorEvent, LineMapper, StreamHost, TokioStreamHost,
};

use crate::ACP_AGENT_ID;
use crate::client::{AcpClient, AcpPermissionPolicy};
use crate::mapper::AcpLineMapper;
use crate::profile::{AcpAgentProfile, AcpLaunchConfig};
use crate::protocol::AcpNotification;

/// The ACP protocol version this client speaks.
const ACP_PROTOCOL_VERSION: u64 = 1;

struct AcpSession {
    handle: Arc<DelegatedSessionHandle>,
    client: Arc<AcpClient>,
    updates: tokio::sync::Mutex<mpsc::Receiver<AcpNotification>>,
    acp_session_id: String,
    cancel: CancellationToken,
    turn_gate: tokio::sync::Mutex<()>,
}

/// ACP delegator agent. Generic over the [`StreamHost`] so tests can script
/// the child; production uses [`TokioStreamHost`].
pub struct AcpAgent<H = TokioStreamHost> {
    profile: AcpAgentProfile,
    policy: AcpPermissionPolicy,
    host: Arc<H>,
    store: Arc<dyn SessionStore>,
    sessions: Mutex<HashMap<SessionId, Arc<AcpSession>>>,
}

/// Build an ACP agent with the real duplex host.
pub fn acp_agent(config: AcpLaunchConfig, store: Arc<dyn SessionStore>) -> AcpAgent {
    AcpAgent::with_host(config, store, Arc::new(TokioStreamHost::new()))
}

impl<H: StreamHost + 'static> AcpAgent<H> {
    pub fn with_host(config: AcpLaunchConfig, store: Arc<dyn SessionStore>, host: Arc<H>) -> Self {
        let mut profile = AcpAgentProfile::new(config.program.clone());
        profile.launch = config;
        Self {
            profile,
            policy: AcpPermissionPolicy::default(),
            host,
            store,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn permission_policy(mut self, policy: AcpPermissionPolicy) -> Self {
        self.policy = policy;
        self
    }

    fn session(&self, id: &SessionId) -> Result<Arc<AcpSession>, AgentError> {
        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(id)
            .cloned()
            .ok_or_else(|| AgentError::SessionNotFound(id.clone()))
    }

    /// Spawn the child and run the `initialize` → `session/new` handshake.
    async fn connect(
        &self,
        cwd: &PathBuf,
        cancel: CancellationToken,
    ) -> Result<(Arc<AcpClient>, mpsc::Receiver<AcpNotification>, String), AgentError> {
        let mut spec = self.profile.launch.process_spec();
        if spec.cwd.is_none() {
            spec = spec.cwd(cwd.clone());
        }
        let proc = self
            .host
            .spawn(&spec, cancel)
            .await
            .map_err(|err| AgentError::Other(format!("failed to launch ACP agent: {err}")))?;
        let (client, updates) = AcpClient::start(proc, self.policy);

        client
            .request(
                "initialize",
                serde_json::json!({
                    "protocolVersion": ACP_PROTOCOL_VERSION,
                    "clientCapabilities": {
                        "fs": { "readTextFile": false, "writeTextFile": false }
                    },
                }),
            )
            .await
            .map_err(|err| AgentError::Other(format!("ACP initialize failed: {err}")))?;

        let session_result = client
            .request(
                "session/new",
                serde_json::json!({
                    "cwd": cwd,
                    "mcpServers": [],
                }),
            )
            .await
            .map_err(|err| AgentError::Other(format!("ACP session/new failed: {err}")))?;
        let acp_session_id = session_result
            .get("sessionId")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                AgentError::Other("ACP session/new response carried no sessionId".to_owned())
            })?
            .to_owned();
        Ok((client, updates, acp_session_id))
    }

    async fn run_turn(
        &self,
        session: &Arc<AcpSession>,
        input: PromptInput,
    ) -> Result<TurnSummary, AgentError> {
        let turn_id = TurnId::generate();
        let started_at = now_ms();
        let handle = &session.handle;

        handle
            .emit_persistent(
                Some(&turn_id),
                AgentEvent::TurnStarted {
                    turn_id: turn_id.clone(),
                },
            )
            .await?;
        handle
            .emit_persistent(
                Some(&turn_id),
                AgentEvent::UserMessage {
                    message_id: MessageId::generate(),
                    content: input.parts.clone(),
                },
            )
            .await?;

        let prompt_request = session.client.request(
            "session/prompt",
            serde_json::json!({
                "sessionId": session.acp_session_id,
                "prompt": [ { "type": "text", "text": input.joined_text() } ],
            }),
        );
        tokio::pin!(prompt_request);

        let mut mapper = AcpLineMapper::new();
        let mut assistant_message_id = MessageId::generate();
        let mut assistant_started = false;
        let mut assistant_text = String::new();
        let mut num_tool_calls: u32 = 0;
        let mut turn_usage = TokenUsage::default();
        let mut turn_cost: Option<f64> = None;
        let mut stop_reason: Option<TurnStopReason> = None;
        let mut mapped_error: Option<String> = None;

        let mut updates = session.updates.lock().await;
        // All notification handling is synchronous, so it lives in one closure
        // used both inside the select loop and to drain the queue afterwards
        // (the prompt response can win the race against a queued update).
        let mut apply = |notification: AcpNotification| {
            // Reuse the line mapper: notifications round-trip as lines.
            let Ok(line) = serde_json::to_string(&notification) else {
                return;
            };
            let Ok(events) = mapper.map_line(&line) else {
                return;
            };
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
                    DelegatorEvent::Usage { usage, cost_usd } => {
                        turn_usage = usage;
                        if cost_usd.is_some() {
                            turn_cost = cost_usd;
                        }
                    }
                    DelegatorEvent::TurnFinished { stop_reason: stop } => {
                        stop_reason = Some(stop);
                    }
                    DelegatorEvent::Error { message } => {
                        mapped_error = Some(message);
                    }
                    // Tool call/result events carry ToolCall state the
                    // handle-level helpers own in line_agent; v1 counts
                    // them and surfaces results as ephemeral deltas only.
                    DelegatorEvent::ToolCall { .. } => {
                        num_tool_calls = num_tool_calls.saturating_add(1);
                    }
                    DelegatorEvent::ToolResult { .. } | DelegatorEvent::Unknown { .. } => {}
                }
            }
        };
        let response = loop {
            tokio::select! {
                biased;
                _ = session.cancel.cancelled() => break None,
                update = updates.recv() => {
                    let Some(notification) = update else { break None };
                    apply(notification);
                }
                response = &mut prompt_request => break Some(response),
            }
        };
        // The response can arrive while updates are still queued — drain them
        // so no streamed content is lost.
        while let Ok(notification) = updates.try_recv() {
            apply(notification);
        }
        drop(updates);

        let stop_reason = match response {
            None => TurnStopReason::Cancelled,
            Some(Err(err)) => {
                mapped_error = Some(err.to_string());
                TurnStopReason::Error
            }
            Some(Ok(result)) => stop_reason.unwrap_or_else(|| {
                match result.get("stopReason").and_then(serde_json::Value::as_str) {
                    Some("cancelled") => TurnStopReason::Cancelled,
                    Some("max_tokens") => TurnStopReason::MaxTokens,
                    Some("refusal") => TurnStopReason::Refusal,
                    _ => TurnStopReason::EndTurn,
                }
            }),
        };

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
        if let Some(message) = &mapped_error {
            handle
                .emit_persistent(
                    Some(&turn_id),
                    AgentEvent::SessionError {
                        error: AgentError::Other(message.clone()).to_engine_error(),
                    },
                )
                .await?;
        }

        let summary = TurnSummary {
            turn_id: turn_id.clone(),
            stop_reason,
            usage: turn_usage,
            cost_usd: turn_cost,
            num_model_calls: 1,
            num_tool_calls,
            duration_ms: now_ms().saturating_sub(started_at),
        };
        handle
            .emit_persistent(
                Some(&turn_id),
                AgentEvent::TurnCompleted {
                    turn_id: turn_id.clone(),
                    summary: summary.clone(),
                },
            )
            .await?;

        match mapped_error {
            Some(message) if stop_reason == TurnStopReason::Error => {
                Err(AgentError::Other(message))
            }
            _ => Ok(summary),
        }
    }
}

#[async_trait]
impl<H: StreamHost + 'static> Agent for AcpAgent<H> {
    fn info(&self) -> AgentInfo {
        self.profile.info.clone()
    }

    fn capabilities(&self) -> AgentCaps {
        self.profile.caps.clone()
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
            agent_id: ACP_AGENT_ID.to_owned(),
            parent_id: None,
            role: None,
            depth: 0,
            provider_session_id: None,
            cwd: cwd.clone(),
            model: params.model,
            fallback_models: params.fallback_models,
            mode: params.mode,
            isolation: None,
            workspace_id: None,
            executor: None,
            base_cwd: None,
            reuse_workspace_id: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        self.store.create(meta.clone()).await?;

        let cancel = CancellationToken::new();
        let (client, updates, acp_session_id) = self.connect(&cwd, cancel.clone()).await?;
        self.store
            .update_meta(
                &id,
                SessionMetaPatch {
                    provider_session_id: Some(acp_session_id.clone()),
                    ..SessionMetaPatch::default()
                },
            )
            .await?;

        let handle = Arc::new(DelegatedSessionHandle::new(
            id.clone(),
            ACP_AGENT_ID.to_owned(),
            self.store.clone(),
            0,
        ));
        handle
            .emit_persistent(None, AgentEvent::SessionCreated { meta })
            .await?;
        handle
            .emit_persistent(
                None,
                AgentEvent::EngineInfo {
                    agent_id: ACP_AGENT_ID.to_owned(),
                    capabilities: self.capabilities(),
                    provider_session_id: Some(acp_session_id.clone()),
                    resolution_trace: vec![format!(
                        "connected ACP agent `{}` (protocol v{ACP_PROTOCOL_VERSION})",
                        self.profile.launch.program
                    )],
                },
            )
            .await?;

        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(
                id.clone(),
                Arc::new(AcpSession {
                    handle,
                    client,
                    updates: tokio::sync::Mutex::new(updates),
                    acp_session_id,
                    cancel,
                    turn_gate: tokio::sync::Mutex::new(()),
                }),
            );
        Ok(id)
    }

    async fn resume_session(&self, id: &SessionId) -> Result<(), AgentError> {
        // A dead child cannot be reattached; reconnect with a fresh ACP
        // session and keep appending to the same event log.
        let meta = self.store.get_meta(id).await?;
        let events = self.store.read(id, 0).await?;
        let next_seq = events.last().map(|e| e.seq + 1).unwrap_or(0);
        let cancel = CancellationToken::new();
        let (client, updates, acp_session_id) = self.connect(&meta.cwd, cancel.clone()).await?;
        let handle = Arc::new(DelegatedSessionHandle::new(
            id.clone(),
            ACP_AGENT_ID.to_owned(),
            self.store.clone(),
            next_seq,
        ));
        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(
                id.clone(),
                Arc::new(AcpSession {
                    handle,
                    client,
                    updates: tokio::sync::Mutex::new(updates),
                    acp_session_id,
                    cancel,
                    turn_gate: tokio::sync::Mutex::new(()),
                }),
            );
        Ok(())
    }

    async fn list_sessions(&self) -> Result<Vec<SessionMeta>, AgentError> {
        Ok(self.store.list().await?)
    }

    fn events(&self, session: &SessionId) -> Result<EventStream, AgentError> {
        let state = self.session(session)?;
        let session_id = session.clone();
        let rx = state.handle.subscribe();
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
        _opts: TurnOptions,
    ) -> Result<TurnSummary, AgentError> {
        let state = self.session(session)?;
        let _guard = state
            .turn_gate
            .try_lock()
            .map_err(|_| AgentError::TurnInProgress(session.clone()))?;
        self.run_turn(&state, input).await
    }

    async fn cancel(&self, session: &SessionId) -> Result<(), AgentError> {
        let state = self.session(session)?;
        let _ = state
            .client
            .notify(
                "session/cancel",
                serde_json::json!({ "sessionId": state.acp_session_id }),
            )
            .await;
        state.cancel.cancel();
        Ok(())
    }

    async fn respond_permission(
        &self,
        session: &SessionId,
        id: PermissionRequestId,
        _decision: PermissionDecision,
    ) -> Result<(), AgentError> {
        let _ = self.session(session)?;
        Err(AgentError::Other(format!(
            "the ACP delegator answers permissions from its configured policy in v1 \
             (pending id {id})"
        )))
    }

    async fn respond_question(
        &self,
        session: &SessionId,
        id: QuestionId,
        _answers: Vec<Answer>,
    ) -> Result<(), AgentError> {
        let _ = self.session(session)?;
        Err(AgentError::Other(format!(
            "the ACP delegator does not expose engine-managed user questions (pending id {id})"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use agentloop_delegator_common::{DelegatorHostError, DelegatorProcessSpec, DuplexProcess};
    use agentloop_session::MemoryStore;

    /// Scripted ACP agent: answers the handshake and streams one text update
    /// before completing the prompt request.
    struct ScriptedAcpHost;

    #[async_trait]
    impl StreamHost for ScriptedAcpHost {
        async fn spawn(
            &self,
            _spec: &DelegatorProcessSpec,
            _cancel: CancellationToken,
        ) -> Result<DuplexProcess, DelegatorHostError> {
            let (to_child, mut child_stdin) = mpsc::channel::<String>(64);
            let (child_stdout, from_child) = mpsc::channel::<String>(64);
            tokio::spawn(async move {
                while let Some(line) = child_stdin.recv().await {
                    let Ok(frame) = serde_json::from_str::<serde_json::Value>(&line) else {
                        continue;
                    };
                    let id = frame.get("id").cloned().unwrap_or(serde_json::Value::Null);
                    match frame.get("method").and_then(serde_json::Value::as_str) {
                        Some("initialize") => {
                            let _ = child_stdout
                                .send(format!(
                                    r#"{{"jsonrpc":"2.0","id":{id},"result":{{"protocolVersion":1}}}}"#
                                ))
                                .await;
                        }
                        Some("session/new") => {
                            let _ = child_stdout
                                .send(format!(
                                    r#"{{"jsonrpc":"2.0","id":{id},"result":{{"sessionId":"acp-s1"}}}}"#
                                ))
                                .await;
                        }
                        Some("session/prompt") => {
                            let _ = child_stdout
                                .send(
                                    r#"{"jsonrpc":"2.0","method":"session/update","params":{"kind":"assistant_delta","text":"pong"}}"#
                                        .to_owned(),
                                )
                                .await;
                            let _ = child_stdout
                                .send(format!(
                                    r#"{{"jsonrpc":"2.0","id":{id},"result":{{"stopReason":"end_turn"}}}}"#
                                ))
                                .await;
                        }
                        _ => {}
                    }
                }
            });
            Ok(DuplexProcess::from_channels(to_child, from_child))
        }
    }

    #[tokio::test]
    async fn handshake_prompt_and_update_flow() {
        let store: Arc<dyn SessionStore> = Arc::new(MemoryStore::new());
        let config = AcpLaunchConfig {
            program: "scripted-acp".to_owned(),
            args: Vec::new(),
            env: Default::default(),
            cwd: None,
        };
        let agent = AcpAgent::with_host(config, store.clone(), Arc::new(ScriptedAcpHost));

        let session = agent
            .create_session(NewSessionParams::default())
            .await
            .expect("session");
        let meta = store.get_meta(&session).await.expect("meta");
        assert_eq!(meta.provider_session_id.as_deref(), Some("acp-s1"));

        let summary = agent
            .prompt(
                &session,
                PromptInput::text("say pong"),
                TurnOptions::default(),
            )
            .await
            .expect("turn");
        assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);

        let events = store.read(&session, 0).await.expect("events");
        let assistant: Vec<_> = events
            .iter()
            .filter_map(|e| match &e.event {
                AgentEvent::AssistantMessage { content, .. } => Some(content.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(assistant.len(), 1);
        assert_eq!(assistant[0], vec![ContentBlock::markdown("pong")]);
    }
}
