//! `serve` subcommand: headless HTTP/SSE server over the resolved engine.

#![allow(clippy::print_stderr)]

use std::sync::Arc;

use agentloop_contracts::branding;
use agentloop_sdk::routines::{FileRoutineStore, RoutineRunner, routine_webhook_router};
use agentloop_transport_http::{AuthToken, HttpServeOptions, serve_http_with_extra};
use axum::Router;
use tokio_util::sync::CancellationToken;

use crate::cli::ServeArgs;
use crate::resolve::resolve_service;

pub(crate) async fn serve(args: ServeArgs) -> anyhow::Result<()> {
    let resolution = resolve_service(
        args.agent.as_deref(),
        args.agent_cmd.as_deref(),
        args.provider.as_deref(),
        args.model.clone(),
        &args.fallback_models,
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

    let engine = Arc::new(resolution.service);
    let mut extra = Router::new();
    if args.enable_routines {
        let Some(store) = FileRoutineStore::with_default_dir() else {
            return Err(anyhow::anyhow!(
                "--enable-routines needs a resolvable home directory for the routines store"
            ));
        };
        let runner = Arc::new(RoutineRunner::new(engine.clone(), Arc::new(store)));
        tokio::spawn(runner.clone().spawn_cron_loop(CancellationToken::new()));
        extra = extra.merge(routine_webhook_router(runner, token.clone()));
        eprintln!("routines enabled: cron polling started, POST /routines/{{id}}/trigger mounted");
    }

    eprintln!("listening on http://{}", args.bind);
    serve_http_with_extra(
        engine,
        HttpServeOptions {
            bind: args.bind,
            token,
            token_was_explicit,
        },
        extra,
    )
    .await
    .map_err(|err| anyhow::anyhow!("{err}"))
}
