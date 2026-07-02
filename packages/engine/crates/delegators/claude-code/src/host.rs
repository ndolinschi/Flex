//! The real process host now lives in `agentloop-delegator-common`;
//! re-exported here for backwards compatibility. The live probe test
//! stays with the crate that knows the CLI.

pub use agentloop_delegator_common::TokioCommandHost;

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_delegator_common::{DelegatorProbeStatus, ProcessHost};
    use tokio_util::sync::CancellationToken;

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
