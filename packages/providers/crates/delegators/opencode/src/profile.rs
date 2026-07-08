//! opencode as a [`DelegatorProfile`] over the shared line-oriented runtime.

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

use crate::OPENCODE_AGENT_ID;
use crate::config::OpencodeConfig;
use crate::mapper::OpencodeLineMapper;

/// opencode's identity, capabilities, and launch shape.
pub struct OpencodeRuntimeProfile {
    pub config: OpencodeConfig,
}

impl DelegatorProfile for OpencodeRuntimeProfile {
    type Mapper = OpencodeLineMapper;

    fn agent_id(&self) -> &str {
        OPENCODE_AGENT_ID
    }

    fn display_name(&self) -> &str {
        "opencode"
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
            reasoning_visible: false,
            // JSON events are structured, but the one-shot host maps them
            // post-hoc, so clients see a snapshot replay, not live deltas.
            streaming: StreamingGranularity::SnapshotOnly,
            resume: ResumeSupport::Replay,
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
        OpencodeLineMapper::new()
    }

    fn resolution_note(&self) -> Vec<String> {
        vec![format!(
            "configured opencode command `{}`",
            self.config.program
        )]
    }
}

/// The opencode delegator agent.
pub type OpencodeAgent<H = TokioCommandHost> = LineDelegatorAgent<OpencodeRuntimeProfile, H>;

/// Build an opencode agent with the real process host.
pub fn opencode_agent(config: OpencodeConfig, store: Arc<dyn SessionStore>) -> OpencodeAgent {
    LineDelegatorAgent::new(
        OpencodeRuntimeProfile { config },
        store,
        Arc::new(TokioCommandHost::new()),
    )
}

/// Build an opencode agent with an ephemeral in-memory store (probing, doctor).
pub fn ephemeral_opencode_agent(config: OpencodeConfig) -> OpencodeAgent {
    opencode_agent(config, Arc::new(MemoryStore::new()))
}

/// Build an opencode agent over a custom [`ProcessHost`] (tests).
pub fn opencode_agent_with_host<H: ProcessHost + 'static>(
    config: OpencodeConfig,
    store: Arc<dyn SessionStore>,
    host: Arc<H>,
) -> OpencodeAgent<H> {
    LineDelegatorAgent::new(OpencodeRuntimeProfile { config }, store, host)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use agentloop_contracts::{
        AgentEvent, NewSessionParams, PromptInput, TurnOptions, TurnStopReason,
    };
    use agentloop_core::Agent;
    use agentloop_delegator_common::{
        DelegatorExitStatus, DelegatorHostError, DelegatorProbeStatus, DelegatorRunOutput,
        DelegatorRunRequest,
    };

    struct ScriptedHost {
        stdout_lines: Vec<String>,
        requests: Mutex<Vec<DelegatorRunRequest>>,
    }

    #[async_trait]
    impl ProcessHost for ScriptedHost {
        async fn probe(
            &self,
            _spec: &DelegatorProcessSpec,
            _cancel: CancellationToken,
        ) -> Result<DelegatorProbeStatus, DelegatorHostError> {
            Ok(DelegatorProbeStatus::Installed {
                version: Some("0.0-test".to_owned()),
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
            Ok(DelegatorRunOutput {
                stdout_lines: self.stdout_lines.clone(),
                stderr: String::new(),
                status: DelegatorExitStatus::success(),
            })
        }
    }

    #[tokio::test]
    async fn json_run_output_becomes_assistant_message_with_usage() {
        // Live-recorded frames, trimmed to the fields the mapper reads.
        let host = Arc::new(ScriptedHost {
            stdout_lines: vec![
                r#"{"type":"step_start","part":{"type":"step-start"}}"#.to_owned(),
                r#"{"type":"text","part":{"type":"text","text":"pong"}}"#.to_owned(),
                r#"{"type":"step_finish","part":{"type":"step-finish","reason":"stop","tokens":{"input":10,"output":2,"reasoning":0,"cache":{"read":0,"write":0}},"cost":0.001}}"#
                    .to_owned(),
            ],
            requests: Mutex::new(Vec::new()),
        });
        let store: Arc<dyn SessionStore> = Arc::new(MemoryStore::new());
        let agent =
            opencode_agent_with_host(OpencodeConfig::default(), store.clone(), host.clone());

        let session = agent
            .create_session(NewSessionParams::default())
            .await
            .expect("session");
        let summary = agent
            .prompt(
                &session,
                PromptInput::text("say pong"),
                TurnOptions::default(),
            )
            .await
            .expect("turn");
        assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);
        assert_eq!(summary.usage.input, 10);
        assert_eq!(summary.usage.output, 2);

        {
            let requests = host.requests.lock().unwrap();
            assert_eq!(requests.len(), 1);
            // Default transport is stdin: the prompt must not leak into args.
            assert_eq!(requests[0].spec.args, vec!["run", "--json"]);
            assert_eq!(requests[0].stdin.as_deref(), Some("say pong"));
        }

        let events = store.read(&session, 0).await.expect("events");
        assert!(
            events
                .iter()
                .any(|(_, event)| matches!(event, AgentEvent::AssistantMessage { .. }))
        );
    }
}
