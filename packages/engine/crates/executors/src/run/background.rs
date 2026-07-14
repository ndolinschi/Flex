//! Background process spawn and [`LocalBackgroundProcess`].

use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;

use agentloop_core::{
    BackgroundProcess, BackgroundSpawn, BackgroundStatus, ChunkSink, ExecError, ExecStream,
};

use super::io::read_and_forward_background;

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

/// Shared, lock-protected state a spawned background process's reader tasks
/// update and [`LocalBackgroundProcess`] reads back for `status`/tail lines.
pub(super) struct BackgroundState {
    pub(super) tail: Vec<u8>,
    pub(super) exit_code: Option<i32>,
    pub(super) running: bool,
}

/// [`BackgroundProcess`] impl for the local backend: holds the child's pid,
/// the shared state the reader/wait tasks update, and the means to kill it.
pub(crate) struct LocalBackgroundProcess {
    pub(super) pid: Option<u32>,
    pub(super) state: Arc<Mutex<BackgroundState>>,
    pub(super) cancel: CancellationToken,
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
