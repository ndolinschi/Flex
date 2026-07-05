//! Copy text to the system clipboard via platform CLI tools.

use std::io::{self, Write};
use std::process::{Command, Stdio};

/// Copy `text` to the system clipboard.
pub fn copy_text(text: &str) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        copy_via_stdin("pbcopy", &[], text)
    }
    #[cfg(target_os = "linux")]
    {
        if copy_via_stdin("wl-copy", &[], text).is_ok() {
            Ok(())
        } else {
            copy_via_stdin("xclip", &["-selection", "clipboard"], text)
        }
    }
    #[cfg(target_os = "windows")]
    {
        copy_via_stdin("clip", &[], text)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = text;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "clipboard copy is not supported on this platform",
        ))
    }
}

fn copy_via_stdin(program: &str, args: &[&str], text: &str) -> io::Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|err| io::Error::new(err.kind(), format!("failed to spawn `{program}`: {err}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "`{program}` exited with {status}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_roundtrip_on_supported_platforms() {
        if std::env::var("AGENTLOOP_CLIPBOARD_TEST").is_err() {
            return;
        }
        copy_text("agentloop clipboard test").expect("clipboard copy succeeds");
    }
}
