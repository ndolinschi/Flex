//! Foreground command execution with timeout, cancel, and optional streaming.

use std::process::Stdio;
use std::time::Duration;

use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;

use agentloop_core::{ChunkSink, ExecError, ExecOutcome, ExecStream};

use super::io::read_and_forward;

/// Run `command` to completion under `timeout_ms` and `cancel`.
///
/// The command must not have its stdio configured; this sets stdin to null and
/// captures stdout/stderr. `kill_on_drop` guarantees the child dies when the
/// future is dropped (cancellation or timeout).
///
/// When `chunk_sink` is `Some`, stdout/stderr are additionally forwarded to it
/// as they arrive (8KB reads, lossy-UTF8 decoded) while still being
/// accumulated in full for the returned [`ExecOutcome`] — the streaming path
/// is purely additive over the historical behavior. `chunk_sink` is `None` at
/// every call site except the local backend today; other backends simply
/// don't stream yet, which is safe (the final output is unaffected).
pub(crate) async fn run_command(
    command: Command,
    timeout_ms: u64,
    cancel: CancellationToken,
    label: &str,
) -> Result<ExecOutcome, ExecError> {
    run_command_with_sink(command, timeout_ms, cancel, label, None).await
}

/// Same as [`run_command`], but takes an optional [`ChunkSink`] for
/// incremental stdout/stderr forwarding. See [`run_command`] for semantics.
pub(crate) async fn run_command_with_sink(
    mut command: Command,
    timeout_ms: u64,
    cancel: CancellationToken,
    label: &str,
    chunk_sink: Option<ChunkSink>,
) -> Result<ExecOutcome, ExecError> {
    crate::win_console::hide_console(&mut command);
    let Some(sink) = chunk_sink else {
        // No sink: keep the original, simplest path untouched.
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|err| ExecError::Failed(format!("cannot start {label}: {err}")))?;
        let pid = child.id();

        let mut wait_task = tokio::spawn(async move { child.wait_with_output().await });
        let output = tokio::select! {
            _ = cancel.cancelled() => {
                wait_task.abort();
                if let Some(pid) = pid {
                    crate::process_group::kill_group(pid);
                }
                return Err(ExecError::Cancelled);
            }
            result = tokio::time::timeout(Duration::from_millis(timeout_ms), &mut wait_task) => {
                match result {
                    Ok(join) => join
                        .map_err(|err| ExecError::Failed(format!(
                            "{label} worker failed before producing output: {err}"
                        )))?
                        .map_err(|err| ExecError::Failed(format!(
                            "{label} failed while collecting output: {err}"
                        )))?,
                    Err(_) => {
                        wait_task.abort();
                        if let Some(pid) = pid {
                            crate::process_group::kill_group(pid);
                        }
                        return Err(ExecError::Timeout(timeout_ms));
                    }
                }
            }
        };

        return Ok(ExecOutcome {
            exit_code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        });
    };

    // Streaming path: pipe stdio, read both streams chunk-by-chunk, forward
    // to the sink, and accumulate into buffers for the final result.
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

    let stdout_sink = sink.clone();
    let stdout_task =
        tokio::spawn(
            async move { read_and_forward(stdout, ExecStream::Stdout, stdout_sink).await },
        );
    let stderr_sink = sink.clone();
    let stderr_task =
        tokio::spawn(
            async move { read_and_forward(stderr, ExecStream::Stderr, stderr_sink).await },
        );

    let pid = child.id();
    let mut wait_task = tokio::spawn(async move { child.wait().await });

    let status = tokio::select! {
        _ = cancel.cancelled() => {
            wait_task.abort();
            stdout_task.abort();
            stderr_task.abort();
            if let Some(pid) = pid {
                crate::process_group::kill_group(pid);
            }
            return Err(ExecError::Cancelled);
        }
        result = tokio::time::timeout(Duration::from_millis(timeout_ms), &mut wait_task) => {
            match result {
                Ok(join) => join
                    .map_err(|err| ExecError::Failed(format!(
                        "{label} worker failed before producing output: {err}"
                    )))?
                    .map_err(|err| ExecError::Failed(format!(
                        "{label} failed while collecting output: {err}"
                    )))?,
                Err(_) => {
                    wait_task.abort();
                    stdout_task.abort();
                    stderr_task.abort();
                    if let Some(pid) = pid {
                        crate::process_group::kill_group(pid);
                    }
                    return Err(ExecError::Timeout(timeout_ms));
                }
            }
        }
    };

    // The child has exited, so its pipes will hit EOF; join the read loops to
    // collect the full accumulated buffers (bounded wait — pipes close once
    // the process is gone).
    let stdout_buf = stdout_task
        .await
        .map_err(|err| ExecError::Failed(format!("{label} stdout reader failed: {err}")))?;
    let stderr_buf = stderr_task
        .await
        .map_err(|err| ExecError::Failed(format!("{label} stderr reader failed: {err}")))?;

    Ok(ExecOutcome {
        exit_code: status.code(),
        stdout: stdout_buf,
        stderr: stderr_buf,
    })
}
