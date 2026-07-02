//! CLI argument parsing for the headless runner.

use std::path::PathBuf;

use anyhow::{Context, bail};

use agentloop_contracts::branding;

#[derive(Debug)]
pub(crate) struct RunArgs {
    pub(crate) agent: Option<String>,
    pub(crate) provider: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) prompt: String,
    pub(crate) workdir: PathBuf,
    pub(crate) output_format: OutputFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    Ndjson,
}

pub(crate) fn parse_run_args(args: &[String]) -> anyhow::Result<RunArgs> {
    let mut agent: Option<String> = None;
    let mut provider: Option<String> = None;
    let mut model: Option<String> = None;
    let mut prompt: Option<String> = None;
    let mut workdir: Option<PathBuf> = None;
    let output_format = OutputFormat::Ndjson;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--agent" => {
                agent = Some(take_value(args, &mut index, "--agent")?);
            }
            "--provider" => {
                provider = Some(take_value(args, &mut index, "--provider")?);
            }
            "--model" => {
                model = Some(take_value(args, &mut index, "--model")?);
            }
            "-p" | "--prompt" => {
                prompt = Some(take_value(args, &mut index, "-p/--prompt")?);
            }
            "--workdir" => {
                workdir = Some(PathBuf::from(take_value(args, &mut index, "--workdir")?));
            }
            "--output-format" => {
                let value = take_value(args, &mut index, "--output-format")?;
                if value != "ndjson" {
                    bail!("unsupported --output-format `{value}`; only `ndjson` is implemented");
                }
            }
            "--help" | "-h" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            other => bail!("unknown run argument: {other}\n{}", usage()),
        }
        index += 1;
    }

    Ok(RunArgs {
        agent,
        provider,
        model,
        prompt: prompt.context("missing prompt: pass -p \"...\" or --prompt \"...\"")?,
        workdir: match workdir {
            Some(path) => path,
            None => std::env::current_dir().context("cannot determine current directory")?,
        },
        output_format,
    })
}

fn take_value(args: &[String], index: &mut usize, flag: &str) -> anyhow::Result<String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .with_context(|| format!("{flag} requires a value"))
}

pub(crate) fn usage() -> String {
    format!(
        "usage:\n  {slug} --version\n  {slug} doctor\n  {slug} run [--agent native|claude-code] \
         [--provider anthropic|openai|gemini|ollama] [--model <model>] -p <prompt> \
         [--workdir <path>] [--output-format ndjson]\n\n\
         With no --agent/--provider, the engine auto-detects: provider API keys in the \
         environment select the native loop; otherwise an installed external agent CLI is \
         probed and delegated to. `doctor` explains the decision.",
        slug = branding::PRODUCT_SLUG
    )
}
