//! Local backend: `/bin/sh -lc` in the session cwd — the historical default.

use async_trait::async_trait;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use agentloop_core::{ExecError, ExecOutcome, ExecSpec, Executor, ExecutorHealth, NetworkPolicy};

use crate::run::run_command_with_sink;

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
        let mut command = Command::new("/bin/sh");
        command.arg("-lc").arg(&spec.command).current_dir(&spec.cwd);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn spec(command: &str) -> ExecSpec {
        ExecSpec {
            command: command.to_owned(),
            cwd: PathBuf::from("."),
            env: Vec::new(),
            timeout_ms: 5_000,
            network: NetworkPolicy::Allowed,
            chunk_sink: None,
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
}
