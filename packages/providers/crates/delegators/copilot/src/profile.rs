//! Copilot as a [`DelegatorProfile`] over the shared line-oriented runtime.

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

use crate::COPILOT_AGENT_ID;
use crate::config::CopilotConfig;
use crate::mapper::CopilotLineMapper;

/// GitHub Copilot's identity, capabilities, and launch shape.
pub struct CopilotProfile {
    pub config: CopilotConfig,
}

impl DelegatorProfile for CopilotProfile {
    type Mapper = CopilotLineMapper;

    fn agent_id(&self) -> &str {
        COPILOT_AGENT_ID
    }

    fn display_name(&self) -> &str {
        "GitHub Copilot"
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
            emits_structured_events: false,
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
        CopilotLineMapper::new()
    }

    fn resolution_note(&self) -> Vec<String> {
        let mut notes = vec![format!(
            "configured GitHub Copilot command `{}`",
            self.config.program
        )];
        if !self.config.allow_all_tools {
            notes.push(
                "Copilot tool use is disabled (enable with allow_all_tools) — \
                 answers only, no file edits"
                    .to_owned(),
            );
        }
        notes
    }
}

/// The GitHub Copilot delegator agent.
pub type CopilotAgent<H = TokioCommandHost> = LineDelegatorAgent<CopilotProfile, H>;

/// Build a Copilot agent with the real process host.
pub fn copilot_agent(config: CopilotConfig, store: Arc<dyn SessionStore>) -> CopilotAgent {
    LineDelegatorAgent::new(
        CopilotProfile { config },
        store,
        Arc::new(TokioCommandHost::new()),
    )
}

/// Build a Copilot agent with an ephemeral in-memory store (probing, doctor).
pub fn ephemeral_copilot_agent(config: CopilotConfig) -> CopilotAgent {
    copilot_agent(config, Arc::new(MemoryStore::new()))
}

/// Build a Copilot agent over a custom [`ProcessHost`] (tests).
pub fn copilot_agent_with_host<H: ProcessHost + 'static>(
    config: CopilotConfig,
    store: Arc<dyn SessionStore>,
    host: Arc<H>,
) -> CopilotAgent<H> {
    LineDelegatorAgent::new(CopilotProfile { config }, store, host)
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
    async fn plain_text_output_becomes_one_assistant_message() {
        let host = Arc::new(ScriptedHost {
            stdout_lines: vec![
                "\u{1b}[1mpong\u{1b}[0m".to_owned(),
                "Total usage est: 1 premium request".to_owned(),
            ],
            requests: Mutex::new(Vec::new()),
        });
        let store: Arc<dyn SessionStore> = Arc::new(MemoryStore::new());
        let agent = copilot_agent_with_host(CopilotConfig::default(), store.clone(), host.clone());

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
            assert_eq!(requests[0].spec.args, vec!["-p", "say pong"]);
        }

        let events = store.read(&session, 0).await.expect("events");
        let assistant: Vec<_> = events
            .iter()
            .filter_map(|(_, event)| match event {
                AgentEvent::AssistantMessage { content, .. } => Some(content.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(assistant.len(), 1);
        assert_eq!(assistant[0], vec![ContentBlock::markdown("pong\n")]);
    }
}
