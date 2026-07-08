//! Container-image backend for HPC-style runtimes (`apptainer`, with
//! `singularity` as the legacy fallback binary).

use async_trait::async_trait;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use agentloop_core::{ExecError, ExecOutcome, ExecSpec, Executor, ExecutorHealth, NetworkPolicy};

use crate::run::{probe_binary, run_command};

/// Candidate binaries, newest first.
const BINARIES: [&str; 2] = ["apptainer", "singularity"];

/// Runs commands inside an Apptainer/Singularity image. The session cwd is
/// visible via the runtime's default home/cwd bind. Honors
/// [`NetworkPolicy::Denied`] with `--net --network none`.
#[derive(Debug, Clone)]
pub struct ContainerImageExecutor {
    /// Path or URI of the image (`.sif`, `docker://…`).
    image: String,
}

impl ContainerImageExecutor {
    pub fn new(image: impl Into<String>) -> Self {
        Self {
            image: image.into(),
        }
    }

    async fn binary(&self) -> Result<&'static str, String> {
        let mut last_err = String::from("no candidate binary found");
        for binary in BINARIES {
            match probe_binary(binary, &["--version"]).await {
                Ok(_) => return Ok(binary),
                Err(err) => last_err = err,
            }
        }
        Err(last_err)
    }
}

#[async_trait]
impl Executor for ContainerImageExecutor {
    fn id(&self) -> &'static str {
        "container-image"
    }

    async fn probe(&self) -> ExecutorHealth {
        match self.binary().await {
            Ok(binary) => ExecutorHealth {
                available: true,
                detail: format!("{binary} available, image {}", self.image),
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
        let binary = self.binary().await.map_err(ExecError::Unavailable)?;
        let mut command = Command::new(binary);
        command
            .arg("exec")
            .arg("--pwd")
            .arg("/work")
            .arg("--bind")
            .arg(format!("{}:/work", spec.cwd.display()));
        if spec.network == NetworkPolicy::Denied {
            command.arg("--net").arg("--network").arg("none");
        }
        for (key, value) in &spec.env {
            command.arg("--env").arg(format!("{key}={value}"));
        }
        command
            .arg(&self.image)
            .arg("sh")
            .arg("-lc")
            .arg(&spec.command);
        run_command(command, spec.timeout_ms, cancel, "container command").await
    }
}
