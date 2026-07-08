//! `Bash`: run a shell command in the session cwd through the composed
//! execution backend.

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::ToolOutput;
use agentloop_core::{
    ExecError, ExecSpec, Executor, NetworkPolicy, PermissionHint, Tool, ToolCategory, ToolContext,
    ToolDescriptor, ToolError,
};

use crate::fs::{schema_of, truncate_chars};

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MAX_TIMEOUT_MS: u64 = 600_000;
const MAX_OUTPUT_CHARS: usize = 120_000;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BashInput {
    /// Shell command to run in the session cwd.
    command: String,
    /// Optional timeout in milliseconds. Defaults to 30000, capped at 600000.
    timeout_ms: Option<u64>,
}

/// Execute a shell command with `sh -lc` semantics through the injected
/// [`Executor`] backend (local process by default; container/remote backends
/// at composition time).
pub struct BashTool {
    executor: Arc<dyn Executor>,
    network: NetworkPolicy,
}

impl BashTool {
    pub fn new(executor: Arc<dyn Executor>) -> Self {
        Self {
            executor,
            network: NetworkPolicy::Allowed,
        }
    }

    /// Set the network posture every command runs under.
    pub fn with_network(mut self, network: NetworkPolicy) -> Self {
        self.network = network;
        self
    }
}

#[async_trait]
impl Tool for BashTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "Bash".to_owned(),
            description: "Run a shell command in the session working directory using \
                          `/bin/sh -lc`. This is for verification, build/test commands, and \
                          carefully scoped automation. Quote paths with spaces. Long-running \
                          commands must set `timeout_ms`; output is captured and explicitly \
                          truncated when large."
                .to_owned(),
            input_schema: schema_of::<BashInput>(),
            read_only: false,
            category: ToolCategory::Shell,
            needs_permission: PermissionHint::Always,
        }
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: BashInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `Bash` must be {{\"command\": \"...\", \"timeout_ms\": \
                 <optional milliseconds>}}: {err}."
            ))
        })?;
        if input.command.trim().is_empty() {
            return Err(ToolError::InvalidInput(
                "`command` cannot be empty. Pass the exact shell command to run.".to_owned(),
            ));
        }
        let timeout_ms = input
            .timeout_ms
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .min(MAX_TIMEOUT_MS);

        let spec = ExecSpec {
            command: input.command,
            cwd: ctx.cwd.clone(),
            env: Vec::new(),
            timeout_ms,
            network: self.network,
        };
        let outcome = match self.executor.exec(spec, ctx.cancel.clone()).await {
            Ok(outcome) => outcome,
            Err(ExecError::Cancelled) => return Err(ToolError::Cancelled),
            Err(ExecError::Timeout(ms)) => return Err(ToolError::Timeout(ms)),
            Err(err) => {
                return Err(ToolError::Execution(format!(
                    "Bash command failed in `{}`: {err}.",
                    ctx.cwd.display()
                )));
            }
        };

        let stdout = String::from_utf8_lossy(&outcome.stdout);
        let stderr = String::from_utf8_lossy(&outcome.stderr);
        let success = outcome.exit_code == Some(0);
        let mut rendered = String::new();
        rendered.push_str("exit_code: ");
        rendered.push_str(
            &outcome
                .exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "terminated_by_signal".to_owned()),
        );
        rendered.push_str("\n\nstdout:\n");
        rendered.push_str(stdout.as_ref());
        rendered.push_str("\n\nstderr:\n");
        rendered.push_str(stderr.as_ref());
        let (rendered, truncated) = truncate_chars(&rendered, MAX_OUTPUT_CHARS);

        Ok(ToolOutput {
            content: vec![agentloop_contracts::ToolResultBlock::markdown(rendered)],
            is_error: !success,
            structured: Some(serde_json::json!({
                "exit_code": outcome.exit_code,
                "success": success,
                "truncated": truncated,
            })),
        })
    }
}
