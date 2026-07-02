//! Claude Code as a [`DelegatorProfile`] over the shared line-oriented
//! delegator runtime.

use std::sync::Arc;

use agentloop_contracts::{
    AgentCaps, AttachmentCaps, CancelSupport, McpPassthrough, ModelDiscovery, PermissionCaps,
    PermissionMode, ResumeSupport, StreamingGranularity,
};
use agentloop_core::SessionStore;
use agentloop_delegator_common::{
    DelegatorProcessSpec, DelegatorProfile, DelegatorRunRequest, LineDelegatorAgent, ProcessHost,
    TokioCommandHost,
};
use agentloop_session::MemoryStore;

use crate::CLAUDE_CODE_AGENT_ID;
use crate::config::ClaudeCodeConfig;
use crate::mapper::ClaudeCodeLineMapper;

/// Claude Code's identity, capabilities, and launch shape.
pub struct ClaudeCodeProfile {
    pub config: ClaudeCodeConfig,
}

impl DelegatorProfile for ClaudeCodeProfile {
    type Mapper = ClaudeCodeLineMapper;

    fn agent_id(&self) -> &str {
        CLAUDE_CODE_AGENT_ID
    }

    fn display_name(&self) -> &str {
        "Claude Code"
    }

    fn capabilities(&self) -> AgentCaps {
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
            cost_reporting: true,
            cancellation: CancelSupport::KillOnly,
            emits_structured_events: true,
            commands: Vec::new(),
        }
    }

    fn probe_spec(&self) -> DelegatorProcessSpec {
        self.config.probe_spec()
    }

    fn prompt_request(&self, prompt: String) -> DelegatorRunRequest {
        self.config.prompt_request(prompt)
    }

    fn mapper(&self) -> Self::Mapper {
        ClaudeCodeLineMapper::new()
    }

    fn resolution_note(&self) -> Vec<String> {
        vec![format!(
            "configured Claude Code command `{}`",
            self.config.program
        )]
    }
}

/// The Claude Code delegator agent.
pub type ClaudeCodeAgent<H = TokioCommandHost> = LineDelegatorAgent<ClaudeCodeProfile, H>;

/// Build a Claude Code agent with the real process host.
pub fn claude_code_agent(
    config: ClaudeCodeConfig,
    store: Arc<dyn SessionStore>,
) -> ClaudeCodeAgent {
    LineDelegatorAgent::new(
        ClaudeCodeProfile { config },
        store,
        Arc::new(TokioCommandHost::new()),
    )
}

/// Build a Claude Code agent with an ephemeral in-memory store (probing,
/// doctor).
pub fn ephemeral_claude_code_agent(config: ClaudeCodeConfig) -> ClaudeCodeAgent {
    claude_code_agent(config, Arc::new(MemoryStore::new()))
}

/// Build a Claude Code agent over a custom [`ProcessHost`] (tests).
pub fn claude_code_agent_with_host<H: ProcessHost + 'static>(
    config: ClaudeCodeConfig,
    store: Arc<dyn SessionStore>,
    host: Arc<H>,
) -> ClaudeCodeAgent<H> {
    LineDelegatorAgent::new(ClaudeCodeProfile { config }, store, host)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use agentloop_contracts::{
        AgentEvent, ContentBlock, NewSessionParams, PromptInput, ToolCallStatus, ToolOutput,
        TurnOptions, TurnStopReason,
    };
    use agentloop_core::{Agent, SessionStore};
    use agentloop_delegator_common::{
        DelegatorExitStatus, DelegatorHostError, DelegatorProbeStatus, DelegatorRunOutput,
        DelegatorRunRequest,
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
            claude_code_agent_with_host(ClaudeCodeConfig::default(), store.clone(), host.clone());

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
        let agent = claude_code_agent_with_host(ClaudeCodeConfig::default(), store, host);
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
