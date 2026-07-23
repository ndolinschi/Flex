use std::path::PathBuf;

use agentloop_delegator_common::{DelegatorProcessSpec, DelegatorRunRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptTransport {
    Argument,

    Stdin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeCodeConfig {
    pub program: String,
    pub cwd: Option<PathBuf>,
    pub base_args: Vec<String>,
    pub probe_args: Vec<String>,
    pub prompt_transport: PromptTransport,
}

impl Default for ClaudeCodeConfig {
    fn default() -> Self {
        Self {
            program: "claude".to_owned(),
            cwd: None,
            base_args: vec![
                "--print".to_owned(),
                "--output-format".to_owned(),
                "stream-json".to_owned(),
                "--verbose".to_owned(),
            ],
            probe_args: vec!["--version".to_owned()],
            prompt_transport: PromptTransport::Argument,
        }
    }
}

impl ClaudeCodeConfig {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_spec_uses_stream_json_mode() {
        let config = ClaudeCodeConfig::default();
        let spec = config.process_spec();
        assert_eq!(spec.program, "claude");
        assert_eq!(
            spec.args,
            vec![
                "--print".to_owned(),
                "--output-format".to_owned(),
                "stream-json".to_owned(),
                "--verbose".to_owned(),
            ]
        );
    }

    #[test]
    fn stdin_prompt_transport_keeps_prompt_out_of_args() {
        let config = ClaudeCodeConfig {
            prompt_transport: PromptTransport::Stdin,
            ..ClaudeCodeConfig::default()
        };

        let request = config.prompt_request("hello");

        assert_eq!(request.stdin.as_deref(), Some("hello"));
        assert!(!request.spec.args.iter().any(|arg| arg == "hello"));
    }
}
