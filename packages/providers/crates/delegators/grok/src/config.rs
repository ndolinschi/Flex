use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use agentloop_contracts::{
    AgentCaps, AgentInfo, CancelSupport, McpPassthrough, ModelDiscovery, ResumeSupport,
    StreamingGranularity,
};
use agentloop_delegator_common::{DelegatorProcessSpec, DelegatorRunRequest};

use crate::GROK_AGENT_ID;

/// Launch config for the official Grok Build CLI headless mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GrokConfig {
    pub program: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub base_args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub probe_args: Vec<String>,
    /// Optional xAI API key (also read from `XAI_API_KEY`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

impl Default for GrokConfig {
    fn default() -> Self {
        Self {
            program: "grok".to_owned(),
            cwd: None,
            // `-p <prompt>` is appended by [`Self::prompt_request`].
            // `--always-approve` keeps headless runs non-interactive;
            // `--no-auto-update` skips background update checks in CI/scripts
            // (see https://docs.x.ai/build/cli/headless-scripting).
            base_args: vec![
                "--output-format".to_owned(),
                "streaming-json".to_owned(),
                "--always-approve".to_owned(),
                "--no-auto-update".to_owned(),
                "-p".to_owned(),
            ],
            probe_args: vec!["--version".to_owned()],
            api_key: None,
        }
    }
}

impl GrokConfig {
    pub fn process_spec(&self) -> DelegatorProcessSpec {
        self.apply_auth(self.apply_cwd(
            DelegatorProcessSpec::new(self.program.clone()).args(self.base_args.clone()),
        ))
    }

    pub fn probe_spec(&self) -> DelegatorProcessSpec {
        self.apply_auth(self.apply_cwd(
            DelegatorProcessSpec::new(self.program.clone()).args(self.probe_args.clone()),
        ))
    }

    /// The prompt is the value of `-p` / `--single` (last base arg).
    pub fn prompt_request(&self, prompt: impl Into<String>) -> DelegatorRunRequest {
        DelegatorRunRequest::new(self.process_spec().arg(prompt.into()))
    }

    /// Attach an xAI API key for headless auth (`XAI_API_KEY`).
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    fn apply_cwd(&self, spec: DelegatorProcessSpec) -> DelegatorProcessSpec {
        if let Some(cwd) = &self.cwd {
            spec.cwd(cwd.clone())
        } else {
            spec
        }
    }

    fn apply_auth(&self, mut spec: DelegatorProcessSpec) -> DelegatorProcessSpec {
        let key = self
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                std::env::var("XAI_API_KEY")
                    .ok()
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty())
            });
        if let Some(api_key) = key {
            spec = spec.env("XAI_API_KEY", api_key);
        }
        spec
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GrokProfile {
    pub info: AgentInfo,
    pub caps: AgentCaps,
    pub config: GrokConfig,
}

impl Default for GrokProfile {
    fn default() -> Self {
        Self {
            info: AgentInfo {
                id: GROK_AGENT_ID.to_owned(),
                display_name: "Grok".to_owned(),
                version: None,
            },
            caps: AgentCaps {
                models: ModelDiscovery::None,
                streaming: StreamingGranularity::SnapshotOnly,
                resume: ResumeSupport::Replay,
                mcp_passthrough: McpPassthrough::None,
                cancellation: CancelSupport::KillOnly,
                emits_structured_events: true,
                ..AgentCaps::default()
            },
            config: GrokConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_process_spec_uses_streaming_json() {
        let config = GrokConfig::default();
        let spec = config.process_spec();

        assert_eq!(spec.program, "grok");
        assert_eq!(
            spec.args,
            vec![
                "--output-format",
                "streaming-json",
                "--always-approve",
                "--no-auto-update",
                "-p"
            ]
        );
    }

    #[test]
    fn prompt_appends_as_dash_p_value() {
        let config = GrokConfig::default();
        let request = config.prompt_request("hello");

        assert_eq!(request.stdin, None);
        assert_eq!(request.spec.args.last().map(String::as_str), Some("hello"));
        assert!(request.spec.args.windows(2).any(|w| w == ["-p", "hello"]));
    }

    #[test]
    fn with_api_key_forwards_env() {
        let config = GrokConfig::default().with_api_key("test-key");
        let spec = config.process_spec();
        assert_eq!(spec.env.get("XAI_API_KEY"), Some(&"test-key".to_owned()));
        assert!(!spec.args.iter().any(|arg| arg == "test-key"));
    }
}
