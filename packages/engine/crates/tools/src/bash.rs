//! `Bash`: run a shell command in the session cwd through the composed
//! execution backend.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::{AgentEvent, ExecStream as WireExecStream, ToolOutput};
use agentloop_core::{
    BackgroundEntry, BackgroundProcessRegistry, ChunkSink, DemoteRegistry, ExecError,
    ExecOrDemoted, ExecSpec, ExecStream, Executor, NetworkPolicy, PermissionHint, Tool,
    ToolCategory, ToolContext, ToolDescriptor, ToolError,
};

use crate::fs::{schema_of, truncate_chars};

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MAX_TIMEOUT_MS: u64 = 600_000;
const MAX_OUTPUT_CHARS: usize = 120_000;

/// Build a [`ChunkSink`] that emits `AgentEvent::ExecChunk` for every
/// incremental chunk a running command produces, mapping the executor's
/// wire-format-free `agentloop_core::ExecStream` onto the wire enum
/// (`agentloop_contracts::ExecStream`) — the only layer that is allowed to
/// know about both is this one (`tools` depends on `core` and `contracts`;
/// `executors` depends on `core` alone).
///
/// Streaming stops once the running total exceeds `MAX_OUTPUT_CHARS`: the
/// executor keeps accumulating the full output for the final, still-truncated
/// `ToolOutput` (unchanged from today), but there is no point flooding live
/// subscribers past the cap the final render already enforces.
fn exec_chunk_sink(ctx: &ToolContext) -> ChunkSink {
    let events = ctx.events.clone();
    let call_id = ctx.call_id.clone();
    let emitted = Arc::new(AtomicUsize::new(0));
    Arc::new(move |stream, text| {
        if emitted.load(Ordering::Relaxed) > MAX_OUTPUT_CHARS {
            return;
        }
        let previous = emitted.fetch_add(text.chars().count(), Ordering::Relaxed);
        if previous > MAX_OUTPUT_CHARS {
            return;
        }
        let stream = match stream {
            ExecStream::Stdout => WireExecStream::Stdout,
            ExecStream::Stderr => WireExecStream::Stderr,
            // `ExecStream` is `#[non_exhaustive]`: an unrecognized future
            // stream kind is treated as stdout rather than dropped or
            // panicking.
            _ => WireExecStream::Stdout,
        };
        events.emit(AgentEvent::ExecChunk {
            call_id: call_id.clone(),
            stream,
            text: text.to_owned(),
        });
    })
}

/// What to do with an already-started background process, named by the id
/// returned when it was started (see [`BashInput::process_id`]).
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum BackgroundAction {
    /// Report whether the process is still running plus its recent output.
    Status,
    /// Terminate the process.
    Kill,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BashInput {
    /// Shell command to run in the session cwd. Not required when `action`
    /// targets an already-started background process (`background_action` +
    /// `process_id`).
    command: Option<String>,
    /// Optional timeout in milliseconds. Defaults to 30000, capped at 600000.
    /// Ignored for `run_in_background: true` (see that field).
    timeout_ms: Option<u64>,
    /// Start long-running processes (dev servers, watchers, tail -f, etc.)
    /// in the background instead of blocking: the call returns once the
    /// process's initial output settles (a few seconds), with a process id
    /// and whatever it printed on startup. The process keeps running and
    /// keeps streaming output to the agent terminal after the call returns;
    /// use `background_action: "status"` with that id later to check on it,
    /// or `"kill"` to stop it. Defaults to `false` (blocking, byte-identical
    /// to today's behavior).
    #[serde(default)]
    run_in_background: bool,
    /// Check on or stop a background process previously started with
    /// `run_in_background: true`. Requires `process_id`; `command` is
    /// ignored when this is set.
    background_action: Option<BackgroundAction>,
    /// The process id returned by the `run_in_background: true` call that
    /// started it. Required with `background_action`.
    process_id: Option<String>,
}

/// Execute a shell command with `sh -lc` semantics through the injected
/// [`Executor`] backend (local process by default; container/remote backends
/// at composition time). Also fields `background_action`/`process_id` for
/// checking on or stopping a process a prior `run_in_background: true` call
/// started — kept on the same tool (rather than a separate one) since both
/// paths share the executor, the process id namespace, and the model's
/// mental model of "the shell tool."
pub struct BashTool {
    executor: Arc<dyn Executor>,
    network: NetworkPolicy,
    background: Arc<BackgroundProcessRegistry>,
    /// Per-call demote signals for still-running **foreground** calls (see
    /// `MOVE-TO-BACKGROUND`): registered for the duration of one blocking
    /// `exec_demotable` call and unregistered the instant it returns, either
    /// way. Shared with the composition root the same way `background` is,
    /// so a `background_demote` Tauri command (via `EngineService`) can reach
    /// it without holding a second `BashTool`.
    demote: Arc<DemoteRegistry>,
}

impl BashTool {
    pub fn new(executor: Arc<dyn Executor>) -> Self {
        Self {
            executor,
            network: NetworkPolicy::Allowed,
            background: Arc::new(BackgroundProcessRegistry::new()),
            demote: Arc::new(DemoteRegistry::new()),
        }
    }

    /// Set the network posture every command runs under.
    pub fn with_network(mut self, network: NetworkPolicy) -> Self {
        self.network = network;
        self
    }

    /// Share a background-process registry rather than owning a private one
    /// — lets the composition root (session teardown, engine drop) reach the
    /// same table this tool registers into.
    pub fn with_background_registry(mut self, registry: Arc<BackgroundProcessRegistry>) -> Self {
        self.background = registry;
        self
    }

    /// Share a demote registry rather than owning a private one — lets the
    /// composition root's `background_demote` reach the same table this
    /// tool's foreground exec path registers into.
    pub fn with_demote_registry(mut self, registry: Arc<DemoteRegistry>) -> Self {
        self.demote = registry;
        self
    }

    /// The shared registry, for composition roots that need to kill sessions'
    /// background processes on teardown without holding a second `BashTool`.
    pub fn background_registry(&self) -> Arc<BackgroundProcessRegistry> {
        self.background.clone()
    }

    /// The shared demote registry, for composition roots wiring up
    /// `background_demote` without holding a second `BashTool`.
    pub fn demote_registry(&self) -> Arc<DemoteRegistry> {
        self.demote.clone()
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

        // Register a demote handle for the duration of this blocking call —
        // `background_demote` (via `DemoteRegistry::request_demote`) can fire
        // it any time between now and the `unregister` below. Backends that
        // don't implement `exec_demotable` (docker, ssh, …) ignore
        // `spec.demote` entirely, so the registration is harmless dead
        // weight for them, not a behavior change.
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
                // Fill in the bookkeeping the executor left for us (it only
                // knows the process handle, not the originating call's
                // command line or true start time).
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
                    // Same shape as `run_in_background`'s structured result
                    // (`process_id`/`pid`/`running`) so the desktop UI's
                    // background-row detection doesn't need a second code
                    // path for a demoted call vs. one started in the
                    // background from the outset — see `ToolStepGroup`'s
                    // `isBackgroundBashCall`.
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

impl BashTool {
    /// Start `command` detached: spawn through the executor's
    /// `exec_background`, wait for the backend's deterministic
    /// initial-output window, register the handle under the originating
    /// call id (stable, unique, and already known to the model as "this
    /// call"), and return immediately. The process keeps running and
    /// streaming into `ctx.events` (same `call_id`) via the sink passed to
    /// the executor — unaffected by this tool call having already returned.
    async fn run_in_background(
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
    async fn run_background_action(
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

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::{SessionId, ToolCallId, TurnId};
    use agentloop_core::EventSink;
    use agentloop_executors::LocalExecutor;
    use tokio_util::sync::CancellationToken;

    fn ctx() -> ToolContext {
        let (events, _rx) = EventSink::channel();
        ToolContext {
            session_id: SessionId::from("sess-test"),
            turn_id: TurnId::from("turn-test"),
            call_id: ToolCallId::from("call-test"),
            cwd: std::path::PathBuf::from("."),
            cancel: CancellationToken::new(),
            events,
        }
    }

    fn bash_tool() -> BashTool {
        BashTool::new(Arc::new(LocalExecutor))
    }

    /// Concatenate every markdown block's text — test helper only; real
    /// callers of `ToolOutput` render the whole `content` vec.
    fn markdown_text(output: &ToolOutput) -> String {
        output
            .content
            .iter()
            .map(|block| match block {
                agentloop_contracts::ToolResultBlock::Markdown { text } => text.as_str(),
                _ => "",
            })
            .collect()
    }

    #[tokio::test]
    async fn foreground_path_is_unchanged() {
        let tool = bash_tool();
        let output = tool
            .run(ctx(), serde_json::json!({"command": "printf hello"}))
            .await
            .expect("run ok");
        assert!(!output.is_error);
        let text = markdown_text(&output);
        assert!(text.contains("hello"));
        assert_eq!(
            output.structured.as_ref().and_then(|s| s.get("success")),
            Some(&serde_json::Value::Bool(true))
        );
    }

    #[tokio::test]
    async fn empty_command_is_rejected() {
        let tool = bash_tool();
        let err = tool
            .run(ctx(), serde_json::json!({"command": "   "}))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn background_run_returns_early_with_a_process_id_while_still_running() {
        let tool = bash_tool();
        let call_ctx = ctx();
        let output = tool
            .run(
                call_ctx,
                serde_json::json!({"command": "echo ready; sleep 5", "run_in_background": true}),
            )
            .await
            .expect("run ok");
        assert!(!output.is_error);
        let text = markdown_text(&output);
        assert!(text.contains("Started background process"));
        assert!(text.contains("ready"));
        let structured = output.structured.expect("structured result");
        assert_eq!(
            structured.get("running"),
            Some(&serde_json::Value::Bool(true))
        );
        let process_id = structured
            .get("process_id")
            .and_then(|v| v.as_str())
            .expect("process_id present")
            .to_owned();

        // Clean up: kill it through the same control surface the model uses.
        let kill_ctx = ctx();
        let kill_output = tool
            .run(
                kill_ctx,
                serde_json::json!({"background_action": "kill", "process_id": process_id}),
            )
            .await
            .expect("kill ok");
        assert!(!kill_output.is_error);
    }

    #[tokio::test]
    async fn background_status_reports_running_then_kill_stops_it() {
        let tool = bash_tool();
        let start = tool
            .run(
                ctx(),
                serde_json::json!({"command": "sleep 5", "run_in_background": true}),
            )
            .await
            .expect("run ok");
        let process_id = start
            .structured
            .as_ref()
            .and_then(|s| s.get("process_id"))
            .and_then(|v| v.as_str())
            .expect("process_id present")
            .to_owned();

        let status = tool
            .run(
                ctx(),
                serde_json::json!({"background_action": "status", "process_id": process_id}),
            )
            .await
            .expect("status ok");
        assert!(!status.is_error);
        assert_eq!(
            status.structured.as_ref().and_then(|s| s.get("running")),
            Some(&serde_json::Value::Bool(true))
        );

        let kill = tool
            .run(
                ctx(),
                serde_json::json!({"background_action": "kill", "process_id": process_id}),
            )
            .await
            .expect("kill ok");
        assert!(!kill.is_error);
        assert_eq!(
            kill.structured.as_ref().and_then(|s| s.get("killed")),
            Some(&serde_json::Value::Bool(true))
        );
    }

    /// `background_action: "kill"` must take down the whole process tree a
    /// backgrounded command started, not just the `/bin/sh` wrapper: this
    /// backgrounds a shell that forks a `sleep` grandchild and prints its
    /// pid, kills the tracked process, then confirms the grandchild pid is
    /// actually gone (not just reparented and still running) via `kill(pid,
    /// 0)`. Guards against a regression to killing only the immediate child.
    #[tokio::test]
    async fn kill_terminates_the_whole_process_group_not_just_the_shell() {
        let tool = bash_tool();
        let start = tool
            .run(
                ctx(),
                serde_json::json!({
                    "command": "sleep 30 & echo $!; wait",
                    "run_in_background": true,
                }),
            )
            .await
            .expect("run ok");
        let process_id = start
            .structured
            .as_ref()
            .and_then(|s| s.get("process_id"))
            .and_then(|v| v.as_str())
            .expect("process_id present")
            .to_owned();

        // Poll the tracked tail for the echoed grandchild pid rather than a
        // fixed sleep guess.
        let mut grandchild_pid: Option<u32> = None;
        for _ in 0..100 {
            let status = tool
                .run(
                    ctx(),
                    serde_json::json!({"background_action": "status", "process_id": process_id}),
                )
                .await
                .expect("status ok");
            let tail = markdown_text(&status);
            if let Some(pid) = tail
                .trim()
                .lines()
                .next_back()
                .and_then(|l| l.trim().parse().ok())
            {
                grandchild_pid = Some(pid);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let grandchild_pid = grandchild_pid.expect("grandchild pid observed in tail output");
        assert!(
            process_is_alive(grandchild_pid),
            "grandchild should be running before kill"
        );

        let kill = tool
            .run(
                ctx(),
                serde_json::json!({"background_action": "kill", "process_id": process_id}),
            )
            .await
            .expect("kill ok");
        assert!(!kill.is_error);

        let mut still_alive = true;
        for _ in 0..100 {
            if !process_is_alive(grandchild_pid) {
                still_alive = false;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(
            !still_alive,
            "killing the tracked background process must also kill its \
             grandchild `sleep`, not leave it orphaned and running"
        );
    }

    /// Portable liveness probe: `kill(pid, 0)` sends no signal, only checks
    /// whether the process (or an unreaped zombie) still exists.
    #[cfg(unix)]
    fn process_is_alive(pid: u32) -> bool {
        // SAFETY: none needed — this crate has no `unsafe_code` allowance,
        // so shell out to `kill -0` instead of linking a signals crate just
        // for a one-line test probe.
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    #[tokio::test]
    async fn status_for_unknown_process_id_is_a_tool_error_not_a_panic() {
        let tool = bash_tool();
        let output = tool
            .run(
                ctx(),
                serde_json::json!({"background_action": "status", "process_id": "no-such-id"}),
            )
            .await
            .expect("run ok");
        assert!(output.is_error);
    }

    #[tokio::test]
    async fn kill_for_unknown_process_id_reports_not_killed_rather_than_erroring() {
        let tool = bash_tool();
        let output = tool
            .run(
                ctx(),
                serde_json::json!({"background_action": "kill", "process_id": "no-such-id"}),
            )
            .await
            .expect("run ok");
        assert!(output.is_error);
    }

    #[tokio::test]
    async fn background_action_without_process_id_is_invalid_input() {
        let tool = bash_tool();
        let err = tool
            .run(ctx(), serde_json::json!({"background_action": "status"}))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn session_teardown_kills_background_processes() {
        let registry = Arc::new(BackgroundProcessRegistry::new());
        let tool = bash_tool().with_background_registry(registry.clone());
        let session = SessionId::from("sess-teardown");
        let (events, _rx) = EventSink::channel();
        let call_ctx = ToolContext {
            session_id: session.clone(),
            turn_id: TurnId::from("turn-test"),
            call_id: ToolCallId::from("call-test"),
            cwd: std::path::PathBuf::from("."),
            cancel: CancellationToken::new(),
            events,
        };
        let start = tool
            .run(
                call_ctx,
                serde_json::json!({"command": "sleep 5", "run_in_background": true}),
            )
            .await
            .expect("run ok");
        let process_id = start
            .structured
            .as_ref()
            .and_then(|s| s.get("process_id"))
            .and_then(|v| v.as_str())
            .expect("process_id present")
            .to_owned();
        assert!(registry.status(&session, &process_id).is_some());

        registry.kill_session(&session).await;

        // Give the wait task a moment to observe the cancellation.
        for _ in 0..50 {
            match registry.status(&session, &process_id) {
                None => break,
                Some(_) => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
            }
        }
        assert!(
            registry.status(&session, &process_id).is_none(),
            "teardown must remove the session's entries from the registry"
        );
    }

    /// Demoting a still-running foreground call returns early with the
    /// "moved to background" notice + output accumulated so far, and the
    /// process shows up in the shared background registry as still running
    /// — from there `background_action: "kill"` (the same control surface a
    /// process started via `run_in_background` uses) works exactly as it
    /// would on any other background entry.
    #[tokio::test]
    async fn demote_mid_run_returns_early_and_process_stays_running() {
        let background = Arc::new(BackgroundProcessRegistry::new());
        let demote = Arc::new(DemoteRegistry::new());
        let tool = bash_tool()
            .with_background_registry(background.clone())
            .with_demote_registry(demote.clone());
        let session = SessionId::from("sess-demote");
        let call_id = ToolCallId::from("call-demote");
        let (events, _rx) = EventSink::channel();
        let call_ctx = ToolContext {
            session_id: session.clone(),
            turn_id: TurnId::from("turn-test"),
            call_id: call_id.clone(),
            cwd: std::path::PathBuf::from("."),
            cancel: CancellationToken::new(),
            events,
        };

        let run = tokio::spawn(async move {
            tool.run(
                call_ctx,
                serde_json::json!({"command": "echo ready; sleep 5"}),
            )
            .await
        });

        // Give the command time to start and register its demote handle,
        // then fire the demote — polling rather than a fixed sleep guess so
        // the test isn't flaky under load.
        let mut demoted = false;
        for _ in 0..100 {
            if demote.request_demote(&session, call_id.as_str()) {
                demoted = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(
            demoted,
            "expected the running call to register a demote handle"
        );

        let output = run
            .await
            .expect("task join ok")
            .expect("run ok after demote");
        assert!(!output.is_error);
        let text = markdown_text(&output);
        assert!(text.contains("Moved to background"));
        assert!(text.contains(call_id.as_str()));

        let structured = output.structured.expect("structured result");
        assert_eq!(
            structured.get("process_id").and_then(|v| v.as_str()),
            Some(call_id.as_str()),
        );
        assert_eq!(
            structured.get("running"),
            Some(&serde_json::Value::Bool(true))
        );

        // The process itself is now tracked as a normal background entry,
        // under the very call id the foreground call was running as.
        let (status, command, _tail) = background
            .status(&session, call_id.as_str())
            .expect("registered in the background registry after demote");
        assert!(status.running);
        assert_eq!(command, "echo ready; sleep 5");

        // Kill it through the same control surface a `run_in_background`
        // process uses, cleaning up the still-sleeping child.
        let killed = background
            .kill(&session, call_id.as_str())
            .await
            .expect("kill ok");
        assert!(killed);
    }

    /// A demote request for a call that already finished naturally (or was
    /// never running) is a no-op: `request_demote` returns `false`, and the
    /// call's own result is completely unaffected (no "moved to background"
    /// framing).
    #[tokio::test]
    async fn demote_after_natural_completion_is_a_noop() {
        let demote = Arc::new(DemoteRegistry::new());
        let tool = bash_tool().with_demote_registry(demote.clone());
        let session = SessionId::from("sess-demote-late");
        let call_id = ToolCallId::from("call-demote-late");
        let (events, _rx) = EventSink::channel();
        let call_ctx = ToolContext {
            session_id: session.clone(),
            turn_id: TurnId::from("turn-test"),
            call_id: call_id.clone(),
            cwd: std::path::PathBuf::from("."),
            cancel: CancellationToken::new(),
            events,
        };

        let output = tool
            .run(call_ctx, serde_json::json!({"command": "printf hello"}))
            .await
            .expect("run ok");
        assert!(!output.is_error);
        assert!(!markdown_text(&output).contains("Moved to background"));

        // The registration was removed the instant the call finished, so a
        // demote request that arrives after the fact finds nothing to signal.
        let demoted = demote.request_demote(&session, call_id.as_str());
        assert!(
            !demoted,
            "demoting an already-finished call must be a no-op"
        );
    }

    /// A normal (non-demoted) run through the demotable path stays
    /// byte-identical to the pre-demote behavior: same stdout/stderr split,
    /// same exit code, no "Moved to background" framing anywhere.
    #[tokio::test]
    async fn non_demoted_foreground_path_is_unaffected_by_demote_plumbing() {
        let tool = bash_tool();
        let output = tool
            .run(
                ctx(),
                serde_json::json!({"command": "printf out; printf err 1>&2"}),
            )
            .await
            .expect("run ok");
        assert!(!output.is_error);
        let text = markdown_text(&output);
        assert!(text.contains("stdout:\nout"));
        assert!(text.contains("stderr:\nerr"));
        assert!(!text.contains("Moved to background"));
        assert_eq!(
            output.structured.as_ref().and_then(|s| s.get("success")),
            Some(&serde_json::Value::Bool(true))
        );
    }
}
