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
        args.agent_cmd.as_deref(),
        args.provider.as_deref(),
        args.model.clone(),
        &args.fallback_models,
        &args.plugins,
        args.workdir.as_deref(),
    )
    .await?;
    for line in &resolution.trace {
        tracing::info!(target: "resolution", "{line}");
    }

    let mut request = OneTurnRequest::new(args.prompt, args.workdir);
    request.fallback_models = args
        .fallback_models
        .into_iter()
        .map(agentloop_contracts::ModelRef)
        .collect();
    let _summary = serve_one_turn(resolution.service, request, tokio::io::stdout())
        .await
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    Ok(())
}
