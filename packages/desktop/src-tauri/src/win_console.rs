//! Hide console windows for child processes spawned from a GUI parent on Windows.
//!
//! The desktop binary uses `windows_subsystem = "windows"` in release builds, so:
//!
//! 1. Every console child (`git`, `gh`, `cmd`, …) would flash a conhost window
//!    without [`CREATE_NO_WINDOW`] — see [`hide_console`] / [`command`].
//! 2. ConPTY (portable-pty terminal panel) is different: `CREATE_NO_WINDOW` is
//!    incompatible with `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE` and breaks pipe I/O.
//!    A GUI parent with *no* console instead makes Windows allocate a **visible**
//!    fallback console per ConPTY child (PowerShell popup). The fix is
//!    [`ensure_hidden_parent_console`]: `AllocConsole` + `SW_HIDE` once at
//!    startup so ConPTY children inherit a hidden console — the same pattern
//!    node-pty / VS Code use. No-op on non-Windows and in debug builds (those
//!    already inherit the developer terminal).

/// Win32 `CREATE_NO_WINDOW` — do not allocate a new console for the child.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Apply creation flags so a console child does not flash a window.
///
/// Use for ordinary `std::process::Command` spawns (`git`, `gh`, …). Do **not**
/// apply this to ConPTY / portable-pty spawns.
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

/// Allocate a hidden console for this GUI-subsystem process so ConPTY children
/// do not pop a visible PowerShell/conhost window.
///
/// Safe to call more than once. Skipped in debug builds: `cargo run` /
/// `tauri dev` already attach a console, and `AllocConsole`+`SW_HIDE` would
/// hide (or fight) the developer's terminal.
pub fn ensure_hidden_parent_console() {
    #[cfg(all(windows, not(debug_assertions)))]
    {
        // kernel32 / user32 — no `windows-sys` dep required for these two calls.
        #[link(name = "kernel32")]
        extern "system" {
            fn AllocConsole() -> i32;
            fn GetConsoleWindow() -> *mut core::ffi::c_void;
        }
        #[link(name = "user32")]
        extern "system" {
            fn ShowWindow(hwnd: *mut core::ffi::c_void, n_cmd_show: i32) -> i32;
        }
        const SW_HIDE: i32 = 0;

        // SAFETY: AllocConsole / GetConsoleWindow / ShowWindow are the standard
        // Win32 startup sequence for a hidden parent console; hwnd is only used
        // when non-null, and SW_HIDE does not free or mutate other process state.
        //
        // Only allocate+hide when this process has *no* console yet. If one is
        // already attached (debugger, unusual launcher), AllocConsole fails and
        // hiding GetConsoleWindow() would conceal that existing console — so
        // leave it alone.
        unsafe {
            if GetConsoleWindow().is_null() && AllocConsole() != 0 {
                let hwnd = GetConsoleWindow();
                if !hwnd.is_null() {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hide_console_is_callable_on_any_host() {
        // Smoke: must compile and run as a no-op off Windows.
        let mut cmd = std::process::Command::new("true");
        hide_console(&mut cmd);
    }

    #[test]
    fn ensure_hidden_parent_console_is_callable() {
        // No-op off Windows / in debug; must not panic in release Windows CI.
        ensure_hidden_parent_console();
    }
}
