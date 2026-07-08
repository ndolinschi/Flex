//! Suite reports: markdown rendering, JSON artifact, and the baseline
//! regression gate.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::EvalError;
use crate::runner::RunResult;

/// Full result of one suite invocation — the CI artifact (`report.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuiteReport {
    /// Unix epoch milliseconds when the suite finished.
    pub generated_at_ms: u64,
    pub results: Vec<RunResult>,
}

/// A `(task, target)` cell aggregated over repeats: passed iff every run passed.
type Cells = BTreeMap<(String, String), bool>;

impl SuiteReport {
    pub fn new(results: Vec<RunResult>) -> Self {
        Self {
            generated_at_ms: agentloop_contracts::now_ms(),
            results,
        }
    }

    fn cells(&self) -> Cells {
        let mut cells = Cells::new();
        for run in &self.results {
            let entry = cells
                .entry((run.task_id.clone(), run.target.clone()))
                .or_insert(true);
            *entry &= run.passed;
        }
        cells
    }

    /// Fraction of `(task, target)` cells whose every run passed.
    pub fn pass_rate(&self) -> f64 {
        let cells = self.cells();
        if cells.is_empty() {
            return 0.0;
        }
        cells.values().filter(|passed| **passed).count() as f64 / cells.len() as f64
    }

    /// Render one markdown table per target plus a summary line.
    pub fn to_markdown(&self) -> String {
        let mut targets: Vec<&str> = self.results.iter().map(|r| r.target.as_str()).collect();
        targets.sort_unstable();
        targets.dedup();

        let mut out = String::new();
        for target in targets {
            let _ = writeln!(out, "## target `{target}`\n");
            let _ = writeln!(
                out,
                "| task | run | pass | turns | tool calls | tokens in/out | cost usd | wall s | failure |"
            );
            let _ = writeln!(out, "|---|---|---|---|---|---|---|---|---|");
            for run in self.results.iter().filter(|r| r.target == target) {
                let _ = writeln!(
                    out,
                    "| {} | {} | {} | {} | {} | {}/{} | {} | {:.1} | {} |",
                    run.task_id,
                    run.run_index,
                    if run.passed { "pass" } else { "FAIL" },
                    run.metrics.turns,
                    run.metrics.num_tool_calls,
                    run.metrics.usage.input,
                    run.metrics.usage.output,
                    run.metrics
                        .cost_usd
                        .map(|c| format!("{c:.4}"))
                        .unwrap_or_else(|| "-".to_owned()),
                    run.metrics.wall_ms as f64 / 1000.0,
                    run.failure
                        .as_deref()
                        .map(|f| f.replace(['|', '\n'], " "))
                        .unwrap_or_default(),
                );
            }
            out.push('\n');
        }
        let cells = self.cells();
        let passed = cells.values().filter(|p| **p).count();
        let _ = writeln!(
            out,
            "**{passed}/{} passed ({:.0}% pass rate)**",
            cells.len(),
            self.pass_rate() * 100.0
        );
        out
    }

    /// Write the JSON artifact.
    pub fn write_json(&self, path: &Path) -> Result<(), EvalError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    /// Load a previously written JSON artifact (the gate baseline).
    pub fn load_json(path: &Path) -> Result<Self, EvalError> {
        let bytes = std::fs::read(path)?;
        serde_json::from_slice(&bytes).map_err(|err| EvalError::Baseline {
            path: path.to_path_buf(),
            message: err.to_string(),
        })
    }

    /// Regressions versus a baseline: every `(task, target)` cell that passed
    /// in the baseline and fails (or vanished) now, plus an overall pass-rate
    /// drop. Empty = the gate is green.
    pub fn regressions_against(&self, baseline: &SuiteReport) -> Vec<String> {
        let mut regressions = Vec::new();
        let current = self.cells();
        for ((task, target), passed) in baseline.cells() {
            if !passed {
                continue;
            }
            match current.get(&(task.clone(), target.clone())) {
                Some(true) => {}
                Some(false) => {
                    regressions.push(format!("{task} on {target}: passed in baseline, now fails"))
                }
                None => regressions.push(format!(
                    "{task} on {target}: passed in baseline, missing from this run"
                )),
            }
        }
        let (current_rate, baseline_rate) = (self.pass_rate(), baseline.pass_rate());
        if current_rate < baseline_rate {
            regressions.push(format!(
                "pass rate dropped: {:.0}% -> {:.0}%",
                baseline_rate * 100.0,
                current_rate * 100.0
            ));
        }
        regressions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::RunMetrics;

    fn run(task: &str, target: &str, run_index: u32, passed: bool) -> RunResult {
        RunResult {
            task_id: task.to_owned(),
            target: target.to_owned(),
            run_index,
            passed,
            failure: (!passed).then(|| "boom".to_owned()),
            metrics: RunMetrics::default(),
            transcript_path: None,
        }
    }

    #[test]
    fn cell_passes_only_when_every_repeat_passes() {
        let report = SuiteReport::new(vec![
            run("a", "t", 0, true),
            run("a", "t", 1, false),
            run("b", "t", 0, true),
        ]);
        assert_eq!(report.pass_rate(), 0.5);
    }

    #[test]
    fn markdown_contains_summary_and_rows() {
        let report = SuiteReport::new(vec![run("a", "t", 0, true), run("b", "t", 0, false)]);
        let md = report.to_markdown();
        assert!(md.contains("## target `t`"));
        assert!(md.contains("| a | 0 | pass |"));
        assert!(md.contains("| b | 0 | FAIL |"));
        assert!(md.contains("**1/2 passed (50% pass rate)**"));
    }

    #[test]
    fn gate_flags_new_failures_missing_cells_and_rate_drops() {
        let baseline = SuiteReport::new(vec![
            run("a", "t", 0, true),
            run("b", "t", 0, true),
            run("c", "t", 0, false),
        ]);
        let current = SuiteReport::new(vec![run("a", "t", 0, false), run("c", "t", 0, false)]);
        let regressions = current.regressions_against(&baseline);
        assert_eq!(regressions.len(), 3, "{regressions:?}");
        assert!(regressions.iter().any(|r| r.contains("a on t")));
        assert!(regressions.iter().any(|r| r.contains("b on t")));
        assert!(regressions.iter().any(|r| r.contains("pass rate dropped")));

        assert!(baseline.regressions_against(&baseline).is_empty());
    }

    #[test]
    fn json_round_trips() {
        let report = SuiteReport::new(vec![run("a", "t", 0, true)]);
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("report.json");
        report.write_json(&path).expect("writes");
        let loaded = SuiteReport::load_json(&path).expect("loads");
        assert_eq!(loaded.results.len(), 1);
        assert_eq!(loaded.results[0].task_id, "a");
    }
}
