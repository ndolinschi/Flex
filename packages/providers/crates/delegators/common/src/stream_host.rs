//! Interactive (duplex) process hosting for protocol-speaking delegators.
//!
//! The one-shot [`crate::ProcessHost`] collects stdout after exit — enough for
//! CLIs that print a transcript and quit, but not for bidirectional protocols
//! like ACP (JSON-RPC over the child's stdio). [`StreamHost`] spawns a
//! long-lived child and exposes line channels in both directions.

use std::io::ErrorKind;
use std::process::Stdio;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{DelegatorHostError, DelegatorProcessSpec};

/// A live duplex child process speaking newline-delimited frames.
///
/// Dropping the handle (or cancelling the token passed to `spawn`) kills the
/// child. `send_line` appends the newline itself.
pub struct DuplexProcess {
    to_child: mpsc::Sender<String>,
    from_child: tokio::sync::Mutex<mpsc::Receiver<String>>,
}

impl DuplexProcess {
    /// Build a duplex handle from raw channel ends — the constructor real
    /// hosts and scripted test hosts share.
    pub fn from_channels(
        to_child: mpsc::Sender<String>,
        from_child: mpsc::Receiver<String>,
    ) -> Self {
        Self {
            to_child,
            from_child: tokio::sync::Mutex::new(from_child),
        }
    }

    /// Write one frame to the child's stdin (newline appended by the host).
    pub async fn send_line(&self, line: String) -> Result<(), DelegatorHostError> {
        self.to_child
            .send(line)
            .await
            .map_err(|_| DelegatorHostError::Io("delegator stdin closed".to_owned()))
    }

    /// Next stdout frame, or `None` once the child's stdout is closed.
    pub async fn next_line(&self) -> Option<String> {
        self.from_child.lock().await.recv().await
    }
}

/// Spawn long-lived duplex children. Implemented by [`TokioStreamHost`] in
/// production and scripted fakes in tests.
#[async_trait]
pub trait StreamHost: Send + Sync {
    async fn spawn(
        &self,
        spec: &DelegatorProcessSpec,
        cancel: CancellationToken,
    ) -> Result<DuplexProcess, DelegatorHostError>;
}

/// Real duplex host over `tokio::process`.
#[derive(Debug, Default)]
pub struct TokioStreamHost;

impl TokioStreamHost {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl StreamHost for TokioStreamHost {
    async fn spawn(
        &self,
        spec: &DelegatorProcessSpec,
        cancel: CancellationToken,
    ) -> Result<DuplexProcess, DelegatorHostError> {
        let mut command = Command::new(&spec.program);
        command.args(&spec.args);
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        command.envs(&spec.env);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::null());
        command.kill_on_drop(true);
        // GUI parents (desktop) must not flash a conhost for connector CLIs.
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command.spawn().map_err(|err| match err.kind() {
            ErrorKind::NotFound => DelegatorHostError::NotInstalled {
                program: spec.program.clone(),
                hint: format!(
                    "install the agent program or set the delegator program path; attempted `{}`",
                    spec.program
                ),
            },
            _ => DelegatorHostError::Io(err.to_string()),
        })?;

        let mut stdin = child.stdin.take().ok_or_else(|| {
            DelegatorHostError::Io("delegator stdin was not available".to_owned())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            DelegatorHostError::Io("delegator stdout was not available".to_owned())
        })?;

        let (to_child, mut to_child_rx) = mpsc::channel::<String>(64);
        let (from_child_tx, from_child) = mpsc::channel::<String>(256);

        // Writer: frames → child stdin. Ends when the sender side is dropped.
        tokio::spawn(async move {
            while let Some(line) = to_child_rx.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err()
                    || stdin.write_all(b"\n").await.is_err()
                    || stdin.flush().await.is_err()
                {
                    break;
                }
            }
        });

        // Reader: child stdout lines → channel; owns the child so that
        // cancellation (or the reader ending) reaps the process.
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            loop {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => break,
                    line = lines.next_line() => match line {
                        Ok(Some(line)) => {
                            if from_child_tx.send(line).await.is_err() {
                                break;
                            }
                        }
                        _ => break,
                    },
                }
            }
            let _ = child.kill().await;
        });

        Ok(DuplexProcess::from_channels(to_child, from_child))
    }
}
