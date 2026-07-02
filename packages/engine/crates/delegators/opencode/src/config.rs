use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use agentloop_contracts::{
    AgentCaps, AgentInfo, CancelSupport, McpPassthrough, ModelDiscovery, ResumeSupport,
    StreamingGranularity,
};
use agentloop_delegator_common::{DelegatorProcessSpec, DelegatorRunRequest};

use crate::OPENCODE_AGENT_ID;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PromptTransport {
    Argument,
    Stdin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OpencodeConfig {
    pub program: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub base_args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub probe_args: Vec<String>,
    pub prompt_transport: PromptTransport,
}

impl Default for OpencodeConfig {
    fn default() -> Self {
        Self {
            program: "opencode".to_owned(),
            cwd: None,
            base_args: vec!["run".to_owned(), "--json".to_owned()],
            probe_args: vec!["--version".to_owned()],
            prompt_transport: PromptTransport::Stdin,
        }
    }
}

impl OpencodeConfig {
    pub fn process_spec(&self) -> DelegatorProcessSpec {
        self.apply_cwd(DelegatorProcessSpec::new(self.program.clone()).args(self.base_args.clone()))
    }

    pub fn probe_spec(&self) -> DelegatorProcessSpec {
        self.apply_cwd(
            DelegatorProcessSpec::new(self.program.clone()).args(self.probe_args.clone()),
        )
    }

    pub fn prompt_request(&self, prompt: impl Into<String>) -> DelegatorRunRequest {
        let prompt = prompt.into();
        match self.prompt_transport {
            PromptTransport::Argument => DelegatorRunRequest::new(self.process_spec().arg(prompt)),
            PromptTransport::Stdin => DelegatorRunRequest::new(self.process_spec()).stdin(prompt),
        }
    }

    fn apply_cwd(&self, spec: DelegatorProcessSpec) -> DelegatorProcessSpec {
        if let Some(cwd) = &self.cwd {
            spec.cwd(cwd.clone())
        } else {
            spec
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OpencodeProfile {
    pub info: AgentInfo,
    pub caps: AgentCaps,
    pub config: OpencodeConfig,
}

impl Default for OpencodeProfile {
    fn default() -> Self {
        Self {
            info: AgentInfo {
                id: OPENCODE_AGENT_ID.to_owned(),
                display_name: "opencode".to_owned(),
                version: None,
            },
            caps: AgentCaps {
                models: ModelDiscovery::Dynamic,
                streaming: StreamingGranularity::TokenDeltas,
                resume: ResumeSupport::Replay,
                mcp_passthrough: McpPassthrough::Flag,
                subagents: true,
                cancellation: CancelSupport::KillOnly,
                ..AgentCaps::default()
            },
            config: OpencodeConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_process_spec_uses_json_mode() {
        let config = OpencodeConfig::default();
        let spec = config.process_spec();

        assert_eq!(spec.program, "opencode");
        assert_eq!(spec.args, vec!["run", "--json"]);
    }

    #[test]
    fn stdin_prompt_transport_keeps_prompt_out_of_args() {
        let config = OpencodeConfig::default();
        let request = config.prompt_request("hello");

        assert_eq!(request.stdin.as_deref(), Some("hello"));
        assert!(!request.spec.args.iter().any(|arg| arg == "hello"));
    }

    #[test]
    fn profile_marks_future_dynamic_capabilities() {
        let profile = OpencodeProfile::default();

        assert_eq!(profile.info.id, "opencode");
        assert_eq!(profile.caps.mcp_passthrough, McpPassthrough::Flag);
        assert!(profile.caps.subagents);
    }
}
