use std::path::PathBuf;

use agentloop_delegator_common::{DelegatorProcessSpec, DelegatorRunRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotConfig {
    pub program: String,
    pub cwd: Option<PathBuf>,

    pub allow_all_tools: bool,

    pub model: Option<String>,

    pub extra_args: Vec<String>,
    pub probe_args: Vec<String>,
}

impl Default for CopilotConfig {
    fn default() -> Self {
        Self {
            program: "copilot".to_owned(),
            cwd: None,
            allow_all_tools: false,
            model: None,
            extra_args: Vec::new(),
            probe_args: vec!["--version".to_owned()],
        }
    }
}

impl CopilotConfig {
    pub fn probe_spec(&self) -> DelegatorProcessSpec {
        self.apply_cwd(
            DelegatorProcessSpec::new(self.program.clone()).args(self.probe_args.clone()),
        )
    }

    pub fn prompt_request(&self, prompt: impl Into<String>) -> DelegatorRunRequest {
        let mut spec = DelegatorProcessSpec::new(self.program.clone());
        if self.allow_all_tools {
            spec = spec.arg("--allow-all-tools");
        }
        if let Some(model) = &self.model {
            spec = spec.arg("--model").arg(model.clone());
        }
        spec = spec.args(self.extra_args.clone());
        spec = spec.arg("-p").arg(prompt.into());
        spec = spec.env("NO_COLOR", "1");
        DelegatorRunRequest::new(self.apply_cwd(spec))
    }

    fn apply_cwd(&self, spec: DelegatorProcessSpec) -> DelegatorProcessSpec {
        match &self.cwd {
            Some(cwd) => spec.cwd(cwd.clone()),
            None => spec,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_prompt_request_is_conservative() {
        let request = CopilotConfig::default().prompt_request("say pong");
        assert_eq!(request.spec.program, "copilot");
        assert_eq!(request.spec.args, vec!["-p", "say pong"]);
        assert!(request.stdin.is_none());
        assert!(
            request
                .spec
                .env
                .iter()
                .any(|(k, v)| k == "NO_COLOR" && v == "1")
        );
    }

    #[test]
    fn flags_order_prompt_last() {
        let config = CopilotConfig {
            allow_all_tools: true,
            model: Some("claude-sonnet-4.5".to_owned()),
            extra_args: vec!["--no-custom-instructions".to_owned()],
            ..CopilotConfig::default()
        };
        let request = config.prompt_request("do it");
        assert_eq!(
            request.spec.args,
            vec![
                "--allow-all-tools",
                "--model",
                "claude-sonnet-4.5",
                "--no-custom-instructions",
                "-p",
                "do it",
            ]
        );
    }

    #[test]
    fn probe_uses_version() {
        let spec = CopilotConfig::default().probe_spec();
        assert_eq!(spec.args, vec!["--version"]);
    }
}
