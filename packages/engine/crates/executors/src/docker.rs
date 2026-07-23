use async_trait::async_trait;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use agentloop_core::{ExecError, ExecOutcome, ExecSpec, Executor, ExecutorHealth, NetworkPolicy};

use crate::run::{probe_binary, run_command};

#[derive(Debug, Clone)]
pub struct DockerExecutor {
    image: String,
    binary: String,
}

impl DockerExecutor {
    pub fn new(image: impl Into<String>) -> Self {
        Self {
            image: image.into(),
            binary: "docker".to_owned(),
        }
    }

    pub fn with_binary(mut self, binary: impl Into<String>) -> Self {
        self.binary = binary.into();
        self
    }
}

#[async_trait]
impl Executor for DockerExecutor {
    fn id(&self) -> &'static str {
        "docker"
    }

    async fn probe(&self) -> ExecutorHealth {
        match probe_binary(
            &self.binary,
            &["version", "--format", "{{.Server.Version}}"],
        )
        .await
        {
            Ok(version) => ExecutorHealth {
                available: true,
                detail: format!("{} server {version}, image {}", self.binary, self.image),
            },
            Err(detail) => ExecutorHealth {
                available: false,
                detail,
            },
        }
    }

    async fn exec(
        &self,
        spec: ExecSpec,
        cancel: CancellationToken,
    ) -> Result<ExecOutcome, ExecError> {
        let cwd = spec.cwd.canonicalize().map_err(|err| {
            ExecError::Failed(format!(
                "cannot resolve cwd `{}` for bind mount: {err}",
                spec.cwd.display()
            ))
        })?;
        let mut command = Command::new(&self.binary);
        command
            .arg("run")
            .arg("--rm")
            .arg("--init")
            .arg("-v")
            .arg(format!("{}:/work", cwd.display()))
            .arg("-w")
            .arg("/work");
        if spec.network == NetworkPolicy::Denied {
            command.arg("--network").arg("none");
        }
        for (key, value) in &spec.env {
            command.arg("-e").arg(format!("{key}={value}"));
        }
        command
            .arg(&self.image)
            .arg("sh")
            .arg("-lc")
            .arg(&spec.command);
        run_command(command, spec.timeout_ms, cancel, "docker command").await
    }
}
