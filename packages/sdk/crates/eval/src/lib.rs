//! Eval harness: run declarative TOML benchmark tasks against composed
//! engine services and score/report the results.
//!
//! The crate is deliberately composition-root-agnostic: callers inject a
//! [`ServiceFactory`] that builds a fresh [`agentloop_engine::EngineService`]
//! per run (the CLI wires its agent/provider resolver; tests wire a scripted
//! mock provider). Metrics come straight from `TurnCompleted.summary`.

mod error;
pub mod metrics;
pub mod report;
pub mod runner;
pub mod task;

use std::path::PathBuf;

pub use error::EvalError;
pub use metrics::RunMetrics;
pub use report::SuiteReport;
pub use runner::{EvalTarget, RunResult, ServiceFactory, ServiceFuture, run_task};
pub use task::{CheckSpec, TaskSpec, discover_tasks, load_task};

/// Everything one suite invocation needs.
pub struct SuiteConfig {
    pub tasks: Vec<TaskSpec>,
    pub targets: Vec<EvalTarget>,
    /// Runs per (task, target); at least 1.
    pub repeat: u32,
    /// Directory receiving transcripts and `report.json`.
    pub out_dir: PathBuf,
    pub factory: ServiceFactory,
}

/// Run every task against every target `repeat` times, sequentially (rate
/// limits and determinism beat wall-clock here), and write `report.json`
/// into the out dir.
pub async fn run_suite(config: SuiteConfig) -> Result<SuiteReport, EvalError> {
    std::fs::create_dir_all(&config.out_dir)?;
    let repeat = config.repeat.max(1);
    let mut results = Vec::new();
    for target in &config.targets {
        for task in &config.tasks {
            for run_index in 0..repeat {
                tracing::info!(
                    task = %task.id,
                    target = %target.label(),
                    run_index,
                    "running eval task"
                );
                results.push(
                    run_task(task, target, &config.factory, &config.out_dir, run_index).await?,
                );
            }
        }
    }
    let report = SuiteReport::new(results);
    report.write_json(&config.out_dir.join("report.json"))?;
    Ok(report)
}
