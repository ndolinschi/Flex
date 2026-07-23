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

use crate::CURSOR_AGENT_ID;
use crate::config::CursorCliConfig;
use crate::mapper::CursorLineMapper;

pub struct CursorRuntimeProfile {
    pub config: CursorCliConfig,
}

impl DelegatorProfile for CursorRuntimeProfile {
    type Mapper = CursorLineMapper;

    fn agent_id(&self) -> &str {
        CURSOR_AGENT_ID
    }

    fn display_name(&self) -> &str {
        "Cursor"
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

            streaming: StreamingGranularity::SnapshotOnly,
            resume: ResumeSupport::Replay,
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

    fn probe_spec(&self) -> DelegatorProcessSpec {
        self.config.probe_spec()
    }

    fn prompt_request(&self, prompt: String) -> DelegatorRunRequest {
        self.config.prompt_request(prompt)
    }

    fn mapper(&self) -> Self::Mapper {
        CursorLineMapper::new()
    }

    fn resolution_note(&self) -> Vec<String> {
        vec![format!(
            "configured Cursor command `{}`",
            self.config.program
        )]
    }
}

pub type CursorAgent<H = TokioCommandHost> = LineDelegatorAgent<CursorRuntimeProfile, H>;

pub fn cursor_agent(config: CursorCliConfig, store: Arc<dyn SessionStore>) -> CursorAgent {
    LineDelegatorAgent::new(
        CursorRuntimeProfile { config },
        store,
        Arc::new(TokioCommandHost::new()),
    )
}

pub fn ephemeral_cursor_agent(config: CursorCliConfig) -> CursorAgent {
    cursor_agent(config, Arc::new(MemoryStore::new()))
}

pub fn cursor_agent_with_host<H: ProcessHost + 'static>(
    config: CursorCliConfig,
    store: Arc<dyn SessionStore>,
    host: Arc<H>,
) -> CursorAgent<H> {
    LineDelegatorAgent::new(CursorRuntimeProfile { config }, store, host)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use agentloop_contracts::{
        AgentEvent, ContentBlock, NewSessionParams, PromptInput, TurnOptions, TurnStopReason,
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
    async fn stream_json_output_becomes_assistant_message() {
        let host = Arc::new(ScriptedHost {
            stdout_lines: vec![
                r#"{"type":"system","subtype":"init","session_id":"s1","model":"Composer"}"#
                    .to_owned(),
                r#"{"type":"assistant","message":{"content":[{"type":"text","text":"pong"}]},"session_id":"s1"}"#
                    .to_owned(),
                r#"{"type":"result","subtype":"success","is_error":false,"result":"pong","session_id":"s1"}"#
                    .to_owned(),
            ],
            requests: Mutex::new(Vec::new()),
        });
        let store: Arc<dyn SessionStore> = Arc::new(MemoryStore::new());
        let agent = cursor_agent_with_host(CursorCliConfig::default(), store.clone(), host.clone());

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

        {
            let requests = host.requests.lock().unwrap();
            assert_eq!(requests.len(), 1);
            assert_eq!(
                requests[0].spec.args,
                vec![
                    "--print",
                    "--output-format",
                    "stream-json",
                    "--force",
                    "say pong"
                ]
            );
        }

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

    #[tokio::test]
    #[ignore = "requires cursor-agent on PATH"]
    async fn live_probe_cursor_agent_cli() {
        let agent = ephemeral_cursor_agent(CursorCliConfig::default());
        let caps = agent.probe(CancellationToken::new()).await.expect("probe");
        assert!(matches!(caps, DelegatorProbeStatus::Installed { .. }));
    }
}
