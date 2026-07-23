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

pub struct SuiteConfig {
    pub tasks: Vec<TaskSpec>,
    pub targets: Vec<EvalTarget>,

    pub repeat: u32,

    pub out_dir: PathBuf,
    pub factory: ServiceFactory,
}

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
