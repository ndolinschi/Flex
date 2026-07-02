//! Interactive terminal client entry point.

use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

use agentloop_cli::args::{ArgError, Args, usage};

#[tokio::main]
async fn main() -> Result<()> {
    let _log_guard = init_tracing()?;
    let args = match Args::parse_env() {
        Ok(args) => args,
        Err(ArgError::Help) => {
            writeln!(std::io::stdout(), "{}", usage())?;
            return Ok(());
        }
        Err(err) => {
            writeln!(std::io::stderr(), "{err}\n{}", usage())?;
            std::process::exit(2);
        }
    };
    agentloop_cli::runtime::run(args).await
}

fn init_tracing() -> Result<tracing_appender::non_blocking::WorkerGuard> {
    let dir = log_dir();
    std::fs::create_dir_all(&dir)?;
    let file = tracing_appender::rolling::never(dir, "cli.log");
    let (writer, guard) = tracing_appender::non_blocking(file);
    let filter = std::env::var("AGENTIC_LOG").unwrap_or_else(|_| "info".to_owned());
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(filter))
        .with_writer(writer)
        .with_ansi(false)
        .init();
    Ok(guard)
}

fn log_dir() -> PathBuf {
    if let Ok(state_home) = std::env::var("XDG_STATE_HOME") {
        if !state_home.trim().is_empty() {
            return PathBuf::from(state_home).join("agenticstudio");
        }
    }
    std::env::var("HOME")
        .ok()
        .filter(|home| !home.trim().is_empty())
        .map(|home| {
            PathBuf::from(home)
                .join(".local")
                .join("state")
                .join("agenticstudio")
        })
        .unwrap_or_else(|| PathBuf::from("."))
}
