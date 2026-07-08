//! One-turn headless run path.

use anyhow::bail;

use agentloop_transport_stdio::{OneTurnRequest, serve_one_turn};

use crate::cli::{OutputFormat, RunArgs};
use crate::resolve::resolve_service;

pub(crate) async fn run(args: RunArgs) -> anyhow::Result<()> {
    if args.output_format != OutputFormat::Ndjson {
        bail!("only ndjson output is implemented");
    }

    let resolution = resolve_service(
        args.agent.as_deref(),
        args.provider.as_deref(),
        args.model.clone(),
        &args.workdir,
    )
    .await?;
    for line in &resolution.trace {
        tracing::info!(target: "resolution", "{line}");
    }

    let request = OneTurnRequest::new(args.prompt, args.workdir);
    let _summary = serve_one_turn(resolution.service, request, tokio::io::stdout())
        .await
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    Ok(())
}
