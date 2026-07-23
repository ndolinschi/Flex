use std::process::Stdio;

use tokio::process::Command;

pub(crate) async fn probe_binary(binary: &str, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new(binary);
    crate::win_console::hide_console(&mut command);
    let output = command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|err| format!("`{binary}` is not runnable: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "`{binary} {}` exited non-zero: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let first_line = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    Ok(first_line)
}
