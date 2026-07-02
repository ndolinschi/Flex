//! Headless runner — the composition root and the only brand-named artifact.
//!
//! The runner composes the native loop with real provider clients and prints
//! the canonical event stream as NDJSON.

// The runner's job is to print to stdout; libraries must not.
#![allow(clippy::print_stdout, clippy::print_stderr)]

mod cli;
mod run;

use anyhow::bail;

use agentloop_contracts::branding;

use crate::cli::{parse_run_args, usage};
use crate::run::run;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
