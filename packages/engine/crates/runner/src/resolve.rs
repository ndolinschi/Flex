//! Agent-implementation resolution — the composition-root half of "which
//! agent serves this run".
//!
//! Precedence: explicit `--agent` > explicit `--provider` > auto-detect
//! (provider API keys in the environment → native loop; otherwise a probed,
//! installed external CLI → delegator). Every decision is recorded in a
//! human-readable trace so `doctor` and logs can explain the outcome.

use std::path::Path;
use std::sync::Arc;

use anyhow::bail;
use tokio_util::sync::CancellationToken;

use agentloop_delegator_claude_code::{
    ClaudeCodeConfig, DelegatorProbeStatus, claude_code_agent, ephemeral_claude_code_agent,
};
use agentloop_delegator_copilot::{CopilotConfig, copilot_agent, ephemeral_copilot_agent};
use agentloop_engine::{EngineOptions, EngineService, EngineServiceError};
use agentloop_session::MemoryStore;

/// A resolved service plus the trace of how it was chosen.
pub(crate) struct Resolution {
    pub(crate) service: EngineService,
    pub(crate) trace: Vec<String>,
}

pub(crate) async fn resolve_service(
    agent: Option<&str>,
    provider: Option<&str>,
    model: Option<String>,
    workdir: &Path,
) -> anyhow::Result<Resolution> {
    let mut trace = Vec::new();

    match agent {
        Some("claude-code") => {
            trace.push("explicit --agent claude-code".to_owned());
            let service = claude_code_service(workdir, &mut trace).await?;
            return Ok(Resolution { service, trace });
        }
        Some("copilot") => {
            trace.push("explicit --agent copilot".to_owned());
            let service = copilot_service(workdir, &mut trace).await?;
            return Ok(Resolution { service, trace });
        }
        Some("native") | None if provider.is_some() => {
            trace.push(format!(
                "explicit --provider {}",
                provider.unwrap_or_default()
            ));
            let service = native_service(provider, model, workdir)?;
            trace.push("selected native loop".to_owned());
            return Ok(Resolution { service, trace });
        }
        Some("native") => {
            trace.push("explicit --agent native".to_owned());
            let service = native_service(provider, model, workdir)?;
            trace.push("selected native loop (provider from environment)".to_owned());
            return Ok(Resolution { service, trace });
        }
        Some(other) => bail!("unknown agent `{other}`; available: native, claude-code, copilot"),
        None => {}
    }

    // Auto-detect: provider keys win; otherwise probe installed CLIs.
    match native_service(None, model, workdir) {
        Ok(service) => {
            trace.push("provider API key found in environment".to_owned());
            trace.push("selected native loop".to_owned());
            Ok(Resolution { service, trace })
        }
        Err(err) if is_auth_missing(&err) => {
            trace.push("no provider API keys in environment".to_owned());
            if let Ok(service) = claude_code_service(workdir, &mut trace).await {
                return Ok(Resolution { service, trace });
            }
            match copilot_service(workdir, &mut trace).await {
                Ok(service) => Ok(Resolution { service, trace }),
                Err(delegator_err) => bail!(
                    "no way to run: {err}\n\
                     no external agent CLI is usable either: {delegator_err}\n\
                     resolution trace:\n  - {}",
                    trace.join("\n  - ")
                ),
            }
        }
        Err(err) => Err(err.into()),
    }
}

fn native_service(
    provider: Option<&str>,
    model: Option<String>,
    workdir: &Path,
) -> Result<EngineService, EngineServiceError> {
    EngineService::native(EngineOptions {
        provider: provider.map(str::to_owned),
        model,
        cwd: workdir.to_path_buf(),
        date: today(),
        custom: Vec::new(),
        roles: Vec::new(),
        mcp: Default::default(),
        mcp_manager: None,
        session_store: None,
        max_iterations: None,
        // The headless runner is one-shot; workspace isolation (with its
        // review/merge step) belongs to the interactive client. Left off here.
        workspace: None,
        isolation_default: Default::default(),
        verify_command: None,
    })
}

async fn claude_code_service(
    workdir: &Path,
    trace: &mut Vec<String>,
) -> anyhow::Result<EngineService> {
    let config = ClaudeCodeConfig {
        cwd: Some(workdir.to_path_buf()),
        ..ClaudeCodeConfig::default()
    };
    let store = Arc::new(MemoryStore::new());
    let agent = Arc::new(claude_code_agent(config, store.clone()));
    match agent.probe(CancellationToken::new()).await {
        Ok(DelegatorProbeStatus::Installed { version }) => {
            trace.push(format!(
                "probed `claude`: installed ({})",
                version.as_deref().unwrap_or("version unknown")
            ));
            trace.push("selected delegator claude-code".to_owned());
            Ok(EngineService::new(agent, store))
        }
        Ok(DelegatorProbeStatus::NotInstalled { hint }) => {
            trace.push("probed `claude`: not installed".to_owned());
            bail!("claude-code is not available: {hint}")
        }
        Err(err) => {
            trace.push(format!("probed `claude`: failed ({err})"));
            bail!("failed to probe claude-code: {err}")
        }
    }
}

async fn copilot_service(workdir: &Path, trace: &mut Vec<String>) -> anyhow::Result<EngineService> {
    let config = CopilotConfig {
        cwd: Some(workdir.to_path_buf()),
        ..CopilotConfig::default()
    };
    let store = Arc::new(MemoryStore::new());
    let agent = Arc::new(copilot_agent(config, store.clone()));
    match agent.probe(CancellationToken::new()).await {
        Ok(DelegatorProbeStatus::Installed { version }) => {
            trace.push(format!(
                "probed `copilot`: installed ({})",
                version.as_deref().unwrap_or("version unknown")
            ));
            trace.push("selected delegator copilot".to_owned());
            Ok(EngineService::new(agent, store))
        }
        Ok(DelegatorProbeStatus::NotInstalled { hint }) => {
            trace.push("probed `copilot`: not installed".to_owned());
            bail!("copilot is not available: {hint}")
        }
        Err(err) => {
            trace.push(format!("probed `copilot`: failed ({err})"));
            bail!("failed to probe copilot: {err}")
        }
    }
}

fn is_auth_missing(err: &EngineServiceError) -> bool {
    matches!(
        err.to_engine_error().code,
        agentloop_contracts::ErrorCode::AuthMissing
    )
}

fn today() -> String {
    // Coarse ISO date from the epoch — the runner has no chrono dependency
    // and the prompt only needs day resolution.
    let days = agentloop_contracts::now_ms() / 86_400_000;
    let mut year = 1970u64;
    let mut remaining = days;
    loop {
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let len = if leap { 366 } else { 365 };
        if remaining < len {
            break;
        }
        remaining -= len;
        year += 1;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_lengths = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    for len in month_lengths {
        if remaining < len {
            break;
        }
        remaining -= len;
        month += 1;
    }
    format!("{year:04}-{month:02}-{:02}", remaining + 1)
}

/// The `doctor` subcommand: explain what the resolver would do and why.
pub(crate) async fn doctor(workdir: &Path) -> anyhow::Result<()> {
    println!("environment:");
    for key in [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "GEMINI_API_KEY",
        "OLLAMA_HOST",
        "OLLAMA_MODEL",
    ] {
        let state = match std::env::var(key) {
            Ok(value) if !value.trim().is_empty() => "set",
            _ => "not set",
        };
        println!("  {key}: {state}");
    }
    let copilot_auth = if agentloop_provider_copilot::CopilotConfig::discoverable() {
        "found (env token or editor/CLI sign-in)"
    } else {
        "not found (sign in with VS Code or the Copilot CLI, or set COPILOT_GITHUB_TOKEN)"
    };
    println!("  github copilot auth: {copilot_auth}");

    println!("external agents:");
    let config = ClaudeCodeConfig::default();
    let agent = ephemeral_claude_code_agent(config);
    match agent.probe(CancellationToken::new()).await {
        Ok(DelegatorProbeStatus::Installed { version }) => {
            println!(
                "  claude-code: installed ({})",
                version.as_deref().unwrap_or("version unknown")
            );
        }
        Ok(DelegatorProbeStatus::NotInstalled { hint }) => {
            println!("  claude-code: not installed ({hint})");
        }
        Err(err) => println!("  claude-code: probe failed ({err})"),
    }
    let copilot = ephemeral_copilot_agent(CopilotConfig::default());
    match copilot.probe(CancellationToken::new()).await {
        Ok(DelegatorProbeStatus::Installed { version }) => {
            println!(
                "  copilot: installed ({})",
                version.as_deref().unwrap_or("version unknown")
            );
        }
        Ok(DelegatorProbeStatus::NotInstalled { hint }) => {
            println!("  copilot: not installed ({hint})");
        }
        Err(err) => println!("  copilot: probe failed ({err})"),
    }

    println!("resolution:");
    match resolve_service(None, None, None, workdir).await {
        Ok(resolution) => {
            for line in &resolution.trace {
                println!("  - {line}");
            }
        }
        Err(err) => {
            println!("  - no usable agent: {err}");
        }
    }
    Ok(())
}
