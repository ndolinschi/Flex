//! One-turn headless run path.

use anyhow::bail;

use agentloop_engine::{EngineOptions, EngineService};
use agentloop_transport_stdio::{OneTurnRequest, serve_one_turn};

use crate::cli::{OutputFormat, RunArgs};

pub(crate) async fn run(args: RunArgs) -> anyhow::Result<()> {
    if args.output_format != OutputFormat::Ndjson {
        bail!("only ndjson output is implemented");
    }

    let service = EngineService::native(EngineOptions {
        provider: args.provider.clone(),
        model: args.model.clone(),
        cwd: args.workdir.clone(),
        date: "2026-07-02".to_owned(),
    })
    .map_err(|err| anyhow::anyhow!("{}", err.to_engine_error().message))?;

    // The headless runner has no permission-response input yet. The transport
    // defaults to plan mode so read-only tools can run while mutations deny.
    let request = OneTurnRequest::new(args.prompt, args.workdir);
    let _summary = serve_one_turn(service, request, tokio::io::stdout())
        .await
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    Ok(())
}
