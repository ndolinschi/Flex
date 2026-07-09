//! `serve` subcommand: headless HTTP/SSE server over the resolved engine.

#![allow(clippy::print_stderr)]

use std::sync::Arc;

use agentloop_contracts::branding;
use agentloop_transport_http::{AuthToken, HttpServeOptions, serve_http};

use crate::cli::ServeArgs;
use crate::resolve::resolve_service;

pub(crate) async fn serve(args: ServeArgs) -> anyhow::Result<()> {
    let resolution = resolve_service(
        args.agent.as_deref(),
        args.agent_cmd.as_deref(),
        args.provider.as_deref(),
        args.model.clone(),
        args.workdir.as_deref(),
    )
    .await?;
    for line in &resolution.trace {
        tracing::info!(target: "resolution", "{line}");
    }

    let (token, token_was_explicit) = match args
        .token
        .or_else(|| std::env::var(format!("{}_SERVE_TOKEN", branding::ENV_PREFIX)).ok())
    {
        Some(token) => (AuthToken::new(token), true),
        None => {
            let token = AuthToken::generate();
            eprintln!("auth token (save this): {}", token.as_str());
            (token, false)
        }
    };

    eprintln!("listening on http://{}", args.bind);
    serve_http(
        Arc::new(resolution.service),
        HttpServeOptions {
            bind: args.bind,
            token,
            token_was_explicit,
        },
    )
    .await
    .map_err(|err| anyhow::anyhow!("{err}"))
}
