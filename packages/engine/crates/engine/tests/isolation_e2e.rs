//! End-to-end isolation over a real git repo and the real `GitWorktrees`
//! backend: `EngineService` provisions a worktree on the first prompt of an
//! isolated session (deferred from `create_session`), an edit there stays out
//! of the base tree, and `integrate_session` merges it back.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use agentloop_contracts::{IsolationPolicy, ModelRef, NewSessionParams, PromptInput, TurnOptions};
use agentloop_core::ProviderRegistry;
use agentloop_engine::{EngineConfig, EngineService};
use agentloop_testkit::{MOCK_MODEL, MOCK_PROVIDER_ID, MockProvider};
use agentloop_workspace::GitWorktrees;

#[allow(clippy::expect_used)]
fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed");
}

#[allow(clippy::expect_used)]
fn init_repo(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test"]);
    std::fs::write(dir.join("seed.txt"), "seed\n").expect("seed");
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "seed"]);
}

#[tokio::test]
async fn engine_provisions_a_real_worktree_and_merges_it_back() {
    let base = tempfile::tempdir().expect("base");
    let worktrees = tempfile::tempdir().expect("worktrees");
    init_repo(base.path());

    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(MockProvider::new()));
    let default_model = Some(ModelRef(format!("{MOCK_PROVIDER_ID}/{MOCK_MODEL}")));

    let service = EngineService::native(
        providers,
        default_model,
        EngineConfig {
            cwd: Some(base.path().to_path_buf()),
            workspace: Some(Arc::new(GitWorktrees::new(worktrees.path()))),
            isolation_default: IsolationPolicy::Required,
            ..EngineConfig::default()
        },
    )
    .expect("service builds");

    let session = service
        .create_session(NewSessionParams {
            cwd: Some(base.path().to_path_buf()),
            ..NewSessionParams::default()
        })
        .await
        .expect("isolated session opens");

    // Deferred: create does not provision yet.
    assert!(
        !service.is_isolated(&session).await.expect("meta"),
        "workspace not provisioned until the first prompt"
    );
    assert!(
        !worktrees.path().join(session.to_string()).exists(),
        "no worktree directory yet"
    );

    // Drive one prompt: the MockProvider's default turn is a no-tool end-turn,
    // so this executes just far enough to run the first-turn workspace ensure.
    service
        .prompt(&session, PromptInput::text("hello"), TurnOptions::default())
        .await
        .expect("first turn drives workspace ensure");

    assert!(service.is_isolated(&session).await.expect("meta"));
    let worktree = worktrees.path().join(session.to_string());
    assert!(
        worktree.is_dir(),
        "worktree provisioned on first prompt at {}",
        worktree.display()
    );
    std::fs::write(worktree.join("added.txt"), "from the agent\n").expect("write in worktree");
    assert!(
        !base.path().join("added.txt").exists(),
        "base tree is untouched before integrate"
    );

    let status = service
        .workspace_status(&session)
        .await
        .expect("status")
        .expect("isolated");
    assert_eq!(status.files_changed, 1);

    service
        .integrate_session(&session)
        .await
        .expect("integrate");
    assert_eq!(
        std::fs::read_to_string(base.path().join("added.txt")).expect("merged into base"),
        "from the agent\n"
    );
    assert!(!worktree.exists(), "worktree removed after merge");
    assert!(!service.is_isolated(&session).await.expect("meta"));
    assert!(
        matches!(
            service.integrate_session(&session).await,
            Err(agentloop_engine::EngineServiceError::NotIsolated(_))
        ),
        "re-integrating an already-merged session is rejected"
    );
}
