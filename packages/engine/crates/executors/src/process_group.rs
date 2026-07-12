//! Process-group spawn/kill helpers so `LocalExecutor` can terminate an
//! entire subtree (a shell plus whatever it forked — pipeline stages, a
//! backgrounded dev server started with `&`, a build tool's workers) rather
//! than just the immediate `/bin/sh` child.
//!
//! Without this, `Child::start_kill`/dropping the `Child` only signals the
//! shell itself: any grandchildren it spawned are reparented to init and keep
//! running as orphans — silently leaking `sleep`s, dev servers, and anything
//! else a demoted or killed background command started.

#[cfg(unix)]
pub(crate) fn configure(command: &mut tokio::process::Command) {
    // pgid 0 means "become the leader of a new group with pgid == my own
    // pid" — distinct from every other process group on the system
    // (including ours), so signaling it can never hit an unrelated process.
    // `tokio::process::Command::process_group` (unix-only) is the async
    // runtime's own re-export of `std::os::unix::process::CommandExt`.
    command.process_group(0);
}

#[cfg(not(unix))]
pub(crate) fn configure(_command: &mut tokio::process::Command) {
    // Windows has no equivalent of POSIX process groups on `Command`; a
    // `cmd /C` child's own subprocesses are killed via `taskkill /T` where it
    // matters (see `kill_group` below). Nothing to configure at spawn time.
}

/// Best-effort SIGKILL to the whole process group `pid` leads (see
/// [`configure`]). Swallows errors: the group may already be gone (process
/// exited between the caller's `running` check and this call), which is not
/// a failure — there's nothing left to kill.
#[cfg(unix)]
pub(crate) fn kill_group(pid: u32) {
    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::Pid;
    let _ = killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
}

/// Windows fallback: `taskkill /T` walks and kills the process tree rooted at
/// `pid` (there is no process-group primitive to target directly). Spawned
/// synchronously and best-effort — same "already gone is fine" contract as
/// the unix path.
#[cfg(not(unix))]
pub(crate) fn kill_group(pid: u32) {
    let _ = std::process::Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .output();
}

#[cfg(all(test, unix))]
// Clippy's `expect_used`/`unwrap_used` lints special-case `#[test]`-attributed
// functions but not plain helper fns called only from them — `expect` in
// `spawn_shell_with_grandchild`/`wait_until_dead` below is exactly that same
// "setup failed, fail the test loudly" case, just factored out to avoid
// repeating the shell-spawn boilerplate across three tests.
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::process::Stdio;

    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::{Child, Command};

    use super::*;

    /// Spawns `/bin/sh` running `sleep 30 & echo $!; wait` — the shell
    /// backgrounds a `sleep` (its own grandchild relative to the caller) and
    /// prints the grandchild's pid, then itself waits — simulating a
    /// dev-server-style script. Returns the spawned shell `Child` plus the
    /// grandchild's pid (read off its stdout).
    async fn spawn_shell_with_grandchild() -> (Child, u32) {
        let mut command = Command::new("/bin/sh");
        command
            .arg("-lc")
            .arg("sleep 30 & echo $!; wait")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .kill_on_drop(true);
        configure(&mut command);

        let mut child = command.spawn().expect("spawn ok");

        let stdout = child.stdout.take().expect("stdout piped");
        let mut lines = BufReader::new(stdout).lines();
        let grandchild_pid: u32 = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            lines
                .next_line()
                .await
                .expect("read line ok")
                .expect("grandchild pid line")
        })
        .await
        .expect("shell prints the backgrounded pid promptly")
        .trim()
        .parse()
        .expect("pid parses as a number");

        (child, grandchild_pid)
    }

    /// Polls briefly rather than sleeping a fixed guess: signal delivery and
    /// reaping are asynchronous. Returns whether the process was observed
    /// dead within the poll window.
    async fn wait_until_dead(pid: u32) -> bool {
        for _ in 0..50 {
            if !process_is_alive(pid) {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        false
    }

    #[tokio::test]
    async fn kill_group_terminates_a_grandchild_process() {
        // Killing only the shell's pid would leave the backgrounded `sleep`
        // running; `kill_group` must take both down.
        let (mut child, grandchild_pid) = spawn_shell_with_grandchild().await;
        let shell_pid = child.id().expect("pid");

        // Sanity: the grandchild is alive before we kill anything.
        assert!(
            process_is_alive(grandchild_pid),
            "grandchild should be running before kill_group"
        );

        kill_group(shell_pid);

        assert!(
            wait_until_dead(grandchild_pid).await,
            "killing the process group must also kill the grandchild sleep, \
             not just the shell"
        );

        let _ = child.kill().await;
    }

    #[tokio::test]
    async fn kill_group_is_idempotent_on_double_kill() {
        // A second `kill_group` call against a pgid that's already dead must
        // not panic or error — `killpg` on an already-gone group is exactly
        // the "process exited between the caller's running check and this
        // call" race the doc comment says is swallowed, not a failure.
        let (mut child, grandchild_pid) = spawn_shell_with_grandchild().await;
        let shell_pid = child.id().expect("pid");

        kill_group(shell_pid);
        assert!(
            wait_until_dead(grandchild_pid).await,
            "first kill_group call must take down the grandchild"
        );

        // Second call against the same (now-dead) pgid — must be a silent
        // no-op, not a panic.
        kill_group(shell_pid);
        assert!(
            !process_is_alive(grandchild_pid),
            "grandchild must still be gone after the redundant second kill_group call"
        );

        let _ = child.kill().await;
    }

    #[tokio::test]
    async fn kill_group_of_already_exited_group_is_a_no_op() {
        // No grandchild this time: a plain `/bin/sh -lc true` process that
        // exits immediately on its own, well before we ever call
        // `kill_group`. The pgid is gone by the time we signal it — this
        // must not panic (the whole point of `kill_group` swallowing
        // `killpg`'s error return).
        let mut command = Command::new("/bin/sh");
        command
            .arg("-lc")
            .arg("true")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .kill_on_drop(true);
        configure(&mut command);

        let mut child = command.spawn().expect("spawn ok");
        let shell_pid = child.id().expect("pid");

        let status = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait())
            .await
            .expect("shell exits promptly")
            .expect("wait ok");
        assert!(status.success(), "the `true` shell command must exit 0");

        // The process (and its solo-member group) is already gone; this
        // must return without panicking.
        kill_group(shell_pid);
    }

    /// `kill(pid, 0)` sends no signal; it only checks whether the process
    /// (or a zombie awaiting reap) still exists — the standard portable way
    /// to probe liveness without a dependency beyond what this module
    /// already pulls in.
    fn process_is_alive(pid: u32) -> bool {
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_ok()
    }
}
