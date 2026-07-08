//! Shared process plumbing: spawn a prepared command, enforce timeout and
//! cancellation, collect output. Every backend funnels through here so the
//! cancel/timeout semantics are identical across backends.

use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use agentloop_core::{ExecError, ExecOutcome};

/// Run `command` to completion under `timeout_ms` and `cancel`.
///
/// The command must not have its stdio configured; this sets stdin to null and
/// captures stdout/stderr. `kill_on_drop` guarantees the child dies when the
/// future is dropped (cancellation or timeout).
pub(crate) async fn run_command(
    mut command: Command,
    timeout_ms: u64,
    cancel: CancellationToken,
    label: &str,
) -> Result<ExecOutcome, ExecError> {
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

    Ok(ExecOutcome {
        exit_code: output.status.code(),
        stdout: output.stdout,
        stderr: output.stderr,
    })
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
