use std::io::ErrorKind;
use std::process::Stdio;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use agentloop_delegator_common::{
    DelegatorExitStatus, DelegatorHostError, DelegatorProbeStatus, DelegatorProcessSpec,
    DelegatorRunOutput, DelegatorRunRequest, ProcessHost,
};

#[derive(Debug, Default)]
pub struct TokioCommandHost;

impl TokioCommandHost {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProcessHost for TokioCommandHost {
    async fn probe(
        &self,
        spec: &DelegatorProcessSpec,
        cancel: CancellationToken,
    ) -> Result<DelegatorProbeStatus, DelegatorHostError> {
        let request = DelegatorRunRequest::new(spec.clone());
        match self.run(request, cancel).await {
            Ok(output) if output.status.success => {
                let version = output
                    .stdout_lines
                    .first()
                    .map(|line| line.trim().to_owned())
                    .filter(|line| !line.is_empty())
                    .or_else(|| {
                        let stderr = output.stderr.trim();
                        (!stderr.is_empty()).then(|| stderr.to_owned())
                    });
                Ok(DelegatorProbeStatus::Installed { version })
            }
            Ok(output) => Ok(DelegatorProbeStatus::NotInstalled {
                hint: format!(
                    "`{}` ran but did not pass its probe (exit {:?}): {}",
                    spec.program,
                    output.status.code,
                    output.stderr.trim()
                ),
            }),
            Err(DelegatorHostError::NotInstalled { hint, .. }) => {
                Ok(DelegatorProbeStatus::NotInstalled { hint })
            }
            Err(err) => Err(err),
        }
    }

    async fn run(
        &self,
        request: DelegatorRunRequest,
        cancel: CancellationToken,
    ) -> Result<DelegatorRunOutput, DelegatorHostError> {
        let mut command = Command::new(&request.spec.program);
        command.args(&request.spec.args);
        if let Some(cwd) = &request.spec.cwd {
            command.current_dir(cwd);
        }
        command.envs(&request.spec.env);
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.stdin(if request.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        command.kill_on_drop(true);

        let mut child = command.spawn().map_err(|err| match err.kind() {
            ErrorKind::NotFound => DelegatorHostError::NotInstalled {
                program: request.spec.program.clone(),
                hint: format!(
                    "install Claude Code or set the delegator program path; attempted `{}`",
                    request.spec.program
                ),
            },
            _ => DelegatorHostError::Io(err.to_string()),
        })?;

        if let Some(stdin) = request.stdin {
            let Some(mut child_stdin) = child.stdin.take() else {
                return Err(DelegatorHostError::Io(
                    "delegator stdin was requested but not available".to_owned(),
                ));
            };
            child_stdin
                .write_all(stdin.as_bytes())
                .await
                .map_err(|err| DelegatorHostError::Io(err.to_string()))?;
            child_stdin
                .shutdown()
                .await
                .map_err(|err| DelegatorHostError::Io(err.to_string()))?;
        }

        let output = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(DelegatorHostError::Cancelled),
            output = child.wait_with_output() => output
                .map_err(|err| DelegatorHostError::Io(err.to_string()))?,
        };

        let stdout = String::from_utf8(output.stdout)
            .map_err(|err| DelegatorHostError::Utf8(err.to_string()))?;
        let stderr = String::from_utf8(output.stderr)
            .map_err(|err| DelegatorHostError::Utf8(err.to_string()))?;
        let status = DelegatorExitStatus {
            code: output.status.code(),
            success: output.status.success(),
        };

        Ok(DelegatorRunOutput {
            stdout_lines: stdout.lines().map(str::to_owned).collect(),
            stderr,
            status,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ClaudeCodeConfig;

    #[tokio::test]
    #[ignore = "requires Claude Code CLI on PATH"]
    async fn live_probe_claude_code_cli() {
        let host = TokioCommandHost::new();
        let config = ClaudeCodeConfig::default();
        let result = host
            .probe(&config.probe_spec(), CancellationToken::new())
            .await;

        match result {
            Ok(DelegatorProbeStatus::Installed { .. })
            | Ok(DelegatorProbeStatus::NotInstalled { .. }) => {}
            Err(err) => panic!("probe should return an actionable status: {err}"),
        }
    }
}
