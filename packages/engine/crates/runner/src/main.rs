//! Headless runner — the composition root and the only brand-named artifact.
//!
//! M0 stub: version reporting only. The `run`/`serve`/`doctor` subcommands
//! arrive with the loop, transports, and resolver.

// The runner's job is to print to stdout; libraries must not.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use agentloop_contracts::branding;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version" | "-V" | "version") => {
            println!("{} {}", branding::PRODUCT_NAME, branding::ENGINE_VERSION);
            Ok(())
        }
        Some(other) => {
            eprintln!(
                "unknown argument: {other}\nusage: {} --version",
                branding::PRODUCT_SLUG
            );
            std::process::exit(2);
        }
        None => {
            println!(
                "{} {} — agent-loop engine (pre-alpha)\nusage: {} --version",
                branding::PRODUCT_NAME,
                branding::ENGINE_VERSION,
                branding::PRODUCT_SLUG
            );
            Ok(())
        }
    }
}
