//! End-to-end isolation over a real git repo and the real `GitWorktrees`
//! backend: `EngineService` provisions a worktree on session creation, an edit
//! there stays out of the base tree, and `integrate_session` merges it back.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use agentloop_contracts::{IsolationPolicy, NewSessionParams};
use agentloop_engine::{EngineOptions, EngineService};
use agentloop_workspace::GitWorktrees;

#[allow(clippy::expect_used)] // test setup: a failing git/fs op is a test bug
fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed");
}

#[allow(clippy::expect_used)] // test setup: a failing git/fs op is a test bug
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

    let service = EngineService::native_all(EngineOptions {
        cwd: base.path().to_path_buf(),
        workspace: Some(Arc::new(GitWorktrees::new(worktrees.path()))),
        isolation_default: IsolationPolicy::Required,
        ..EngineOptions::default()
    })
    .expect("service builds");

    let session = service
        .create_session(NewSessionParams {
            cwd: Some(base.path().to_path_buf()),
            ..NewSessionParams::default()
        })
        .await
        .expect("isolated session opens");
    assert!(service.is_isolated(&session).await.expect("meta"));

    // The worktree lives at <root>/<session-id> (GitWorktrees' layout). An edit
    // there must not appear in the base tree until integration.
    let worktree = worktrees.path().join(session.to_string());
    assert!(worktree.is_dir(), "a real worktree was created");
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
    // After integration the session is repointed to the base and reads as
    // no longer isolated, so a second integrate is a clean error, not a no-op
    // against the base tree.
    assert!(!service.is_isolated(&session).await.expect("meta"));
    assert!(
        matches!(
            service.integrate_session(&session).await,
            Err(agentloop_engine::EngineServiceError::NotIsolated(_))
        ),
        "re-integrating an already-merged session is rejected"
    );
}
