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

use agentloop_engine::{EngineService, EngineServiceError};
use agentloop_providers::delegator_acp::{AcpLaunchConfig, acp_agent};
use agentloop_providers::delegator_claude_code::{
    ClaudeCodeConfig, DelegatorProbeStatus, claude_code_agent, ephemeral_claude_code_agent,
};
use agentloop_providers::delegator_copilot::{
    CopilotConfig, copilot_agent, ephemeral_copilot_agent,
};
use agentloop_providers::delegator_cursor::{
    CursorCliConfig, cursor_agent, ephemeral_cursor_agent,
};
use agentloop_providers::delegator_opencode::{
    OpencodeConfig, ephemeral_opencode_agent, opencode_agent,
};
use agentloop_session::MemoryStore;

use agentloop_sdk::AgentBuilder;

/// A resolved service plus the trace of how it was chosen.
pub(crate) struct Resolution {
    pub(crate) service: EngineService,
    pub(crate) trace: Vec<String>,
}

pub(crate) async fn resolve_service(
    agent: Option<&str>,
    agent_cmd: Option<&str>,
    provider: Option<&str>,
    model: Option<String>,
    fallback_models: &[String],
    plugins: &[String],
    workdir: Option<&Path>,
) -> anyhow::Result<Resolution> {
    let mut trace = Vec::new();

    match agent {
        Some("acp") => {
            trace.push("explicit --agent acp".to_owned());
            let Some(program) = agent_cmd else {
                bail!(
                    "--agent acp needs --agent-cmd <program>: the ACP delegator has no \
                     default binary to launch"
                );
            };
            let service = acp_service(program, workdir, &mut trace).await?;
            return Ok(Resolution { service, trace });
        }
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
        Some("opencode") => {
            trace.push("explicit --agent opencode".to_owned());
            let service = opencode_service(workdir, &mut trace).await?;
            return Ok(Resolution { service, trace });
        }
        Some("cursor") => {
            trace.push("explicit --agent cursor".to_owned());
            let service = cursor_service(workdir, &mut trace).await?;
            return Ok(Resolution { service, trace });
        }
        Some("native") | None if provider.is_some() => {
            trace.push(format!(
                "explicit --provider {}",
                provider.unwrap_or_default()
            ));
            let service = native_service(provider, model, fallback_models, plugins, workdir)?;
            trace.push("selected native loop".to_owned());
            return Ok(Resolution { service, trace });
        }
        Some("native") => {
            trace.push("explicit --agent native".to_owned());
            let service = native_service(provider, model, fallback_models, plugins, workdir)?;
            trace.push("selected native loop (provider from environment)".to_owned());
            return Ok(Resolution { service, trace });
        }
        Some(other) => bail!(
            "unknown agent `{other}`; available: native, claude-code, copilot, opencode, \
             cursor, acp"
        ),
        None => {}
    }

    match native_service(None, model, fallback_models, plugins, workdir) {
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
            if let Ok(service) = copilot_service(workdir, &mut trace).await {
                return Ok(Resolution { service, trace });
            }
            if let Ok(service) = opencode_service(workdir, &mut trace).await {
                return Ok(Resolution { service, trace });
            }
            match cursor_service(workdir, &mut trace).await {
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
    fallback_models: &[String],
    plugins: &[String],
    workdir: Option<&Path>,
) -> Result<EngineService, EngineServiceError> {
    let mut builder = AgentBuilder::new().date(today());
    if let Some(workdir) = workdir {
        builder = builder.cwd(workdir.to_path_buf());
    }
    if let Some(provider) = provider {
        builder = builder.provider(provider);
    }
    if let Some(model) = model {
        builder = builder.model(model);
    }
    if !fallback_models.is_empty() {
        // A fallback entry naming a provider other than --provider needs
        // that provider registered too, or resolution just fails with "no
        // provider registered" the first time the chain advances to it.
        if fallback_models
            .iter()
            .any(|candidate| crosses_provider(provider, candidate))
        {
            builder = builder.all_providers(true);
        }
        builder = builder.fallback_models(fallback_models.to_vec());
    }
    for id in plugins {
        builder = builder.enable_plugin(id);
    }
    builder.build()
}

/// Whether a `provider/model`-qualified fallback entry names a different
/// provider than the one explicitly selected (or the auto-detected default
/// when none was). An unqualified entry inherits whatever provider resolves
/// the primary model, so it never crosses.
fn crosses_provider(selected_provider: Option<&str>, candidate: &str) -> bool {
    let Some((candidate_provider, _)) = candidate.split_once('/') else {
        return false;
    };
    match selected_provider {
        Some(selected) => candidate_provider != selected,
        None => true,
    }
}

async fn claude_code_service(
    workdir: Option<&Path>,
    trace: &mut Vec<String>,
) -> anyhow::Result<EngineService> {
    let config = ClaudeCodeConfig {
        cwd: workdir.map(|p| p.to_path_buf()),
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

async fn copilot_service(
    workdir: Option<&Path>,
    trace: &mut Vec<String>,
) -> anyhow::Result<EngineService> {
    let config = CopilotConfig {
        cwd: workdir.map(|p| p.to_path_buf()),
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

async fn acp_service(
    program: &str,
    workdir: Option<&Path>,
    trace: &mut Vec<String>,
) -> anyhow::Result<EngineService> {
    let config = AcpLaunchConfig {
        program: program.to_owned(),
        args: Vec::new(),
        env: Default::default(),
        cwd: workdir.map(|p| p.to_path_buf()),
    };
    let store = Arc::new(MemoryStore::new());
    let agent = Arc::new(acp_agent(config, store.clone()));
    trace.push(format!("launching ACP agent `{program}` (explicit only)"));
    Ok(EngineService::new(agent, store))
}

async fn opencode_service(
    workdir: Option<&Path>,
    trace: &mut Vec<String>,
) -> anyhow::Result<EngineService> {
    let config = OpencodeConfig {
        cwd: workdir.map(|p| p.to_path_buf()),
        ..OpencodeConfig::default()
    };
    let store = Arc::new(MemoryStore::new());
    let agent = Arc::new(opencode_agent(config, store.clone()));
    match agent.probe(CancellationToken::new()).await {
        Ok(DelegatorProbeStatus::Installed { version }) => {
            trace.push(format!(
                "probed `opencode`: installed ({})",
                version.as_deref().unwrap_or("version unknown")
            ));
            trace.push("selected delegator opencode".to_owned());
            Ok(EngineService::new(agent, store))
        }
        Ok(DelegatorProbeStatus::NotInstalled { hint }) => {
            trace.push("probed `opencode`: not installed".to_owned());
            bail!("opencode is not available: {hint}")
        }
        Err(err) => {
            trace.push(format!("probed `opencode`: failed ({err})"));
            bail!("failed to probe opencode: {err}")
        }
    }
}

async fn cursor_service(
    workdir: Option<&Path>,
    trace: &mut Vec<String>,
) -> anyhow::Result<EngineService> {
    let config = CursorCliConfig {
        cwd: workdir.map(|p| p.to_path_buf()),
        ..CursorCliConfig::default()
    };
    let store = Arc::new(MemoryStore::new());
    let agent = Arc::new(cursor_agent(config, store.clone()));
    match agent.probe(CancellationToken::new()).await {
        Ok(DelegatorProbeStatus::Installed { version }) => {
            trace.push(format!(
                "probed `cursor-agent`: installed ({})",
                version.as_deref().unwrap_or("version unknown")
            ));
            trace.push("selected delegator cursor".to_owned());
            Ok(EngineService::new(agent, store))
        }
        Ok(DelegatorProbeStatus::NotInstalled { hint }) => {
            trace.push("probed `cursor-agent`: not installed".to_owned());
            bail!("cursor is not available: {hint}")
        }
        Err(err) => {
            trace.push(format!("probed `cursor-agent`: failed ({err})"));
            bail!("failed to probe cursor: {err}")
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
    let workdir = Some(workdir);
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
    let copilot_auth = if agentloop_providers::copilot::CopilotConfig::discoverable() {
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
    let opencode = ephemeral_opencode_agent(OpencodeConfig::default());
    match opencode.probe(CancellationToken::new()).await {
        Ok(DelegatorProbeStatus::Installed { version }) => {
            println!(
                "  opencode: installed ({})",
                version.as_deref().unwrap_or("version unknown")
            );
        }
        Ok(DelegatorProbeStatus::NotInstalled { hint }) => {
            println!("  opencode: not installed ({hint})");
        }
        Err(err) => println!("  opencode: probe failed ({err})"),
    }
    let cursor = ephemeral_cursor_agent(CursorCliConfig::default());
    match cursor.probe(CancellationToken::new()).await {
        Ok(DelegatorProbeStatus::Installed { version }) => {
            println!(
                "  cursor: installed ({})",
                version.as_deref().unwrap_or("version unknown")
            );
        }
        Ok(DelegatorProbeStatus::NotInstalled { hint }) => {
            println!("  cursor: not installed ({hint})");
        }
        Err(err) => println!("  cursor: probe failed ({err})"),
    }

    println!("execution backends:");
    {
        use agentloop_core::Executor as _;
        let local = agentloop_executors::LocalExecutor;
        let health = local.probe().await;
        println!("  local: available ({})", health.detail);
        let docker = agentloop_executors::DockerExecutor::new("(configured image)");
        let health = docker.probe().await;
        let state = if health.available {
            "available"
        } else {
            "unavailable"
        };
        println!("  docker: {state} ({})", health.detail);
        let image = agentloop_executors::ContainerImageExecutor::new("(configured image)");
        let health = image.probe().await;
        let state = if health.available {
            "available"
        } else {
            "unavailable"
        };
        println!("  container-image: {state} ({})", health.detail);
        let remote = agentloop_executors::RemoteFnExecutor;
        let health = remote.probe().await;
        println!("  remote-fn: unavailable ({})", health.detail);
    }

    println!("resolution:");
    match resolve_service(None, None, None, None, &[], &[], workdir).await {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_provider_qualified_fallback_does_not_cross() {
        assert!(!crosses_provider(
            Some("anthropic"),
            "anthropic/claude-haiku"
        ));
    }

    #[test]
    fn different_provider_qualified_fallback_crosses() {
        assert!(crosses_provider(Some("anthropic"), "openai/gpt-5"));
    }

    #[test]
    fn unqualified_fallback_never_crosses() {
        assert!(!crosses_provider(Some("anthropic"), "claude-haiku"));
        assert!(!crosses_provider(None, "claude-haiku"));
    }

    #[test]
    fn qualified_fallback_with_no_explicit_provider_crosses() {
        // We don't know what auto-detect will pick, so err on the side of
        // registering every available provider.
        assert!(crosses_provider(None, "openai/gpt-5"));
    }
}
