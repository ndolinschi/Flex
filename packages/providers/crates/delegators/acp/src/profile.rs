use std::collections::BTreeMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use agentloop_contracts::{
    AgentCaps, AgentInfo, CancelSupport, McpPassthrough, ModelDiscovery, ResumeSupport,
    StreamingGranularity,
};
use agentloop_delegator_common::DelegatorProcessSpec;

use crate::ACP_AGENT_ID;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AcpLaunchConfig {
    pub program: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
}

impl AcpLaunchConfig {
    pub fn process_spec(&self) -> DelegatorProcessSpec {
        let mut spec = DelegatorProcessSpec::new(self.program.clone()).args(self.args.clone());
        for (key, value) in &self.env {
            spec = spec.env(key.clone(), value.clone());
        }
        if let Some(cwd) = &self.cwd {
            spec.cwd(cwd.clone())
        } else {
            spec
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AcpAgentProfile {
    pub info: AgentInfo,
    pub caps: AgentCaps,
    pub launch: AcpLaunchConfig,
}

impl AcpAgentProfile {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            info: AgentInfo {
                id: ACP_AGENT_ID.to_owned(),
                display_name: "ACP Agent".to_owned(),
                version: None,
            },
            caps: AgentCaps {
                models: ModelDiscovery::Dynamic,
                streaming: StreamingGranularity::TokenDeltas,
                resume: ResumeSupport::Native,
                mcp_passthrough: McpPassthrough::SessionNew,
                cancellation: CancelSupport::Graceful,
                ..AgentCaps::default()
            },
            launch: AcpLaunchConfig {
                program: program.into(),
                args: Vec::new(),
                env: BTreeMap::new(),
                cwd: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_declares_acp_protocol_capabilities() {
        let profile = AcpAgentProfile::new("acp-agent");

        assert_eq!(profile.info.id, "acp");
        assert_eq!(profile.caps.mcp_passthrough, McpPassthrough::SessionNew);
        assert_eq!(profile.caps.resume, ResumeSupport::Native);
        assert_eq!(profile.caps.cancellation, CancelSupport::Graceful);
    }

    #[test]
    fn launch_config_builds_process_spec() {
        let mut env = BTreeMap::new();
        env.insert("ACP_LOG".to_owned(), "debug".to_owned());
        let config = AcpLaunchConfig {
            program: "agent".to_owned(),
            args: vec!["--stdio".to_owned()],
            env,
            cwd: Some(PathBuf::from("/tmp/work")),
        };

        let spec = config.process_spec();

        assert_eq!(spec.program, "agent");
        assert_eq!(spec.args, vec!["--stdio"]);
        assert_eq!(spec.env.get("ACP_LOG"), Some(&"debug".to_owned()));
        assert_eq!(spec.cwd, Some(PathBuf::from("/tmp/work")));
    }
}
