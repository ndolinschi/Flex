//! Git-worktree implementation of [`agentloop_core::Workspaces`].
//!
//! A provisioned workspace is a `git worktree` checked out on a fresh branch,
//! branched from the base repository's current `HEAD`. The session's tools run
//! there (via `SessionMeta.cwd`), so every edit is contained. Integration
//! commits the work, optionally runs a verify command, and fast-forwards the
//! base branch onto it; discard removes the worktree and its branch.
//!
//! This is the sanctioned I/O edge for isolation: it is the only place that
//! spawns `git`.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::process::Command;

use agentloop_contracts::branding::PRODUCT_SLUG;
use agentloop_contracts::{IntegrationOutcome, IsolationPolicy, SessionId};
use agentloop_core::workspace::{Workspace, WorkspaceError, WorkspaceStatus, Workspaces};

/// Win32 `CREATE_NO_WINDOW` — do not allocate a new console for the child.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Apply creation flags so a console child does not flash a window when
/// spawned from a GUI parent (desktop). No-op on non-Windows.
fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    #[cfg(not(windows))]
    let _ = command;
}

/// Provisions git-worktree-backed isolated workspaces under `root`.
pub struct GitWorktrees {
    /// Directory under which per-session worktrees are created.
    root: PathBuf,
}

impl GitWorktrees {
    /// Create a provisioner that places worktrees under `root` (e.g.
    /// `~/.local/state/<slug>/worktrees`).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The commit message for the single squash-style commit that captures a
    /// session's work. Brand-free by design.
    fn commit_message() -> &'static str {
        "agent session changes"
    }
}

/// Run `git` in `dir`, returning trimmed stdout. Spawn failure → `GitUnavailable`;
/// a non-zero exit → `GitFailed` (with stderr).
async fn git(dir: &Path, args: &[&str]) -> Result<String, WorkspaceError> {
    let mut command = Command::new("git");
    hide_console(&mut command);
    let output = command
        .current_dir(dir)
        .args(args)
        .output()
        .await
        .map_err(|err| WorkspaceError::GitUnavailable(err.to_string()))?;
    if !output.status.success() {
        return Err(WorkspaceError::GitFailed(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Map a "cannot isolate" condition to the policy-appropriate result: an error
/// when isolation is required, a graceful `None` when it is optional.
fn cannot_isolate(
    policy: IsolationPolicy,
    base: &Path,
) -> Result<Option<Workspace>, WorkspaceError> {
    if policy.is_required() {
        Err(WorkspaceError::NotAGitRepo(base.to_path_buf()))
    } else {
        Ok(None)
    }
}

fn path_str(path: &Path) -> Result<&str, WorkspaceError> {
    path.to_str()
        .ok_or_else(|| WorkspaceError::Io(format!("non-UTF-8 path: {}", path.display())))
}

/// Count of changed entries reported by `git status --porcelain`.
fn changed_count(porcelain: &str) -> u32 {
    porcelain
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count() as u32
}

#[async_trait]
impl Workspaces for GitWorktrees {
    async fn provision(
        &self,
        base: &Path,
        session: &SessionId,
        policy: IsolationPolicy,
    ) -> Result<Option<Workspace>, WorkspaceError> {
        let toplevel = match git(base, &["rev-parse", "--show-toplevel"]).await {
            Ok(top) => PathBuf::from(top),
            Err(_) => return cannot_isolate(policy, base),
        };
        let base_ref = match git(&toplevel, &["rev-parse", "HEAD"]).await {
            Ok(sha) => sha,
            Err(_) => return cannot_isolate(policy, base),
        };

        tokio::fs::create_dir_all(&self.root)
            .await
            .map_err(|err| WorkspaceError::Io(err.to_string()))?;
        let worktree_root = self.root.join(session.to_string());
        let branch = format!("{PRODUCT_SLUG}/session-{session}");

        git(
            &toplevel,
            &[
                "worktree",
                "add",
                "-b",
                &branch,
                path_str(&worktree_root)?,
                &base_ref,
            ],
        )
        .await?;

        tracing::info!(
            target: "workspace",
            session = %session,
            worktree = %worktree_root.display(),
            branch = %branch,
            "provisioned isolated workspace"
        );
        Ok(Some(Workspace {
            id: branch,
            root: worktree_root,
            base_ref,
        }))
    }

    async fn status(&self, root: &Path) -> Result<WorkspaceStatus, WorkspaceError> {
        if !root.exists() {
            return Err(WorkspaceError::NotFound(root.to_path_buf()));
        }
        let porcelain = git(root, &["status", "--porcelain"]).await?;
        Ok(WorkspaceStatus {
            files_changed: changed_count(&porcelain),
            summary: if porcelain.is_empty() {
                "no changes".to_owned()
            } else {
                porcelain
            },
        })
    }

    async fn integrate(
        &self,
        root: &Path,
        base: &Path,
        verify: Option<&str>,
    ) -> Result<IntegrationOutcome, WorkspaceError> {
        if !root.exists() {
            return Err(WorkspaceError::NotFound(root.to_path_buf()));
        }
        let porcelain = git(root, &["status", "--porcelain"]).await?;
        if !porcelain.is_empty() {
            git(root, &["add", "-A"]).await?;
            git(root, &["commit", "-m", Self::commit_message()]).await?;
        }

        let base_head = git(base, &["rev-parse", "HEAD"]).await?;
        let range = format!("{base_head}..HEAD");
        let ahead = git(root, &["rev-list", "--count", &range]).await?;
        if ahead.trim() == "0" {
            self.discard(root, base).await?;
            return Ok(IntegrationOutcome::Empty);
        }
        let files_changed = changed_count(&git(root, &["diff", "--name-only", &range]).await?);

        if let Some(cmd) = verify {
            if let Some(detail) = run_verify(root, cmd).await? {
                return Ok(IntegrationOutcome::VerifyFailed { detail });
            }
        }

        let branch = git(root, &["rev-parse", "--abbrev-ref", "HEAD"]).await?;
        match git(base, &["merge", "--ff-only", &branch]).await {
            Ok(_) => {
                let _ = git(base, &["worktree", "remove", "--force", path_str(root)?]).await;
                let _ = git(base, &["branch", "-D", &branch]).await;
                Ok(IntegrationOutcome::Merged { files_changed })
            }
            Err(_) => Ok(IntegrationOutcome::Diverged { branch }),
        }
    }

    async fn discard(&self, root: &Path, base: &Path) -> Result<(), WorkspaceError> {
        let branch = git(root, &["rev-parse", "--abbrev-ref", "HEAD"]).await.ok();
        if let Ok(root_str) = path_str(root) {
            let _ = git(base, &["worktree", "remove", "--force", root_str]).await;
        }
        if let Some(branch) = branch {
            let _ = git(base, &["branch", "-D", &branch]).await;
        }
        Ok(())
    }

    async fn snapshot(&self, root: &Path, label: &str) -> Result<Option<String>, WorkspaceError> {
        if !root.exists() {
            return Err(WorkspaceError::NotFound(root.to_path_buf()));
        }
        if git(root, &["rev-parse", "--verify", "--quiet", "HEAD"])
            .await
            .is_err()
        {
            return Ok(None);
        }

        let created = git(root, &["stash", "create", label]).await?;
        let commit = if created.is_empty() {
            git(root, &["rev-parse", "HEAD"]).await?
        } else {
            created
        };

        let shadow = format!("refs/{PRODUCT_SLUG}/snapshots/{commit}");
        git(root, &["update-ref", &shadow, &commit]).await?;

        tracing::debug!(
            target: "workspace",
            snapshot = %commit,
            label = %label,
            "captured workspace snapshot"
        );
        Ok(Some(commit))
    }

    async fn restore(&self, root: &Path, snapshot_id: &str) -> Result<(), WorkspaceError> {
        if !root.exists() {
            return Err(WorkspaceError::NotFound(root.to_path_buf()));
        }
        let commitish = format!("{snapshot_id}^{{commit}}");
        if git(root, &["rev-parse", "--verify", "--quiet", &commitish])
            .await
            .is_err()
        {
            return Err(WorkspaceError::NotFound(root.join(snapshot_id)));
        }

        git(root, &["read-tree", "-u", "--reset", snapshot_id]).await?;

        tracing::info!(
            target: "workspace",
            snapshot = %snapshot_id,
            "restored workspace snapshot"
        );
        Ok(())
    }
}

/// Run a verify command via `sh -c` in `root`. Returns `Ok(None)` on success,
/// `Ok(Some(tail))` with a truncated output tail on failure.
async fn run_verify(root: &Path, cmd: &str) -> Result<Option<String>, WorkspaceError> {
    let mut command = Command::new("sh");
    hide_console(&mut command);
    let output = command
        .arg("-c")
        .arg(cmd)
        .current_dir(root)
        .output()
        .await
        .map_err(|err| WorkspaceError::Io(format!("failed to run verify command: {err}")))?;
    if output.status.success() {
        return Ok(None);
    }
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(Some(tail(&combined, 800)))
}

/// The last `max` chars of `text`, prefixed with an elision marker when cut.
fn tail(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_owned();
    }
    let start = trimmed.chars().count() - max;
    let tail: String = trimmed.chars().skip(start).collect();
    format!("…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn init_repo(dir: &Path) {
        git(dir, &["init", "-q"]).await.expect("git init");
        git(dir, &["config", "user.email", "test@example.com"])
            .await
            .expect("config email");
        git(dir, &["config", "user.name", "Test"])
            .await
            .expect("config name");
        tokio::fs::write(dir.join("seed.txt"), "seed\n")
            .await
            .expect("write seed");
        git(dir, &["add", "-A"]).await.expect("add seed");
        git(dir, &["commit", "-q", "-m", "seed"])
            .await
            .expect("commit seed");
    }

    fn sess() -> SessionId {
        SessionId::from("test-session")
    }

    #[tokio::test]
    async fn non_repo_declines_or_errors_by_policy() {
        let base = tempfile::tempdir().expect("tempdir");
        let wt = tempfile::tempdir().expect("tempdir");
        let backend = GitWorktrees::new(wt.path());

        let optional = backend
            .provision(base.path(), &sess(), IsolationPolicy::Optional)
            .await
            .expect("optional never errors on a non-repo");
        assert!(optional.is_none(), "optional falls back on a non-repo");

        let required = backend
            .provision(base.path(), &sess(), IsolationPolicy::Required)
            .await;
        assert!(
            matches!(required, Err(WorkspaceError::NotAGitRepo(_))),
            "required errors on a non-repo: {required:?}"
        );
    }

    #[tokio::test]
    async fn provision_edit_integrate_merges_back() {
        let base = tempfile::tempdir().expect("tempdir");
        let wt = tempfile::tempdir().expect("tempdir");
        init_repo(base.path()).await;
        let backend = GitWorktrees::new(wt.path());

        let ws = backend
            .provision(base.path(), &sess(), IsolationPolicy::Required)
            .await
            .expect("provision ok")
            .expect("some workspace");
        assert!(ws.root.exists(), "worktree dir created");

        tokio::fs::write(ws.root.join("new.txt"), "hello\n")
            .await
            .expect("write in worktree");
        assert!(
            !base.path().join("new.txt").exists(),
            "base tree untouched before integrate"
        );

        let status = backend.status(&ws.root).await.expect("status");
        assert_eq!(status.files_changed, 1);

        let outcome = backend
            .integrate(&ws.root, base.path(), None)
            .await
            .expect("integrate ok");
        assert!(
            matches!(outcome, IntegrationOutcome::Merged { files_changed: 1 }),
            "merged one file: {outcome:?}"
        );
        assert_eq!(
            tokio::fs::read_to_string(base.path().join("new.txt"))
                .await
                .expect("file merged into base"),
            "hello\n"
        );
        assert!(!ws.root.exists(), "worktree removed after merge");
    }

    #[tokio::test]
    async fn committed_but_clean_work_is_merged_not_discarded() {
        let base = tempfile::tempdir().expect("tempdir");
        let wt = tempfile::tempdir().expect("tempdir");
        init_repo(base.path()).await;
        let backend = GitWorktrees::new(wt.path());

        let ws = backend
            .provision(base.path(), &sess(), IsolationPolicy::Required)
            .await
            .expect("provision")
            .expect("some");
        tokio::fs::write(ws.root.join("committed.txt"), "agent work\n")
            .await
            .expect("write");
        git(&ws.root, &["add", "-A"]).await.expect("add");
        git(&ws.root, &["commit", "-q", "-m", "agent commit"])
            .await
            .expect("commit");
        assert!(
            git(&ws.root, &["status", "--porcelain"])
                .await
                .expect("status")
                .is_empty(),
            "tree is clean after the agent's own commit"
        );

        let outcome = backend
            .integrate(&ws.root, base.path(), None)
            .await
            .expect("integrate");
        assert!(
            matches!(outcome, IntegrationOutcome::Merged { .. }),
            "committed-but-clean work merges, not discarded: {outcome:?}"
        );
        assert_eq!(
            tokio::fs::read_to_string(base.path().join("committed.txt"))
                .await
                .expect("merged into base"),
            "agent work\n"
        );
    }

    #[tokio::test]
    async fn verify_failure_keeps_workspace() {
        let base = tempfile::tempdir().expect("tempdir");
        let wt = tempfile::tempdir().expect("tempdir");
        init_repo(base.path()).await;
        let backend = GitWorktrees::new(wt.path());

        let ws = backend
            .provision(base.path(), &sess(), IsolationPolicy::Required)
            .await
            .expect("provision")
            .expect("some");
        tokio::fs::write(ws.root.join("new.txt"), "hi\n")
            .await
            .expect("write");

        let outcome = backend
            .integrate(&ws.root, base.path(), Some("exit 1"))
            .await
            .expect("integrate returns");
        assert!(
            matches!(outcome, IntegrationOutcome::VerifyFailed { .. }),
            "verify failure reported: {outcome:?}"
        );
        assert!(
            ws.root.exists(),
            "workspace kept for review after verify fail"
        );
        assert!(
            !base.path().join("new.txt").exists(),
            "nothing merged into base on verify fail"
        );
    }

    #[tokio::test]
    async fn discard_leaves_base_untouched_and_is_idempotent() {
        let base = tempfile::tempdir().expect("tempdir");
        let wt = tempfile::tempdir().expect("tempdir");
        init_repo(base.path()).await;
        let backend = GitWorktrees::new(wt.path());

        let ws = backend
            .provision(base.path(), &sess(), IsolationPolicy::Required)
            .await
            .expect("provision")
            .expect("some");
        tokio::fs::write(ws.root.join("scratch.txt"), "x\n")
            .await
            .expect("write");

        backend
            .discard(&ws.root, base.path())
            .await
            .expect("discard");
        assert!(!ws.root.exists(), "worktree removed on discard");
        assert!(!base.path().join("scratch.txt").exists(), "base untouched");
        backend
            .discard(&ws.root, base.path())
            .await
            .expect("second discard is a no-op");
    }

    #[tokio::test]
    async fn snapshot_is_none_on_a_non_repo() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = GitWorktrees::new(dir.path());
        let snap = backend
            .snapshot(dir.path(), "turn-1")
            .await
            .expect("snapshot never errors on a non-repo");
        assert!(snap.is_none(), "no snapshot without a git repo");
    }

    #[tokio::test]
    async fn snapshot_then_restore_rewinds_tracked_edits() {
        let base = tempfile::tempdir().expect("tempdir");
        init_repo(base.path()).await;
        let backend = GitWorktrees::new(base.path());
        let root = base.path();

        let snap = backend
            .snapshot(root, "turn-1")
            .await
            .expect("snapshot ok")
            .expect("git repo → some snapshot");

        tokio::fs::write(root.join("seed.txt"), "corrupted\n")
            .await
            .expect("edit seed");
        tokio::fs::write(root.join("added.txt"), "new\n")
            .await
            .expect("write added");
        git(root, &["add", "-A"]).await.expect("stage");

        backend.restore(root, &snap).await.expect("restore ok");

        let seed = tokio::fs::read_to_string(root.join("seed.txt"))
            .await
            .expect("read seed");
        assert_eq!(seed, "seed\n", "tracked edit rewound");
        assert!(
            !root.join("added.txt").exists(),
            "file tracked after the snapshot is dropped on restore"
        );
        let head = git(root, &["rev-parse", "--abbrev-ref", "HEAD"])
            .await
            .expect("head");
        assert!(!head.is_empty(), "still on a branch after restore");
    }

    #[tokio::test]
    async fn snapshot_captures_uncommitted_working_tree() {
        let base = tempfile::tempdir().expect("tempdir");
        init_repo(base.path()).await;
        let backend = GitWorktrees::new(base.path());
        let root = base.path();

        tokio::fs::write(root.join("seed.txt"), "work in progress\n")
            .await
            .expect("edit");
        let snap = backend
            .snapshot(root, "turn-2")
            .await
            .expect("snapshot ok")
            .expect("some");

        tokio::fs::write(root.join("seed.txt"), "later change\n")
            .await
            .expect("edit again");
        backend.restore(root, &snap).await.expect("restore");

        let seed = tokio::fs::read_to_string(root.join("seed.txt"))
            .await
            .expect("read");
        assert_eq!(seed, "work in progress\n", "uncommitted state restored");
    }

    #[tokio::test]
    async fn restore_rejects_unknown_snapshot() {
        let base = tempfile::tempdir().expect("tempdir");
        init_repo(base.path()).await;
        let backend = GitWorktrees::new(base.path());
        let err = backend
            .restore(base.path(), "0000000000000000000000000000000000000000")
            .await;
        assert!(
            matches!(err, Err(WorkspaceError::NotFound(_))),
            "unknown snapshot id → NotFound: {err:?}"
        );
    }
}
