//! Demotable foreground execution (MOVE-TO-BACKGROUND handoff).

use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;

use agentloop_core::{
    BackgroundEntry, ChunkSink, ExecError, ExecOrDemoted, ExecOutcome, ExecStream,
};

use super::background::{BackgroundState, LocalBackgroundProcess};
use super::foreground::run_command_with_sink;
use super::io::read_and_forward_dual;

/// Same as [`run_command_with_sink`], but also races an optional `demote`
/// token: if it fires before the command exits (and before `cancel`/timeout),
/// the child process and its reader tasks are handed off to a fresh
/// [`BackgroundEntry`] (same shape [`spawn_background`] produces) instead of
/// being waited on, and the accumulated output-so-far is returned alongside
/// it. `label` is reused as the [`BackgroundEntry::command`] is set by the
/// caller (`Bash`), not here — this only builds the process handle.
///
/// Structurally this mirrors [`run_command_with_sink`]'s streaming branch,
/// with two differences forced by the handoff: `child.wait()` is polled
/// directly in the `select!` (not spawned into its own task) so `child`
/// itself is still owned locally and can be moved into the background entry
/// on demote, and the readers mirror into a shared, capped tail buffer (like
/// [`spawn_background`]'s do) rather than a plain accumulator, since a
/// demoted process's tail must be servable by `BackgroundProcess::tail_text`
/// afterward.
pub(crate) async fn run_command_demotable(
    mut command: Command,
    timeout_ms: u64,
    cancel: CancellationToken,
    demote: Option<CancellationToken>,
    label: &str,
    chunk_sink: Option<ChunkSink>,
) -> Result<ExecOrDemoted, ExecError> {
    crate::win_console::hide_console(&mut command);
    let Some(demote) = demote else {
        // No demote signal wired up: identical to the plain streaming path.
        return run_command_with_sink(command, timeout_ms, cancel, label, chunk_sink)
            .await
            .map(ExecOrDemoted::Completed);
    };

    let mut child: Child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| ExecError::Failed(format!("cannot start {label}: {err}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ExecError::Failed(format!("{label}: no stdout pipe")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ExecError::Failed(format!("{label}: no stderr pipe")))?;

    // Shared, capped tail buffer (same shape `spawn_background` uses) so
    // that if this call demotes, the resulting `BackgroundProcess::
    // tail_text()` has something to show immediately, and the `accumulated`
    // text returned alongside the demote is exactly what the model already
    // saw stream past (both streams interleaved in arrival order — matching
    // what `[process exited...]` markers and `background_action: "status"`
    // already show for processes started directly in the background).
    // Readers *also* accumulate their own full, uncapped buffer per stream so
    // the non-demoted `Completed` case stays byte-identical to
    // `run_command_with_sink`: same two separate `Vec<u8>`s, same accumulation
    // rule, only ever capped by `Bash`'s own `MAX_OUTPUT_CHARS` truncation.
    let state = Arc::new(Mutex::new(BackgroundState {
        tail: Vec::new(),
        exit_code: None,
        running: true,
    }));

    let stdout_state = state.clone();
    let stdout_sink = chunk_sink.clone();
    let stdout_task = tokio::spawn(async move {
        read_and_forward_dual(stdout, ExecStream::Stdout, stdout_sink, stdout_state).await
    });
    let stderr_state = state.clone();
    let stderr_sink = chunk_sink.clone();
    let stderr_task = tokio::spawn(async move {
        read_and_forward_dual(stderr, ExecStream::Stderr, stderr_sink, stderr_state).await
    });

    let pid = child.id();
    let status = tokio::select! {
        _ = cancel.cancelled() => {
            let _ = child.start_kill();
            if let Some(pid) = pid {
                crate::process_group::kill_group(pid);
            }
            stdout_task.abort();
            stderr_task.abort();
            return Err(ExecError::Cancelled);
        }
        _ = demote.cancelled() => {
            // Hand off: the child and its reader tasks keep running/streaming
            // exactly as they were — only the bookkeeping changes hands. A
            // fresh cancel token backs the new `LocalBackgroundProcess`'s
            // `kill`, independent of the `cancel` this call was running
            // under (which is scoped to the now-finished tool call).
            let accumulated = {
                let s = state.lock().unwrap_or_else(|p| p.into_inner());
                String::from_utf8_lossy(&s.tail).into_owned()
            };
            let pid = child.id();
            let bg_cancel = CancellationToken::new();
            let wait_state = state.clone();
            let wait_cancel = bg_cancel.clone();
            let sink_for_exit = chunk_sink.clone();
            tokio::spawn(async move {
                let bg_pid = child.id();
                let status = tokio::select! {
                    _ = wait_cancel.cancelled() => {
                        let _ = child.start_kill();
                        if let Some(bg_pid) = bg_pid {
                            crate::process_group::kill_group(bg_pid);
                        }
                        child.wait().await
                    }
                    status = child.wait() => status,
                };
                // Readers hit EOF on their own once the pipes close; join
                // them so the tail buffer reflects everything the process
                // ever printed (their returned per-stream buffers are
                // discarded here — nothing is left waiting on them once
                // demoted, only `tail_text()`/the live sink matter anymore).
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                let mut state = wait_state.lock().unwrap_or_else(|p| p.into_inner());
                state.running = false;
                state.exit_code = status.ok().and_then(|s| s.code());
                if let Some(sink) = &sink_for_exit {
                    let code_text = state
                        .exit_code
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "terminated_by_signal".to_owned());
                    drop(state);
                    sink(
                        ExecStream::Stdout,
                        &format!("\n[process exited with code {code_text}]\n"),
                    );
                }
            });
            return Ok(ExecOrDemoted::Demoted {
                accumulated,
                entry: BackgroundEntry {
                    command: String::new(), // caller (`Bash`) fills in the real command line.
                    started_at_ms: 0, // caller preserves the original `started_at`.
                    handle: Arc::new(LocalBackgroundProcess {
                        pid,
                        state,
                        cancel: bg_cancel,
                    }),
                },
            });
        }
        result = tokio::time::timeout(Duration::from_millis(timeout_ms), child.wait()) => {
            match result {
                Ok(status) => status.map_err(|err| ExecError::Failed(format!(
                    "{label} failed while collecting output: {err}"
                )))?,
                Err(_) => {
                    let _ = child.start_kill();
                    if let Some(pid) = pid {
                        crate::process_group::kill_group(pid);
                    }
                    stdout_task.abort();
                    stderr_task.abort();
                    return Err(ExecError::Timeout(timeout_ms));
                }
            }
        }
    };

    // The child exited on its own without ever being demoted: join the
    // readers for their full per-stream buffers, exactly like
    // `run_command_with_sink`'s streaming branch.
    let stdout_buf = stdout_task
        .await
        .map_err(|err| ExecError::Failed(format!("{label} stdout reader failed: {err}")))?;
    let stderr_buf = stderr_task
        .await
        .map_err(|err| ExecError::Failed(format!("{label} stderr reader failed: {err}")))?;

    Ok(ExecOrDemoted::Completed(ExecOutcome {
        exit_code: status.code(),
        stdout: stdout_buf,
        stderr: stderr_buf,
    }))
}
