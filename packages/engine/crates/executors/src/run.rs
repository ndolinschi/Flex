//! Shared process plumbing: spawn a prepared command, enforce timeout and
//! cancellation, collect output. Every backend funnels through here so the
//! cancel/timeout semantics are identical across backends.

use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;

use agentloop_core::{ChunkSink, ExecError, ExecOutcome, ExecStream};

/// Size of each incremental read. Chunk-based (not line-based): a command
/// that never emits a newline (a progress spinner, a long single-line log)
/// still streams instead of stalling until EOF.
const CHUNK_BUF_SIZE: usize = 8 * 1024;

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
    let Some(sink) = chunk_sink else {
        // No sink: keep the original, simplest path untouched.
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|err| ExecError::Failed(format!("cannot start {label}: {err}")))?;

        let mut wait_task = tokio::spawn(async move { child.wait_with_output().await });
        let output = tokio::select! {
            _ = cancel.cancelled() => {
                wait_task.abort();
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

    let mut wait_task = tokio::spawn(async move { child.wait().await });

    let status = tokio::select! {
        _ = cancel.cancelled() => {
            wait_task.abort();
            stdout_task.abort();
            stderr_task.abort();
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

/// Read one pipe to EOF in 8KB chunks, forwarding each chunk to `sink` (lossy
/// UTF-8) while accumulating the raw bytes for the final result.
async fn read_and_forward<R>(mut reader: R, stream: ExecStream, sink: ChunkSink) -> Vec<u8>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut buf = vec![0u8; CHUNK_BUF_SIZE];
    let mut acc = Vec::new();
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                acc.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&buf[..n]);
                sink(stream, &text);
            }
            Err(_) => break,
        }
    }
    acc
}

/// Probe helper: run `binary --version`-style invocation and summarize.
pub(crate) async fn probe_binary(binary: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|err| format!("`{binary}` is not runnable: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "`{binary} {}` exited non-zero: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let first_line = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    Ok(first_line)
}
