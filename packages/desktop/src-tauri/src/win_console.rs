
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn hide_console(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

pub fn command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    hide_console(&mut cmd);
    cmd
}

pub fn ensure_hidden_parent_console() {
    #[cfg(all(windows, not(debug_assertions)))]
    {
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
        let mut cmd = std::process::Command::new("true");
        hide_console(&mut cmd);
    }

    #[test]
    fn ensure_hidden_parent_console_is_callable() {
        ensure_hidden_parent_console();
    }
}
