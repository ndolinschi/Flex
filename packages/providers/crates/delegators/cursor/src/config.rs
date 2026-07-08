use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use agentloop_contracts::{
    AgentCaps, AgentInfo, CancelSupport, McpPassthrough, ModelDiscovery, ResumeSupport,
    StreamingGranularity,
};
use agentloop_delegator_common::{DelegatorProcessSpec, DelegatorRunRequest};

use crate::CURSOR_AGENT_ID;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CursorIntegrationKind {
    Cli,
    WorkspaceBridge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CursorLaunchConfig {
    pub integration: CursorIntegrationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
}

impl Default for CursorLaunchConfig {
    fn default() -> Self {
        Self {
            integration: CursorIntegrationKind::WorkspaceBridge,
            program: None,
            args: Vec::new(),
            cwd: None,
        }
    }
}

impl CursorLaunchConfig {
    pub fn process_spec(&self) -> Result<DelegatorProcessSpec, CursorConfigError> {
        match self.integration {
            CursorIntegrationKind::Cli => {
                let Some(program) = self
                    .program
                    .as_ref()
                    .filter(|program| !program.trim().is_empty())
                else {
                    return Err(CursorConfigError::NotInstalled {
                        hint: "configure a Cursor-compatible agent CLI program before selecting the Cursor delegator".to_owned(),
                    });
                };
                let spec = DelegatorProcessSpec::new(program.clone()).args(self.args.clone());
                Ok(if let Some(cwd) = &self.cwd {
                    spec.cwd(cwd.clone())
                } else {
                    spec
                })
            }
            CursorIntegrationKind::WorkspaceBridge => Err(CursorConfigError::NotImplemented {
                hint: "Cursor workspace bridge support is only a profile placeholder in this crate"
                    .to_owned(),
            }),
        }
    }
}

/// Infallible launch config for the `cursor-agent` CLI — the runtime path.
/// The older [`CursorLaunchConfig`] stays as the schema-facing shape that can
/// also describe the (unimplemented) workspace bridge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CursorCliConfig {
    pub program: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub base_args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub probe_args: Vec<String>,
}

impl Default for CursorCliConfig {
    fn default() -> Self {
        Self {
            program: "cursor-agent".to_owned(),
            cwd: None,
            base_args: vec![
                "--print".to_owned(),
                "--output-format".to_owned(),
                "stream-json".to_owned(),
                "--force".to_owned(),
            ],
            probe_args: vec!["--version".to_owned()],
        }
    }
}

impl CursorCliConfig {
    pub fn process_spec(&self) -> DelegatorProcessSpec {
        self.apply_cwd(DelegatorProcessSpec::new(self.program.clone()).args(self.base_args.clone()))
    }

    pub fn probe_spec(&self) -> DelegatorProcessSpec {
        self.apply_cwd(
            DelegatorProcessSpec::new(self.program.clone()).args(self.probe_args.clone()),
        )
    }

    /// The prompt rides as the final positional argument.
    pub fn prompt_request(&self, prompt: impl Into<String>) -> DelegatorRunRequest {
        DelegatorRunRequest::new(self.process_spec().arg(prompt.into()))
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
pub struct CursorProfile {
    pub info: AgentInfo,
    pub caps: AgentCaps,
    pub launch: CursorLaunchConfig,
}

impl Default for CursorProfile {
    fn default() -> Self {
        Self {
            info: AgentInfo {
                id: CURSOR_AGENT_ID.to_owned(),
                display_name: "Cursor".to_owned(),
                version: None,
            },
            caps: AgentCaps {
                models: ModelDiscovery::None,
                streaming: StreamingGranularity::SnapshotOnly,
                resume: ResumeSupport::Replay,
                mcp_passthrough: McpPassthrough::None,
                cancellation: CancelSupport::KillOnly,
                emits_structured_events: false,
                ..AgentCaps::default()
            },
            launch: CursorLaunchConfig::default(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CursorConfigError {
    #[error("Cursor delegator is not installed or configured: {hint}")]
    NotInstalled { hint: String },
    #[error("Cursor delegator runtime is not implemented yet: {hint}")]
    NotImplemented { hint: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_is_not_runtime_ready() {
        let profile = CursorProfile::default();
        let err = profile.launch.process_spec();

        assert_eq!(profile.info.id, "cursor");
        assert!(!profile.caps.emits_structured_events);
        assert!(matches!(
            err,
            Err(CursorConfigError::NotImplemented { hint })
                if hint.contains("profile placeholder")
        ));
    }

    #[test]
    fn cli_profile_requires_program() {
        let config = CursorLaunchConfig {
            integration: CursorIntegrationKind::Cli,
            program: None,
            args: Vec::new(),
            cwd: None,
        };

        assert!(matches!(
            config.process_spec(),
            Err(CursorConfigError::NotInstalled { hint })
                if hint.contains("configure")
        ));
    }

    #[test]
    fn cli_profile_builds_process_spec_when_configured() {
        let config = CursorLaunchConfig {
            integration: CursorIntegrationKind::Cli,
            program: Some("cursor-agent".to_owned()),
            args: vec!["--stdio".to_owned()],
            cwd: Some(PathBuf::from("/tmp/work")),
        };

        let spec = config.process_spec();

        match spec {
            Ok(spec) => {
                assert_eq!(spec.program, "cursor-agent");
                assert_eq!(spec.args, vec!["--stdio"]);
                assert_eq!(spec.cwd, Some(PathBuf::from("/tmp/work")));
            }
            Err(err) => panic!("configured CLI profile should build: {err}"),
        }
    }
}
