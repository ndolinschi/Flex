//! Background process start / status / kill paths for [`super::BashTool`].

use agentloop_contracts::ToolOutput;
use agentloop_core::{BackgroundEntry, ExecError, ExecSpec, ToolContext, ToolError};

use crate::fs::truncate_chars;

use super::chunk_sink::exec_chunk_sink;
use super::input::BackgroundAction;
use super::{BashTool, MAX_OUTPUT_CHARS, MAX_TIMEOUT_MS};

impl BashTool {
    pub(super) async fn run_in_background(
        &self,
        ctx: &ToolContext,
        command: String,
    ) -> Result<ToolOutput, ToolError> {
        let process_id = ctx.call_id.as_str().to_owned();
        let spec = ExecSpec {
            command: command.clone(),
            cwd: ctx.cwd.clone(),
            env: Vec::new(),
            // Background processes are not subject to the blocking timeout —
            // the executor's own initial-output window bounds this call.
            timeout_ms: MAX_TIMEOUT_MS,
            network: self.network,
            chunk_sink: Some(exec_chunk_sink(ctx)),
            demote: None,
        };
        let spawn = match self.executor.exec_background(spec).await {
            Ok(spawn) => spawn,
            Err(ExecError::Unsupported(detail)) => {
                return Err(ToolError::Execution(format!(
                    "this session's execution backend does not support \
                     `run_in_background`: {detail}. Run the command normally with an explicit \
                     `timeout_ms` instead."
                )));
            }
            Err(ExecError::Cancelled) => return Err(ToolError::Cancelled),
            Err(err) => {
                return Err(ToolError::Execution(format!(
                    "failed to start background process in `{}`: {err}.",
                    ctx.cwd.display()
                )));
            }
        };

        let status = spawn.handle.status();
        self.background.insert(
            ctx.session_id.clone(),
            process_id.clone(),
            BackgroundEntry {
                command,
                started_at_ms: agentloop_contracts::now_ms(),
                handle: spawn.handle,
            },
        );

        let (initial, truncated) = truncate_chars(&spawn.initial_output, MAX_OUTPUT_CHARS);
        let state = if status.running { "running" } else { "exited" };
        let rendered = format!(
            "Started background process {process_id} (pid {}), now {state}. Initial output:\n{}",
            status
                .pid
                .map(|p| p.to_string())
                .unwrap_or_else(|| "unknown".to_owned()),
            if initial.is_empty() {
                "(none yet)".to_owned()
            } else {
                initial
            }
        );

        Ok(ToolOutput {
            content: vec![agentloop_contracts::ToolResultBlock::markdown(rendered)],
            is_error: false,
            structured: Some(serde_json::json!({
                "process_id": process_id,
                "pid": status.pid,
                "running": status.running,
                "truncated": truncated,
            })),
        })
    }

    /// Handle `background_action: "status"|"kill"` against a previously
    /// started process id.
    pub(super) async fn run_background_action(
        &self,
        ctx: &ToolContext,
        action: BackgroundAction,
        process_id: Option<&str>,
    ) -> Result<ToolOutput, ToolError> {
        let process_id = process_id.filter(|s| !s.trim().is_empty()).ok_or_else(|| {
            ToolError::InvalidInput(
                "`background_action` requires `process_id`: the id returned when the process \
                 was started with `run_in_background: true`."
                    .to_owned(),
            )
        })?;
        match action {
            BackgroundAction::Status => {
                let Some((status, command, tail)) =
                    self.background.status(&ctx.session_id, process_id)
                else {
                    return Ok(ToolOutput::error(format!(
                        "No background process `{process_id}` in this session. It may have \
                         never existed, or the session may have been torn down."
                    )));
                };
                let (tail, truncated) = truncate_chars(&tail, MAX_OUTPUT_CHARS);
                let state = if status.running {
                    "running".to_owned()
                } else {
                    format!(
                        "exited (code {})",
                        status
                            .exit_code
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| "terminated_by_signal".to_owned())
                    )
                };
                let rendered = format!(
                    "process {process_id} ({command}): {state}\n\nrecent output:\n{}",
                    if tail.is_empty() {
                        "(none)".to_owned()
                    } else {
                        tail
                    }
                );
                Ok(ToolOutput {
                    content: vec![agentloop_contracts::ToolResultBlock::markdown(rendered)],
                    is_error: false,
                    structured: Some(serde_json::json!({
                        "process_id": process_id,
                        "running": status.running,
                        "exit_code": status.exit_code,
                        "truncated": truncated,
                    })),
                })
            }
            BackgroundAction::Kill => {
                match self.background.kill(&ctx.session_id, process_id).await {
                    Ok(true) => Ok(ToolOutput {
                        content: vec![agentloop_contracts::ToolResultBlock::markdown(format!(
                            "process {process_id}: killed."
                        ))],
                        is_error: false,
                        structured: Some(serde_json::json!({
                            "process_id": process_id,
                            "killed": true,
                        })),
                    }),
                    Ok(false) => Ok(ToolOutput::error(format!(
                        "No background process `{process_id}` in this session. It may have \
                         never existed, already exited, or the session may have been torn down."
                    ))),
                    Err(err) => Err(ToolError::Execution(format!(
                        "failed to kill background process `{process_id}`: {err}."
                    ))),
                }
            }
        }
    }
}
