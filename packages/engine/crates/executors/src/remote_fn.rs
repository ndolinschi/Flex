use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use agentloop_core::{ExecError, ExecOutcome, ExecSpec, Executor, ExecutorHealth};

#[derive(Debug, Default, Clone, Copy)]
pub struct RemoteFnExecutor;

#[async_trait]
impl Executor for RemoteFnExecutor {
    fn id(&self) -> &'static str {
        "remote-fn"
    }

    async fn probe(&self) -> ExecutorHealth {
        ExecutorHealth {
            available: false,
            detail: "serverless backend not implemented yet".to_owned(),
        }
    }

    async fn exec(
        &self,
        _spec: ExecSpec,
        _cancel: CancellationToken,
    ) -> Result<ExecOutcome, ExecError> {
        Err(ExecError::Unavailable(
            "the serverless backend is not implemented yet; use local, docker, ssh, or \
             container-image"
                .to_owned(),
        ))
    }
}
