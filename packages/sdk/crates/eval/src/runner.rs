//! Per-(task × target) execution: workspace prep, one agent turn under a
//! timeout, check evaluation, metrics, and a raw JSONL event dump.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use agentloop_contracts::{NewSessionParams, PermissionMode, PromptInput, TurnOptions};
use agentloop_engine::EngineService;

use crate::error::EvalError;
use crate::metrics::RunMetrics;
use crate::task::{CheckSpec, TaskSpec};

/// One agent/provider/model combination a suite is run against.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalTarget {
    /// Agent implementation id (`native`, `claude-code`, ...); `None` = auto.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl EvalTarget {
    /// Compact human-readable label, e.g. `native:anthropic/claude-sonnet-4`.
    pub fn label(&self) -> String {
        let mut label = self.agent.clone().unwrap_or_else(|| "auto".to_owned());
        if let Some(provider) = &self.provider {
            label.push(':');
            label.push_str(provider);
        }
        if let Some(model) = &self.model {
            label.push('/');
            label.push_str(model);
        }
        label
    }
}

/// Future returned by a [`ServiceFactory`].
pub type ServiceFuture = Pin<Box<dyn Future<Output = Result<EngineService, EvalError>> + Send>>;

/// Builds a fresh [`EngineService`] for one run, rooted at the run workspace.
/// Injected by the caller (the CLI wires its resolver; tests wire a mock).
pub type ServiceFactory = Arc<dyn Fn(EvalTarget, PathBuf) -> ServiceFuture + Send + Sync>;

/// Outcome of one task run against one target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub task_id: String,
    /// [`EvalTarget::label`] of the target this run used.
    pub target: String,
    /// 0-based repeat index.
    pub run_index: u32,
    pub passed: bool,
    /// Why the run failed (check output, timeout, session error); `None` on pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    pub metrics: RunMetrics,
    /// Raw JSONL dump of the session's persisted events, when captured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<PathBuf>,
}

/// Execute one task against one target: copy the fixture into a temp git
/// workspace, drive the prompt to completion under the task's timeout, run
/// the check, and dump the raw event log under `out_dir`.
pub async fn run_task(
    task: &TaskSpec,
    target: &EvalTarget,
    factory: &ServiceFactory,
    out_dir: &Path,
    run_index: u32,
) -> Result<RunResult, EvalError> {
    let started = Instant::now();
    let mut result = RunResult {
        task_id: task.id.clone(),
        target: target.label(),
        run_index,
        passed: false,
        failure: None,
        metrics: RunMetrics::default(),
        transcript_path: None,
    };

    let workspace = tempfile::tempdir()?;
    if let Some(fixture) = &task.fixture {
        copy_dir(fixture, workspace.path())?;
    }
    init_git(workspace.path()).await;

    let outcome = drive(
        task,
        target,
        factory,
        workspace.path(),
        &mut result,
        out_dir,
    )
    .await;
    match outcome {
        Ok(()) => {
            match evaluate_check(&task.check, workspace.path()).await {
                Ok(()) => result.passed = true,
                Err(reason) => result.failure = Some(reason),
            };
        }
        Err(reason) => result.failure = Some(reason),
    }
    result.metrics.wall_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    Ok(result)
}

/// Build the service, run the prompt under the timeout, fold metrics, and dump
/// the transcript. A returned `Err(String)` is a *run* failure (recorded on
/// the result), not an infrastructure error.
async fn drive(
    task: &TaskSpec,
    target: &EvalTarget,
    factory: &ServiceFactory,
    workspace: &Path,
    result: &mut RunResult,
    out_dir: &Path,
) -> Result<(), String> {
    let service = factory(target.clone(), workspace.to_path_buf())
        .await
        .map_err(|err| format!("service build failed: {err}"))?;
    let session = service
        .create_session(NewSessionParams {
            cwd: Some(workspace.to_path_buf()),
            permission_mode: Some(PermissionMode::BypassPermissions),
            ..NewSessionParams::default()
        })
        .await
        .map_err(|err| format!("create_session failed: {err}"))?;

    let turn = tokio::time::timeout(
        Duration::from_secs(task.timeout_secs),
        service.prompt(
            &session,
            PromptInput::text(&task.prompt),
            TurnOptions {
                permission_mode: Some(PermissionMode::BypassPermissions),
                ..TurnOptions::default()
            },
        ),
    )
    .await;

    // Dump the persisted event log regardless of how the turn ended.
    match service.replay(&session, 0).await {
        Ok(events) => {
            match dump_transcript(
                out_dir,
                &result.task_id,
                &result.target,
                result.run_index,
                &events,
            ) {
                Ok(path) => result.transcript_path = Some(path),
                Err(err) => tracing::warn!("failed to dump transcript: {err}"),
            }
        }
        Err(err) => tracing::warn!("failed to replay session events: {err}"),
    }

    match turn {
        Err(_elapsed) => {
            // Best effort: stop the in-flight turn before the workspace drops.
            let _ = service.cancel(&session).await;
            Err(format!("timed out after {}s", task.timeout_secs))
        }
        Ok(Err(err)) => Err(format!("turn failed: {err}")),
        Ok(Ok(summary)) => {
            result.metrics.fold_turn(&summary);
            Ok(())
        }
    }
}

/// Run the task's check in the workspace; `Err` carries the failure reason.
async fn evaluate_check(check: &CheckSpec, workspace: &Path) -> Result<(), String> {
    if let Some(cmd) = &check.cmd {
        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(workspace)
            .output()
            .await
            .map_err(|err| format!("check command failed to spawn: {err}"))?;
        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "check `{cmd}` exited with {}: {}{}",
                output.status,
                stdout.trim(),
                stderr.trim()
            ));
        }
    }
    for file in &check.expect_files {
        if !workspace.join(file).exists() {
            return Err(format!("expected file missing: {}", file.display()));
        }
    }
    for (file, needle) in &check.expect_contains {
        let text = std::fs::read_to_string(workspace.join(file))
            .map_err(|err| format!("expected file unreadable: {}: {err}", file.display()))?;
        if !text.contains(needle) {
            return Err(format!("{} does not contain `{needle}`", file.display()));
        }
    }
    Ok(())
}

/// Serialize the persisted events as JSONL under `out_dir`.
fn dump_transcript(
    out_dir: &Path,
    task_id: &str,
    target: &str,
    run_index: u32,
    events: &[agentloop_contracts::SessionEvent],
) -> Result<PathBuf, EvalError> {
    std::fs::create_dir_all(out_dir)?;
    let file_name = format!("{task_id}-{}-{run_index}.jsonl", sanitize(target));
    let path = out_dir.join(file_name);
    let mut buf = String::new();
    for event in events {
        buf.push_str(&serde_json::to_string(event)?);
        buf.push('\n');
    }
    std::fs::write(&path, buf)?;
    Ok(path)
}

fn sanitize(label: &str) -> String {
    label
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Recursively copy `src` into `dst` (which must exist), skipping `.git`.
fn copy_dir(src: &Path, dst: &Path) -> Result<(), EvalError> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        if entry.file_type()?.is_dir() {
            std::fs::create_dir_all(&to)?;
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Turn the workspace into a git repo with an initial commit so per-turn
/// snapshots have something to work with and diffs are easy to inspect.
/// Best-effort: a missing `git` degrades to a plain directory.
async fn init_git(workspace: &Path) {
    for args in [
        vec!["init", "-q"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.email=eval@localhost",
            "-c",
            "user.name=eval",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "eval fixture",
        ],
    ] {
        let status = tokio::process::Command::new("git")
            .args(&args)
            .current_dir(workspace)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;
        match status {
            Ok(status) if status.success() => {}
            Ok(status) => {
                tracing::warn!("git {:?} exited with {status} in eval workspace", args);
                return;
            }
            Err(err) => {
                tracing::warn!("git unavailable for eval workspace: {err}");
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_label_formats() {
        assert_eq!(EvalTarget::default().label(), "auto");
        let target = EvalTarget {
            agent: Some("native".to_owned()),
            provider: Some("anthropic".to_owned()),
            model: Some("claude-sonnet-4".to_owned()),
        };
        assert_eq!(target.label(), "native:anthropic/claude-sonnet-4");
    }

    #[tokio::test]
    async fn check_evaluation_covers_cmd_files_and_contains() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("hello.txt"), "hello world\n").expect("write");

        let check = CheckSpec {
            cmd: Some("grep -qx 'hello world' hello.txt".to_owned()),
            expect_files: vec![PathBuf::from("hello.txt")],
            expect_contains: [(PathBuf::from("hello.txt"), "hello".to_owned())].into(),
        };
        assert!(evaluate_check(&check, dir.path()).await.is_ok());

        let failing = CheckSpec {
            cmd: Some("false".to_owned()),
            ..CheckSpec::default()
        };
        assert!(evaluate_check(&failing, dir.path()).await.is_err());

        let missing = CheckSpec {
            expect_files: vec![PathBuf::from("nope.txt")],
            ..CheckSpec::default()
        };
        assert!(evaluate_check(&missing, dir.path()).await.is_err());
    }

    #[test]
    fn copy_dir_skips_git() {
        let src = tempfile::tempdir().expect("tempdir");
        let dst = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(src.path().join(".git")).expect("mkdir");
        std::fs::create_dir_all(src.path().join("sub")).expect("mkdir");
        std::fs::write(src.path().join("sub/a.txt"), "a").expect("write");
        copy_dir(src.path(), dst.path()).expect("copies");
        assert!(dst.path().join("sub/a.txt").exists());
        assert!(!dst.path().join(".git").exists());
    }
}
