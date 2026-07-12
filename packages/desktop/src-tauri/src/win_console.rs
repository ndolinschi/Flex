//! Hide console windows for child processes spawned from a GUI parent on Windows.
//!
//! The desktop binary uses `windows_subsystem = "windows"`, so every console
//! child (`git`, `gh`, `cmd`, …) would flash a conhost window without
//! `CREATE_NO_WINDOW`. No-op on non-Windows.

/// Win32 `CREATE_NO_WINDOW` — do not allocate a new console for the child.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Apply creation flags so a console child does not flash a window.
pub fn hide_console(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// Build a [`std::process::Command`] with console-hiding applied on Windows.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    hide_console(&mut cmd);
    cmd
}
