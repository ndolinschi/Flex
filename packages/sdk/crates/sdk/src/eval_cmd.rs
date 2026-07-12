//! The `eval` subcommand: benchmark TOML tasks against a resolved agent
//! service and print/report/gate the results.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, bail};

use agentloop_contracts::branding;
use agentloop_eval::{
    EvalError, EvalTarget, ServiceFactory, SuiteConfig, SuiteReport, discover_tasks, run_suite,
};

use crate::resolve::resolve_service;

#[derive(Debug, Default)]
struct EvalArgs {
    tasks: Vec<String>,
    tasks_dir: Option<PathBuf>,
    agent: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    repeat: u32,
    out: Option<PathBuf>,
    json: Option<PathBuf>,
    baseline: Option<PathBuf>,
}

pub(crate) fn eval_usage() -> String {
    format!(
        "usage:\n  {slug} eval [--task <id>]... [--tasks-dir <dir>] [--agent <agent>] \
         [--provider <id>] [--model <model>] [--repeat <n>] [--out <dir>] \
         [--json <path>] [--baseline <report.json>]\n\n\
         Runs each TOML task (default dir: packages/sdk/evals/tasks) in a fresh temp \
         workspace against the resolved agent, scores it with the task's check, prints a \
         markdown report, and writes report.json + per-run JSONL transcripts to the out \
         dir (default: target/eval/<timestamp>). With --baseline, exits nonzero if a \
         previously-passing task now fails or the pass rate drops.",
        slug = branding::PRODUCT_SLUG
    )
}

fn parse_eval_args(args: &[String]) -> anyhow::Result<EvalArgs> {
    let mut parsed = EvalArgs {
        repeat: 1,
        ..EvalArgs::default()
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--task" => parsed.tasks.push(take_value(args, &mut index, "--task")?),
            "--tasks-dir" => {
                parsed.tasks_dir = Some(PathBuf::from(take_value(args, &mut index, "--tasks-dir")?))
            }
            "--agent" => parsed.agent = Some(take_value(args, &mut index, "--agent")?),
            "--provider" => parsed.provider = Some(take_value(args, &mut index, "--provider")?),
            "--model" => parsed.model = Some(take_value(args, &mut index, "--model")?),
            "--repeat" => {
                let value = take_value(args, &mut index, "--repeat")?;
                parsed.repeat = value
                    .parse()
                    .with_context(|| format!("--repeat expects a number, got `{value}`"))?;
                if parsed.repeat == 0 {
                    bail!("--repeat must be at least 1");
                }
            }
            "--out" => parsed.out = Some(PathBuf::from(take_value(args, &mut index, "--out")?)),
            "--json" => parsed.json = Some(PathBuf::from(take_value(args, &mut index, "--json")?)),
            "--baseline" => {
                parsed.baseline = Some(PathBuf::from(take_value(args, &mut index, "--baseline")?))
            }
            "--help" | "-h" => {
                println!("{}", eval_usage());
                std::process::exit(0);
            }
            other => bail!("unknown eval argument: {other}\n{}", eval_usage()),
        }
        index += 1;
    }
    Ok(parsed)
}

fn take_value(args: &[String], index: &mut usize, flag: &str) -> anyhow::Result<String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .with_context(|| format!("{flag} requires a value"))
}

fn default_tasks_dir() -> anyhow::Result<PathBuf> {
    for candidate in ["packages/sdk/evals/tasks", "evals/tasks"] {
        let path = PathBuf::from(candidate);
        if path.is_dir() {
            return Ok(path);
        }
    }
    bail!("no tasks dir found (tried packages/sdk/evals/tasks and evals/tasks); pass --tasks-dir")
}

pub(crate) async fn eval(args: &[String]) -> anyhow::Result<()> {
    let args = parse_eval_args(args)?;
    let tasks_dir = match args.tasks_dir {
        Some(dir) => dir,
        None => default_tasks_dir()?,
    };
    let tasks = discover_tasks(&tasks_dir, &args.tasks)?;
    let target = EvalTarget {
        agent: args.agent,
        provider: args.provider,
        model: args.model,
    };
    let out_dir = args.out.unwrap_or_else(|| {
        PathBuf::from("target")
            .join("eval")
            .join(agentloop_contracts::now_ms().to_string())
    });

    let factory: ServiceFactory = Arc::new(|target: EvalTarget, cwd: PathBuf| {
        Box::pin(async move {
            let resolution = resolve_service(
                target.agent.as_deref(),
                None,
                target.provider.as_deref(),
                target.model.clone(),
                &[],
                &[],
                Some(&cwd),
            )
            .await
            .map_err(|err| EvalError::Service(err.to_string()))?;
            Ok(resolution.service)
        })
    });

    let report = run_suite(SuiteConfig {
        tasks,
        targets: vec![target],
        repeat: args.repeat,
        out_dir: out_dir.clone(),
        factory,
    })
    .await?;

    println!("{}", report.to_markdown());
    println!("report: {}", out_dir.join("report.json").display());
    if let Some(json) = &args.json {
        report.write_json(json)?;
    }

    if let Some(baseline_path) = &args.baseline {
        let baseline = SuiteReport::load_json(baseline_path)?;
        let regressions = report.regressions_against(&baseline);
        if !regressions.is_empty() {
            eprintln!("regressions against {}:", baseline_path.display());
            for regression in &regressions {
                eprintln!("  - {regression}");
            }
            std::process::exit(1);
        }
        println!("baseline gate: green");
    }
    Ok(())
}
