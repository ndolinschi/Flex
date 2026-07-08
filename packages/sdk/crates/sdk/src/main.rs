//! Headless runner — the composition root and the only brand-named artifact.
//!
//! The runner composes the native loop with real provider clients and prints
//! the canonical event stream as NDJSON.

#![allow(clippy::print_stdout, clippy::print_stderr)]

mod cli;
mod resolve;
mod run;

use anyhow::bail;

use agentloop_contracts::branding;

use crate::cli::{parse_run_args, usage};
use crate::run::run;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version" | "-V" | "version") => {
            println!("{} {}", branding::PRODUCT_NAME, branding::ENGINE_VERSION);
            Ok(())
        }
        Some("--help" | "-h" | "help") => {
            println!("{}", usage());
            Ok(())
        }
        Some("doctor") => {
            let cwd = std::env::current_dir()?;
            resolve::doctor(&cwd).await
        }
        Some("run") => run(parse_run_args(&args[1..])?).await,
        Some(other) if other.starts_with('-') => run(parse_run_args(&args)?).await,
        Some(other) => bail!("unknown argument: {other}\n{}", usage()),
        None => {
            println!(
                "{} {} - agent-loop engine (pre-alpha)\n{}",
                branding::PRODUCT_NAME,
                branding::ENGINE_VERSION,
                usage()
            );
            Ok(())
        }
    }
}

/// Logs go to stderr (stdout carries the NDJSON event stream). Controlled by
/// the engine's log env var; silent by default.
fn init_tracing() {
    let env_var = format!("{}_LOG", branding::ENV_PREFIX);
    let filter = tracing_subscriber::EnvFilter::try_from_env(&env_var)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
