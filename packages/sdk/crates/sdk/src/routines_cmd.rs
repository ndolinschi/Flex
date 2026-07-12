//! `routines` subcommand and the `flex serve --enable-routines` webhook
//! route. Kept separate from `routines.rs` (the runner/store logic, which
//! doesn't know about the CLI or HTTP) — this module is the composition of
//! that logic into `flex`'s actual entry points.

use std::sync::Arc;

use anyhow::{Context, bail};

use agentloop_sdk::routines::{FileRoutineStore, RoutineRunner};

pub(crate) fn usage() -> &'static str {
    "usage:\n  flex routines list\n  flex routines run <id>\n  flex routines remove <id>\n\n\
     Adding a routine (there is no `routines add` subcommand — a RoutineSpec is\n\
     structured data: goal, session seed, and trigger) means writing a TOML file\n\
     directly under ~/.config/agentloop/routines/<id>.toml; `flex routines list`\n\
     shows what the store already has. `flex serve --enable-routines` polls for\n\
     due cron-triggered routines and mounts POST /routines/{id}/trigger for\n\
     webhook-triggered ones."
}

/// `flex routines <list|run|remove> [id]`.
pub(crate) async fn routines_cmd(args: &[String]) -> anyhow::Result<()> {
    let Some(store) = FileRoutineStore::with_default_dir() else {
        bail!("could not resolve a home directory for the routines store");
    };
    match args.first().map(String::as_str) {
        Some("list") => {
            let specs = agentloop_channel::RoutineStore::list(&store).await?;
            if specs.is_empty() {
                println!("no routines configured");
            }
            for spec in specs {
                println!("{} — {:?}", spec.id, spec.trigger);
            }
        }
        Some("run") => {
            let id = args.get(1).context("usage: flex routines run <id>")?;
            let spec = agentloop_channel::RoutineStore::get(&store, id)
                .await?
                .with_context(|| format!("routine `{id}` not found"))?;
            let resolution = crate::resolve::resolve_service(None, None, None, None, &[], &[], None)
                .await
                .context("resolving an agent to run the routine with")?;
            let runner = RoutineRunner::new(Arc::new(resolution.service), Arc::new(store));
            let outcome = runner.run_once(&spec).await?;
            println!("stop reason: {:?}", outcome.stop_reason);
        }
        Some("remove") => {
            let id = args.get(1).context("usage: flex routines remove <id>")?;
            agentloop_channel::RoutineStore::remove(&store, id).await?;
            println!("removed `{id}`");
        }
        Some("--help") | Some("-h") | None => println!("{}", usage()),
        Some(other) => bail!("unknown routines argument: {other}\n{}", usage()),
    }
    Ok(())
}
