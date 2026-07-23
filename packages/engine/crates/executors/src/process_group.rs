#[cfg(unix)]
pub(crate) fn configure(command: &mut tokio::process::Command) {
    command.process_group(0);
}

#[cfg(not(unix))]
pub(crate) fn configure(_command: &mut tokio::process::Command) {}

#[cfg(unix)]
pub(crate) fn kill_group(pid: u32) {
    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::Pid;
    let _ = killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
}

#[cfg(not(unix))]
pub(crate) fn kill_group(pid: u32) {
    let mut command = std::process::Command::new("taskkill");
    command.args(["/T", "/F", "/PID", &pid.to_string()]);
    crate::win_console::hide_console_std(&mut command);
    let _ = command.output();
}

#[cfg(all(test, unix))]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::process::Stdio;

    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::{Child, Command};

    use super::*;

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
        let (mut child, grandchild_pid) = spawn_shell_with_grandchild().await;
        let shell_pid = child.id().expect("pid");

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
        let (mut child, grandchild_pid) = spawn_shell_with_grandchild().await;
        let shell_pid = child.id().expect("pid");

        kill_group(shell_pid);
        assert!(
            wait_until_dead(grandchild_pid).await,
            "first kill_group call must take down the grandchild"
        );

        kill_group(shell_pid);
        assert!(
            !process_is_alive(grandchild_pid),
            "grandchild must still be gone after the redundant second kill_group call"
        );

        let _ = child.kill().await;
    }

    #[tokio::test]
    async fn kill_group_of_already_exited_group_is_a_no_op() {
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

        kill_group(shell_pid);
    }

    fn process_is_alive(pid: u32) -> bool {
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_ok()
    }
}
