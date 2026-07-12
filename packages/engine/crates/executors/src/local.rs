//! Local backend: runs the session's shell directly in the session cwd — the
//! historical default. Unix uses `/bin/sh -c`; Windows uses `cmd /C` (see
//! [`shell_command`] for why).

use async_trait::async_trait;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use agentloop_core::{
    BackgroundSpawn, ExecError, ExecOrDemoted, ExecOutcome, ExecSpec, Executor, ExecutorHealth,
    NetworkPolicy,
};

use crate::run::{run_command_demotable, run_command_with_sink, spawn_background};

/// Build a [`Command`] that runs `script` as a single-line shell command.
///
/// Unix: `/bin/sh -lc <script>` (login shell, matching the historical
/// behavior so `PATH`/profile-sourced env stays intact). Windows: `cmd /C
/// <script>` rather than PowerShell — `cmd /C` takes the trailing argument as
/// one already-quoted command line with predictable, POSIX-`sh`-like
/// quoting for the single-line scripts this executor runs, whereas
/// PowerShell's parameter binding and quoting rules differ enough (e.g.
/// `-Command` re-tokenizing, different escaping for `"`/`$`) to risk subtly
/// mis-parsing scripts written against `sh` semantics.
fn shell_command(script: &str) -> Command {
    #[cfg(windows)]
    {
        let mut command = Command::new("cmd");
        command.arg("/C").arg(script);
        command
    }
    #[cfg(not(windows))]
    {
        let mut command = Command::new("/bin/sh");
        command.arg("-lc").arg(script);
        // Make the shell the leader of a new process group (pgid = its own
        // pid) instead of inheriting ours. Killing just the `/bin/sh` pid on
        // cancel/timeout/kill leaves any children it spawned (a pipeline
        // stage, a backgrounded `node`/`python` server via `&`, a `make`
        // sub-process) running as orphans reparented to init — this crate's
        // kill paths (`run_command*`'s cancel/timeout branches,
        // `LocalBackgroundProcess::kill`) all signal the *group* via
        // `crate::process_group::kill_group` instead, which requires every
        // spawn to actually be its own group leader.
        crate::process_group::configure(&mut command);
        command
    }
}

/// Runs commands directly on the host with no isolation. Cannot honor
/// [`NetworkPolicy::Denied`].
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalExecutor;

#[async_trait]
impl Executor for LocalExecutor {
    fn id(&self) -> &'static str {
        "local"
    }

    async fn probe(&self) -> ExecutorHealth {
        ExecutorHealth {
            available: true,
            #[cfg(windows)]
            detail: "cmd /C on the host".to_owned(),
            #[cfg(not(windows))]
            detail: "/bin/sh on the host".to_owned(),
        }
    }

    async fn exec(
        &self,
        spec: ExecSpec,
        cancel: CancellationToken,
    ) -> Result<ExecOutcome, ExecError> {
        if spec.network == NetworkPolicy::Denied {
            return Err(ExecError::Unsupported(
                "the local backend cannot isolate the network; use a container backend".to_owned(),
            ));
        }
        let mut command = shell_command(&spec.command);
        command.current_dir(&spec.cwd);
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        run_command_with_sink(
            command,
            spec.timeout_ms,
            cancel,
            "local command",
            spec.chunk_sink,
        )
        .await
    }

    async fn exec_demotable(
        &self,
        spec: ExecSpec,
        cancel: CancellationToken,
    ) -> Result<ExecOrDemoted, ExecError> {
        if spec.network == NetworkPolicy::Denied {
            return Err(ExecError::Unsupported(
                "the local backend cannot isolate the network; use a container backend".to_owned(),
            ));
        }
        let mut command = shell_command(&spec.command);
        command.current_dir(&spec.cwd);
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        run_command_demotable(
            command,
            spec.timeout_ms,
            cancel,
            spec.demote,
            "local command",
            spec.chunk_sink,
        )
        .await
    }

    async fn exec_background(&self, spec: ExecSpec) -> Result<BackgroundSpawn, ExecError> {
        if spec.network == NetworkPolicy::Denied {
            return Err(ExecError::Unsupported(
                "the local backend cannot isolate the network; use a container backend".to_owned(),
            ));
        }
        let mut command = shell_command(&spec.command);
        command.current_dir(&spec.cwd);
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        spawn_background(command, "local background command", spec.chunk_sink).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    fn spec(command: &str) -> ExecSpec {
        ExecSpec {
            command: command.to_owned(),
            cwd: PathBuf::from("."),
            env: Vec::new(),
            timeout_ms: 5_000,
            network: NetworkPolicy::Allowed,
            chunk_sink: None,
            demote: None,
        }
    }

    #[tokio::test]
    async fn runs_a_command_and_captures_output() {
        let outcome = LocalExecutor
            .exec(spec("printf hello"), CancellationToken::new())
            .await
            .expect("exec ok");
        assert_eq!(outcome.exit_code, Some(0));
        assert_eq!(outcome.stdout, b"hello");
    }

    #[tokio::test]
    async fn propagates_exit_codes() {
        let outcome = LocalExecutor
            .exec(spec("exit 3"), CancellationToken::new())
            .await
            .expect("exec ok");
        assert_eq!(outcome.exit_code, Some(3));
    }

    #[tokio::test]
    async fn times_out() {
        let mut s = spec("sleep 5");
        s.timeout_ms = 100;
        let err = LocalExecutor
            .exec(s, CancellationToken::new())
            .await
            .unwrap_err();
        assert!(matches!(err, ExecError::Timeout(100)));
    }

    #[tokio::test]
    async fn honors_cancellation() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let err = LocalExecutor
            .exec(spec("sleep 5"), cancel)
            .await
            .unwrap_err();
        assert!(matches!(err, ExecError::Cancelled));
    }

    #[tokio::test]
    async fn rejects_network_denial() {
        let mut s = spec("true");
        s.network = NetworkPolicy::Denied;
        let err = LocalExecutor
            .exec(s, CancellationToken::new())
            .await
            .unwrap_err();
        assert!(matches!(err, ExecError::Unsupported(_)));
    }

    #[tokio::test]
    async fn streams_chunks_while_still_returning_full_output() {
        use std::sync::{Arc, Mutex};

        use agentloop_core::ExecStream;

        let chunks: Arc<Mutex<Vec<(ExecStream, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let collector = chunks.clone();
        let sink: agentloop_core::ChunkSink = Arc::new(move |stream, text| {
            collector
                .lock()
                .expect("lock")
                .push((stream, text.to_owned()));
        });

        let mut s = spec("printf 'a\\nb\\n'; printf 'oops\\n' 1>&2");
        s.chunk_sink = Some(sink);
        let outcome = LocalExecutor
            .exec(s, CancellationToken::new())
            .await
            .expect("exec ok");

        // Final result still carries the complete, unstreamed-view output.
        assert_eq!(outcome.exit_code, Some(0));
        assert_eq!(outcome.stdout, b"a\nb\n");
        assert_eq!(outcome.stderr, b"oops\n");

        // The sink actually received chunks for both streams.
        let seen = chunks.lock().expect("lock");
        assert!(
            seen.iter()
                .any(|(s, text)| *s == ExecStream::Stdout && text.contains("a")),
            "expected a stdout chunk, got {seen:?}"
        );
        assert!(
            seen.iter()
                .any(|(s, text)| *s == ExecStream::Stderr && text.contains("oops")),
            "expected a stderr chunk, got {seen:?}"
        );
    }

    #[tokio::test]
    async fn background_returns_early_while_process_still_runs() {
        // The process sleeps well past the initial-output window; the call
        // must still return (proving it doesn't wait for exit) with the
        // banner it printed before sleeping, and `status()` must report it
        // as still running.
        let s = spec("echo ready; sleep 5");
        let spawn = LocalExecutor
            .exec_background(s)
            .await
            .expect("background spawn ok");
        assert!(spawn.initial_output.contains("ready"));
        let status = spawn.handle.status();
        assert!(status.running, "process should still be running");
        assert!(status.pid.is_some());

        // Clean up so the test doesn't leave a sleeping child around.
        spawn.handle.kill().await.expect("kill ok");
    }

    #[tokio::test]
    async fn background_kill_stops_the_process() {
        let s = spec("sleep 30");
        let spawn = LocalExecutor
            .exec_background(s)
            .await
            .expect("background spawn ok");
        assert!(spawn.handle.status().running);

        spawn.handle.kill().await.expect("kill ok");

        // Give the wait task a moment to observe the cancellation and flip
        // the shared state; poll briefly rather than sleeping a fixed guess.
        for _ in 0..50 {
            if !spawn.handle.status().running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(
            !spawn.handle.status().running,
            "process should be reported stopped after kill"
        );
    }

    #[tokio::test]
    async fn background_reports_exit_code_after_natural_completion() {
        let s = spec("exit 7");
        let spawn = LocalExecutor
            .exec_background(s)
            .await
            .expect("background spawn ok");

        for _ in 0..50 {
            if !spawn.handle.status().running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let status = spawn.handle.status();
        assert!(!status.running);
        assert_eq!(status.exit_code, Some(7));
    }

    #[tokio::test]
    async fn foreground_exec_is_unaffected_by_background_support() {
        // Non-background behavior stays byte-identical: same call, same
        // path, regardless of `exec_background` existing on the trait.
        let outcome = LocalExecutor
            .exec(spec("printf hello"), CancellationToken::new())
            .await
            .expect("exec ok");
        assert_eq!(outcome.exit_code, Some(0));
        assert_eq!(outcome.stdout, b"hello");
    }

    #[tokio::test]
    async fn exec_demotable_without_a_demote_token_behaves_like_exec() {
        use agentloop_core::ExecOrDemoted;

        let mut s = spec("printf hello");
        s.demote = None;
        let result = LocalExecutor
            .exec_demotable(s, CancellationToken::new())
            .await
            .expect("exec_demotable ok");
        match result {
            ExecOrDemoted::Completed(outcome) => {
                assert_eq!(outcome.exit_code, Some(0));
                assert_eq!(outcome.stdout, b"hello");
            }
            ExecOrDemoted::Demoted { .. } => panic!("must not demote without a token"),
        }
    }

    #[tokio::test]
    async fn exec_demotable_hands_off_on_demote_and_process_keeps_running() {
        use agentloop_core::ExecOrDemoted;

        let demote = CancellationToken::new();
        let mut s = spec("echo ready; sleep 5");
        s.demote = Some(demote.clone());

        let call = tokio::spawn(async move {
            LocalExecutor
                .exec_demotable(s, CancellationToken::new())
                .await
        });

        // Give the process a moment to print its banner, then demote.
        tokio::time::sleep(Duration::from_millis(200)).await;
        demote.cancel();

        let result = call
            .await
            .expect("task join ok")
            .expect("exec_demotable ok");
        match result {
            ExecOrDemoted::Demoted { accumulated, entry } => {
                assert!(accumulated.contains("ready"));
                let status = entry.handle.status();
                assert!(status.running, "handed-off process should still be running");
                assert!(status.pid.is_some());
                entry.handle.kill().await.expect("kill ok");
            }
            ExecOrDemoted::Completed(_) => panic!("expected a demote handoff"),
        }
    }

    #[tokio::test]
    async fn exec_demotable_completes_normally_when_demote_never_fires() {
        use agentloop_core::ExecOrDemoted;

        let mut s = spec("printf out; printf err 1>&2");
        s.demote = Some(CancellationToken::new());
        let result = LocalExecutor
            .exec_demotable(s, CancellationToken::new())
            .await
            .expect("exec_demotable ok");
        match result {
            ExecOrDemoted::Completed(outcome) => {
                assert_eq!(outcome.exit_code, Some(0));
                assert_eq!(outcome.stdout, b"out");
                assert_eq!(outcome.stderr, b"err");
            }
            ExecOrDemoted::Demoted { .. } => panic!("must not demote when the token never fires"),
        }
    }
}
