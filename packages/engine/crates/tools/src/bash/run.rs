use async_trait::async_trait;

use agentloop_contracts::ToolOutput;
use agentloop_core::{
    BackgroundEntry, ExecError, ExecOrDemoted, ExecSpec, PermissionHint, Tool, ToolCategory,
    ToolContext, ToolDescriptor, ToolError,
};

use crate::fs::{schema_of, truncate_chars};

use super::chunk_sink::exec_chunk_sink;
use super::input::BashInput;
use super::{BashTool, DEFAULT_TIMEOUT_MS, MAX_OUTPUT_CHARS, MAX_TIMEOUT_MS};

#[async_trait]
impl Tool for BashTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "Bash".to_owned(),
            description: "Run a shell command in the session working directory. \
                          On Unix this uses `/bin/sh -lc`; on Windows it uses \
                          PowerShell (`powershell.exe -Command`). This is for \
                          verification, build/test commands, and carefully scoped \
                          automation. Quote paths with spaces. Long-running \
                          commands must set `timeout_ms`; output is captured and explicitly \
                          truncated when large. For long-running processes (dev servers, \
                          watchers), set `run_in_background: true` instead of `timeout_ms`: the \
                          call returns after initial output with a process id, output keeps \
                          streaming to the agent terminal, and the process keeps running after \
                          the call returns. Check on it later or stop it with \
                          `background_action: \"status\"|\"kill\"` and that `process_id`."
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
                "Input for `Bash` must be {{\"command\": \"...\", \"timeout_ms\": <optional \
                 milliseconds>, \"run_in_background\": <optional bool>}} or \
                 {{\"background_action\": \"status\"|\"kill\", \"process_id\": \"...\"}}: {err}."
            ))
        })?;

        if let Some(action) = input.background_action {
            return self
                .run_background_action(&ctx, action, input.process_id.as_deref())
                .await;
        }

        let command = input
            .command
            .filter(|c| !c.trim().is_empty())
            .ok_or_else(|| {
                ToolError::InvalidInput(
                    "`command` cannot be empty. Pass the exact shell command to run, or use \
                 `background_action` + `process_id` to control an already-started background \
                 process."
                        .to_owned(),
                )
            })?;

        if input.run_in_background {
            return self.run_in_background(&ctx, command).await;
        }

        let timeout_ms = input
            .timeout_ms
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .min(MAX_TIMEOUT_MS);

        let call_id = ctx.call_id.as_str().to_owned();
        let demote_token = self
            .demote
            .register(ctx.session_id.clone(), call_id.clone());
        let started_at_ms = agentloop_contracts::now_ms();

        let spec = ExecSpec {
            command: command.clone(),
            cwd: ctx.cwd.clone(),
            env: Vec::new(),
            timeout_ms,
            network: self.network,
            chunk_sink: Some(exec_chunk_sink(&ctx)),
            demote: Some(demote_token),
        };
        let result = self.executor.exec_demotable(spec, ctx.cancel.clone()).await;
        self.demote.unregister(&ctx.session_id, &call_id);

        let outcome = match result {
            Ok(ExecOrDemoted::Completed(outcome)) => outcome,
            Ok(ExecOrDemoted::Demoted { accumulated, entry }) => {
                let status = entry.handle.status();
                self.background.insert(
                    ctx.session_id.clone(),
                    call_id.clone(),
                    BackgroundEntry {
                        command,
                        started_at_ms,
                        handle: entry.handle,
                    },
                );
                let (accumulated, truncated) = truncate_chars(&accumulated, MAX_OUTPUT_CHARS);
                let rendered = format!(
                    "Moved to background (process {call_id}). Output so far:\n{}\n\n\
                     [output continues in the agent terminal; use Bash background_action \
                     status/kill with process_id {call_id}]",
                    if accumulated.is_empty() {
                        "(none yet)".to_owned()
                    } else {
                        accumulated
                    }
                );
                return Ok(ToolOutput {
                    content: vec![agentloop_contracts::ToolResultBlock::markdown(rendered)],
                    is_error: false,
                    structured: Some(serde_json::json!({
                        "process_id": call_id,
                        "pid": status.pid,
                        "running": status.running,
                        "truncated": truncated,
                    })),
                });
            }
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
