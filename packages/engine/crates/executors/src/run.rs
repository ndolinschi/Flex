//! Shared process plumbing: spawn a prepared command, enforce timeout and
//! cancellation, collect output. Every backend funnels through here so the
//! cancel/timeout semantics are identical across backends.

use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;

use agentloop_core::{
    BackgroundEntry, BackgroundProcess, BackgroundSpawn, BackgroundStatus, ChunkSink, ExecError,
    ExecOrDemoted, ExecOutcome, ExecStream,
};

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

/// Read one pipe to EOF in 8KB chunks: forwards each chunk to `sink` (lossy
/// UTF-8), mirrors it into the shared capped tail buffer (see
/// [`TAIL_BUFFER_CAP_BYTES`]) for a possible demote handoff, and separately
/// accumulates the complete, uncapped bytes to return if the call finishes
/// normally — the dual bookkeeping [`run_command_demotable`] needs since it
/// doesn't know at read time whether this call will demote.
async fn read_and_forward_dual<R>(
    mut reader: R,
    stream: ExecStream,
    sink: Option<ChunkSink>,
    state: Arc<Mutex<BackgroundState>>,
) -> Vec<u8>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut buf = vec![0u8; CHUNK_BUF_SIZE];
    let mut acc = Vec::new();
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let chunk = &buf[..n];
                acc.extend_from_slice(chunk);
                if let Some(sink) = &sink {
                    let text = String::from_utf8_lossy(chunk);
                    sink(stream, &text);
                }
                let mut state = state.lock().unwrap_or_else(|p| p.into_inner());
                state.tail.extend_from_slice(chunk);
                if state.tail.len() > TAIL_BUFFER_CAP_BYTES {
                    let overflow = state.tail.len() - TAIL_BUFFER_CAP_BYTES;
                    state.tail.drain(0..overflow);
                }
            }
            Err(_) => break,
        }
    }
    acc
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

/// Wall-clock cap on the initial-output window for a background process (see
/// [`spawn_background`]): after this long, return to the caller regardless of
/// whether output is still arriving. Long enough to catch an immediate crash
/// or startup error from most dev servers/watchers; short enough that the
/// tool call itself never risks the pool's per-call timeout.
const INITIAL_OUTPUT_MAX_MS: u64 = 3_000;

/// Quiet-gap rule: once *any* output has arrived, a gap this long with no
/// further output is treated as "settled" and ends the initial window early
/// — most dev servers print their "ready" banner and then go quiet, so this
/// lets the common case return well under [`INITIAL_OUTPUT_MAX_MS`].
const INITIAL_OUTPUT_QUIET_GAP_MS: u64 = 500;

/// Cap on the accumulated tail buffer, in bytes. Older bytes are dropped as
/// new ones arrive (ring-buffer-by-truncation) so a chatty long-running
/// process can't grow the registry entry unbounded.
const TAIL_BUFFER_CAP_BYTES: usize = 16 * 1024;

/// Shared, lock-protected state a spawned background process's reader tasks
/// update and [`LocalBackgroundProcess`] reads back for `status`/tail lines.
struct BackgroundState {
    tail: Vec<u8>,
    exit_code: Option<i32>,
    running: bool,
}

/// [`BackgroundProcess`] impl for the local backend: holds the child's pid,
/// the shared state the reader/wait tasks update, and the means to kill it.
pub(crate) struct LocalBackgroundProcess {
    pid: Option<u32>,
    state: Arc<Mutex<BackgroundState>>,
    cancel: CancellationToken,
}

#[async_trait::async_trait]
impl BackgroundProcess for LocalBackgroundProcess {
    fn status(&self) -> BackgroundStatus {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        BackgroundStatus {
            running: state.running,
            exit_code: state.exit_code,
            pid: self.pid,
        }
    }

    fn tail_text(&self) -> String {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        String::from_utf8_lossy(&state.tail).into_owned()
    }

    async fn kill(&self) -> Result<(), ExecError> {
        // Tripping the cancel token is enough: the wait task below races the
        // child's exit against this token and calls `Child::kill` on the
        // cancel branch. Killing an already-exited process is a no-op
        // because the wait task has already flipped `running` to false and
        // won't observe the cancellation (it already returned).
        self.cancel.cancel();
        Ok(())
    }
}

/// Start `command` and return once its initial output window closes (see
/// [`INITIAL_OUTPUT_MAX_MS`]/[`INITIAL_OUTPUT_QUIET_GAP_MS`]), handing back
/// the process handle plus whatever stdout/stderr text arrived in that
/// window. The process is **not** waited on further here — a detached task
/// keeps reading both pipes for the process's entire life, forwarding every
/// chunk to `chunk_sink` (if set) and mirroring it into the capped tail
/// buffer the handle's `status` (via the registry) can report later.
pub(crate) async fn spawn_background(
    mut command: Command,
    label: &str,
    chunk_sink: Option<ChunkSink>,
) -> Result<BackgroundSpawn, ExecError> {
    crate::win_console::hide_console(&mut command);
    let mut child: Child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| ExecError::Failed(format!("cannot start {label}: {err}")))?;
    let pid = child.id();

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ExecError::Failed(format!("{label}: no stdout pipe")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ExecError::Failed(format!("{label}: no stderr pipe")))?;

    // Initial-output collection: both pipes feed one channel of
    // (stream, chunk) so the window can apply one deterministic quiet-gap
    // clock across both streams combined, not one per stream.
    let (initial_tx, mut initial_rx) =
        tokio::sync::mpsc::unbounded_channel::<(ExecStream, Vec<u8>)>();

    let state = Arc::new(Mutex::new(BackgroundState {
        tail: Vec::new(),
        exit_code: None,
        running: true,
    }));

    let stdout_tx = initial_tx.clone();
    let stdout_state = state.clone();
    let stdout_sink = chunk_sink.clone();
    let stdout_task = tokio::spawn(async move {
        read_and_forward_background(
            stdout,
            ExecStream::Stdout,
            stdout_sink,
            stdout_state,
            Some(stdout_tx),
        )
        .await;
    });
    let stderr_state = state.clone();
    let stderr_sink = chunk_sink.clone();
    let stderr_task = tokio::spawn(async move {
        read_and_forward_background(
            stderr,
            ExecStream::Stderr,
            stderr_sink,
            stderr_state,
            Some(initial_tx),
        )
        .await;
    });

    let cancel = CancellationToken::new();
    let wait_state = state.clone();
    let wait_cancel = cancel.clone();
    tokio::spawn(async move {
        let status = tokio::select! {
            _ = wait_cancel.cancelled() => {
                let _ = child.start_kill();
                if let Some(pid) = child.id() {
                    crate::process_group::kill_group(pid);
                }
                child.wait().await
            }
            status = child.wait() => status,
        };
        // Readers hit EOF on their own once the pipes close; join them so
        // the tail buffer reflects everything the process ever printed.
        let _ = stdout_task.await;
        let _ = stderr_task.await;
        let mut state = wait_state.lock().unwrap_or_else(|p| p.into_inner());
        state.running = false;
        state.exit_code = status.ok().and_then(|s| s.code());
        if let Some(sink) = &chunk_sink {
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

    // Collect the initial window: read from the combined channel until
    // either the wall-clock cap elapses or a quiet gap follows first output.
    let mut initial = String::new();
    let deadline = tokio::time::sleep(Duration::from_millis(INITIAL_OUTPUT_MAX_MS));
    tokio::pin!(deadline);
    let mut seen_any = false;
    loop {
        let gap = tokio::time::sleep(Duration::from_millis(INITIAL_OUTPUT_QUIET_GAP_MS));
        tokio::pin!(gap);
        tokio::select! {
            _ = &mut deadline => break,
            _ = &mut gap, if seen_any => break,
            item = initial_rx.recv() => match item {
                Some((_, bytes)) => {
                    seen_any = true;
                    initial.push_str(&String::from_utf8_lossy(&bytes));
                }
                None => break,
            },
        }
    }

    Ok(BackgroundSpawn {
        handle: Arc::new(LocalBackgroundProcess { pid, state, cancel }),
        initial_output: initial,
    })
}

/// Like [`read_and_forward`], but for a detached background process: mirrors
/// each chunk into the shared, capped tail buffer (instead of an unbounded
/// accumulator returned to a waiting caller) and optionally forwards the raw
/// bytes to `initial_tx` for the caller collecting the initial-output window.
async fn read_and_forward_background<R>(
    mut reader: R,
    stream: ExecStream,
    sink: Option<ChunkSink>,
    state: Arc<Mutex<BackgroundState>>,
    initial_tx: Option<tokio::sync::mpsc::UnboundedSender<(ExecStream, Vec<u8>)>>,
) where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut buf = vec![0u8; CHUNK_BUF_SIZE];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let chunk = &buf[..n];
                if let Some(sink) = &sink {
                    let text = String::from_utf8_lossy(chunk);
                    sink(stream, &text);
                }
                {
                    let mut state = state.lock().unwrap_or_else(|p| p.into_inner());
                    state.tail.extend_from_slice(chunk);
                    if state.tail.len() > TAIL_BUFFER_CAP_BYTES {
                        let overflow = state.tail.len() - TAIL_BUFFER_CAP_BYTES;
                        state.tail.drain(0..overflow);
                    }
                }
                if let Some(tx) = &initial_tx {
                    let _ = tx.send((stream, chunk.to_vec()));
                }
            }
            Err(_) => break,
        }
    }
}

/// Probe helper: run `binary --version`-style invocation and summarize.
pub(crate) async fn probe_binary(binary: &str, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new(binary);
    crate::win_console::hide_console(&mut command);
    let output = command
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
