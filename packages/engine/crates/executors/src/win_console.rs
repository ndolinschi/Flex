//! Hide console windows for child processes spawned from a GUI parent on Windows.
//!
//! A GUI-subsystem parent (the desktop app uses `windows_subsystem = "windows"`)
//! that spawns a console-subsystem child (`cmd`, `git`, `docker`, `taskkill`, …)
//! without [`CREATE_NO_WINDOW`] gets a transient conhost window — the classic
//! flash-and-close. Unix has no equivalent allocation model; these helpers are
//! no-ops there.

/// Win32 `CREATE_NO_WINDOW` — do not allocate a new console for the child.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Apply creation flags so a console child does not flash a window.
pub(crate) fn hide_console(command: &mut tokio::process::Command) {
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    #[cfg(not(windows))]
    let _ = command;
}

/// Same as [`hide_console`] for synchronous [`std::process::Command`].
/// Only compiled off-unix (where `taskkill` and similar console children run).
#[cfg(not(unix))]
pub(crate) fn hide_console_std(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}
