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
                    command: String::new(),
                    started_at_ms: 0,
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
