//! `Bash`: run a shell command in the session cwd.

use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::process::Command;

use agentloop_contracts::ToolOutput;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

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

/// Execute a shell command through `/bin/sh -lc`.
#[derive(Debug, Default, Clone, Copy)]
pub struct BashTool;

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

        let child = Command::new("/bin/sh")
            .arg("-lc")
            .arg(&input.command)
            .current_dir(&ctx.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|err| {
                ToolError::Execution(format!(
                    "Cannot start Bash command in `{}`: {err}.",
                    ctx.cwd.display()
                ))
            })?;

        let mut wait_task = tokio::spawn(async move { child.wait_with_output().await });
        let output = tokio::select! {
            _ = ctx.cancel.cancelled() => {
                wait_task.abort();
                return Err(ToolError::Cancelled);
            }
            result = tokio::time::timeout(Duration::from_millis(timeout_ms), &mut wait_task) => {
                match result {
                    Ok(output) => output.map_err(|err| {
                        ToolError::Execution(format!("Bash worker failed before producing output: {err}."))
                    })?.map_err(|err| {
                        ToolError::Execution(format!("Bash command failed while collecting output: {err}."))
                    })?,
                    Err(_) => {
                        wait_task.abort();
                        return Err(ToolError::Timeout(timeout_ms));
                    }
                }
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code();
        let mut rendered = String::new();
        rendered.push_str("exit_code: ");
        rendered.push_str(
            &exit_code
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
            is_error: !output.status.success(),
            structured: Some(serde_json::json!({
                "exit_code": exit_code,
                "success": output.status.success(),
                "truncated": truncated,
            })),
        })
    }
}
