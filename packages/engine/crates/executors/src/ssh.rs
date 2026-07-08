//! SSH backend: run commands on a remote host, mapping the session cwd onto a
//! configured remote root and syncing files with `rsync`.

use std::path::Path;

use async_trait::async_trait;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use agentloop_core::{ExecError, ExecOutcome, ExecSpec, Executor, ExecutorHealth, NetworkPolicy};

use crate::run::{probe_binary, run_command};

/// Timeout for the file-sync phases, independent of per-command timeouts.
const SYNC_TIMEOUT_MS: u64 = 300_000;

/// Runs commands on `host` over `ssh`, inside `remote_root`. The session's
/// working tree is pushed with `rsync` before commands run ([`Executor::sync_in`])
/// and pulled back after ([`Executor::sync_out`]); callers decide the cadence.
/// Cannot honor [`NetworkPolicy::Denied`].
#[derive(Debug, Clone)]
pub struct SshExecutor {
    /// `user@host` or an `ssh_config` alias.
    host: String,
    /// Absolute directory on the remote host that mirrors the session cwd.
    remote_root: String,
}

impl SshExecutor {
    pub fn new(host: impl Into<String>, remote_root: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            remote_root: remote_root.into(),
        }
    }

    fn rsync_spec(&self) -> String {
        // Trailing slashes make rsync mirror directory *contents*.
        format!("{}:{}/", self.host, self.remote_root.trim_end_matches('/'))
    }
}

#[async_trait]
impl Executor for SshExecutor {
    fn id(&self) -> &'static str {
        "ssh"
    }

    async fn probe(&self) -> ExecutorHealth {
        if let Err(detail) = probe_binary("rsync", &["--version"]).await {
            return ExecutorHealth {
                available: false,
                detail,
            };
        }
        let mut command = Command::new("ssh");
        command
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("ConnectTimeout=5")
            .arg(&self.host)
            .arg("true");
        match run_command(command, 10_000, CancellationToken::new(), "ssh probe").await {
            Ok(outcome) if outcome.exit_code == Some(0) => ExecutorHealth {
                available: true,
                detail: format!("{} reachable, root {}", self.host, self.remote_root),
            },
            Ok(outcome) => ExecutorHealth {
                available: false,
                detail: format!(
                    "ssh {} failed: {}",
                    self.host,
                    String::from_utf8_lossy(&outcome.stderr).trim()
                ),
            },
            Err(err) => ExecutorHealth {
                available: false,
                detail: err.to_string(),
            },
        }
    }

    async fn exec(
        &self,
        spec: ExecSpec,
        cancel: CancellationToken,
    ) -> Result<ExecOutcome, ExecError> {
        if spec.network == NetworkPolicy::Denied {
            return Err(ExecError::Unsupported(
                "the ssh backend cannot isolate the network on the remote host".to_owned(),
            ));
        }
        // cd into the mapped root and run under a login shell, mirroring the
        // local `/bin/sh -lc` semantics.
        let mut remote = String::new();
        for (key, value) in &spec.env {
            remote.push_str(&format!("export {key}={}; ", shell_quote(value)));
        }
        remote.push_str(&format!(
            "cd {} && sh -lc {}",
            shell_quote(&self.remote_root),
            shell_quote(&spec.command)
        ));
        let mut command = Command::new("ssh");
        command
            .arg("-o")
            .arg("BatchMode=yes")
            .arg(&self.host)
            .arg(remote);
        run_command(command, spec.timeout_ms, cancel, "ssh command").await
    }

    async fn sync_in(&self, cwd: &Path) -> Result<(), ExecError> {
        let mut command = Command::new("rsync");
        command
            .arg("-a")
            .arg("--delete")
            .arg("--exclude=.git")
            .arg(format!("{}/", cwd.display()))
            .arg(self.rsync_spec());
        let outcome = run_command(
            command,
            SYNC_TIMEOUT_MS,
            CancellationToken::new(),
            "rsync push",
        )
        .await?;
        if outcome.exit_code != Some(0) {
            return Err(ExecError::Failed(format!(
                "rsync push failed: {}",
                String::from_utf8_lossy(&outcome.stderr).trim()
            )));
        }
        Ok(())
    }

    async fn sync_out(&self, cwd: &Path) -> Result<(), ExecError> {
        let mut command = Command::new("rsync");
        command
            .arg("-a")
            .arg("--exclude=.git")
            .arg(self.rsync_spec())
            .arg(format!("{}/", cwd.display()));
        let outcome = run_command(
            command,
            SYNC_TIMEOUT_MS,
            CancellationToken::new(),
            "rsync pull",
        )
        .await?;
        if outcome.exit_code != Some(0) {
            return Err(ExecError::Failed(format!(
                "rsync pull failed: {}",
                String::from_utf8_lossy(&outcome.stderr).trim()
            )));
        }
        Ok(())
    }
}

/// Minimal POSIX single-quote escaping.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::shell_quote;

    #[test]
    fn quotes_single_quotes() {
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    }
}
