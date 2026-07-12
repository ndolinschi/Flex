//! End-to-end harness self-test: a scripted `MockProvider` deterministically
//! solves the shipped `create-file` task, proving runner → check → report
//! with no network and no API keys. The testkit is a dev-dependency only.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentloop_core::ProviderRegistry;
use agentloop_engine::{EngineConfig, EngineService};
use agentloop_eval::{EvalError, EvalTarget, ServiceFactory, SuiteConfig, load_task, run_suite};
use agentloop_testkit::MockProvider;

fn create_file_task_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/tasks/create-file.toml")
}

/// A factory whose service is backed by a mock provider that writes the file
/// the `create-file` check expects (when `solve` is true) or just talks.
fn mock_factory(solve: bool) -> ServiceFactory {
    Arc::new(move |_target: EvalTarget, cwd: PathBuf| {
        Box::pin(async move {
            let provider = MockProvider::new();
            if solve {
                let (turn, _ids) = MockProvider::tool_turn(&[(
                    "Write",
                    serde_json::json!({
                        "file_path": cwd.join("hello.txt").to_string_lossy(),
                        "content": "hello world\n",
                    }),
                )]);
                provider.push_turn(turn);
            }
            provider.push_turn(MockProvider::text_turn("All done."));

            let mut registry = ProviderRegistry::new();
            registry.register(Arc::new(provider));
            let config = EngineConfig {
                cwd: Some(cwd),
                date: "2026-07-08".to_owned(),
                ..EngineConfig::default()
            };
            EngineService::native(registry, Some("mock/mock-1".into()), config)
                .map_err(|err| EvalError::Service(err.to_string()))
        })
    })
}

#[tokio::test]
async fn scripted_provider_solves_create_file_end_to_end() {
    let task = load_task(&create_file_task_path()).expect("shipped task parses");
    let out = tempfile::tempdir().expect("tempdir");

    let report = run_suite(SuiteConfig {
        tasks: vec![task],
        targets: vec![EvalTarget::default()],
        repeat: 1,
        out_dir: out.path().to_path_buf(),
        factory: mock_factory(true),
    })
    .await
    .expect("suite runs");

    assert_eq!(report.results.len(), 1);
    let run = &report.results[0];
    assert!(run.passed, "run failed: {:?}", run.failure);
    assert_eq!(run.task_id, "create-file");
    assert!(run.metrics.turns >= 1);
    assert!(run.metrics.num_tool_calls >= 1, "the Write tool was called");
    assert!(run.metrics.usage.input > 0);

    // The raw JSONL transcript was dumped and contains a TurnCompleted event.
    let transcript = run.transcript_path.as_ref().expect("transcript dumped");
    let jsonl = std::fs::read_to_string(transcript).expect("transcript readable");
    assert!(jsonl.lines().any(|line| line.contains("turn_completed")));

    // report.json is the CI artifact and round-trips through the gate.
    let json_path = out.path().join("report.json");
    let loaded = agentloop_eval::SuiteReport::load_json(&json_path).expect("report.json loads");
    assert!(loaded.regressions_against(&report).is_empty());

    let markdown = report.to_markdown();
    assert!(markdown.contains("create-file"));
    assert!(markdown.contains("100% pass rate"));
}

#[tokio::test]
async fn unsolved_task_fails_the_check_and_gates_against_a_passing_baseline() {
    let task = load_task(&create_file_task_path()).expect("shipped task parses");
    let out_pass = tempfile::tempdir().expect("tempdir");
    let out_fail = tempfile::tempdir().expect("tempdir");

    let baseline = run_suite(SuiteConfig {
        tasks: vec![task.clone()],
        targets: vec![EvalTarget::default()],
        repeat: 1,
        out_dir: out_pass.path().to_path_buf(),
        factory: mock_factory(true),
    })
    .await
    .expect("baseline suite runs");

    let current = run_suite(SuiteConfig {
        tasks: vec![task],
        targets: vec![EvalTarget::default()],
        repeat: 1,
        out_dir: out_fail.path().to_path_buf(),
        factory: mock_factory(false),
    })
    .await
    .expect("failing suite runs");

    let run = &current.results[0];
    assert!(!run.passed);
    assert!(
        run.failure
            .as_deref()
            .is_some_and(|f| f.contains("check") || f.contains("expected file missing"))
    );

    let regressions = current.regressions_against(&baseline);
    assert!(
        regressions.iter().any(|r| r.contains("create-file")),
        "gate flags the regression: {regressions:?}"
    );
}
