//! Terminal panel commands — PTY-backed shells surfaced in the right panel.
//!
//! Invariant: `state.terminals` is a std (blocking) `Mutex`, not a tokio one,
//! because PTY I/O (writer writes, resize, kill) is synchronous. Every
//! command below locks the map, performs its blocking call, and drops the
//! guard before returning — no `.await` ever happens while the guard is
//! held, so blocking this mutex briefly is safe inside an async fn.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::error::{DesktopError, DesktopResult};
use crate::state::{AppState, TerminalHandle};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub id: String,
    pub cwd: String,
    pub created_at_ms: u64,
    /// Present when the requested session cwd was missing and the PTY fell
    /// back to the home directory — UI can toast without guessing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd_fallback_from: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputEvent {
    pub id: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExitEvent {
    pub id: String,
    pub exit_code: Option<i32>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Per-OS home directory for `terminal_create` when no session cwd is given.
///
/// Uses [`dirs::home_dir`] (USERPROFILE on Windows, $HOME elsewhere) rather
/// than reading `$HOME` alone — Windows often has no `HOME`, and the previous
/// fallback of `/` is not a usable cwd (CreateProcess / PowerShell path errors
/// that surface with doubled `\\` in the message).
fn default_cwd() -> String {
    if let Some(home) = dirs::home_dir() {
        return home.to_string_lossy().into_owned();
    }
    #[cfg(windows)]
    {
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            return profile.to_string_lossy().into_owned();
        }
        return std::env::var_os("SystemDrive")
            .map(|d| format!("{}\\", d.to_string_lossy()))
            .unwrap_or_else(|| "C:\\".to_owned());
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME")
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/".to_owned())
    }
}

/// Pick an existing directory for the PTY shell. Prefers the requested cwd,
/// then a collapsed-backslash variant (Windows double-escape), then home.
/// Returns `(resolved_path, fallback_from)` — `fallback_from` is set when the
/// requested path was unusable.
fn resolve_cwd(cwd: Option<String>) -> DesktopResult<(String, Option<String>)> {
    let raw = cwd.unwrap_or_else(default_cwd);
    if Path::new(&raw).is_dir() {
        return Ok((raw, None));
    }

    let collapsed = crate::path_resolve::collapse_extra_backslashes(&raw);
    if collapsed != raw && Path::new(&collapsed).is_dir() {
        tracing::warn!(
            requested = %raw,
            collapsed = %collapsed,
            "terminal cwd had doubled backslashes; using collapsed path"
        );
        return Ok((collapsed, None));
    }

    let fallback = default_cwd();
    if Path::new(&fallback).is_dir() {
        tracing::warn!(
            requested = %raw,
            fallback = %fallback,
            "terminal cwd is missing; using default home directory"
        );
        return Ok((fallback, Some(raw)));
    }

    Err(DesktopError::Message(format!(
        "terminal cwd is not a directory: {raw}"
    )))
}

/// Per-OS default shell binary for `terminal_create`, mirroring what each
/// platform's own terminal apps default to: `$SHELL` (falling back to
/// `/bin/zsh`, macOS's default login shell since Catalina) on macOS,
/// `$SHELL` (falling back to `/bin/bash`) on Linux, and `powershell.exe` on
/// Windows (`$SHELL` isn't a Windows convention). `portable-pty`'s
/// `CommandBuilder::new_default_prog()` already does something close to this
/// internally (`$SHELL`/`COMSPEC`), but doesn't expose the exact fallback
/// chain the product wants here, so it's spelled out explicitly.
#[cfg(target_os = "macos")]
fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_owned())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_owned())
}

#[cfg(windows)]
fn default_shell() -> String {
    "powershell.exe".to_owned()
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn terminal_create(
    app: AppHandle,
    state: State<'_, AppState>,
    cwd: Option<String>,
) -> DesktopResult<TerminalInfo> {
    let (cwd, cwd_fallback_from) = resolve_cwd(cwd)?;
    let cwd_path = PathBuf::from(&cwd);

    let id = {
        let mut seq = state
            .next_terminal_seq
            .lock()
            .map_err(|_| DesktopError::Message("terminal sequence lock poisoned".into()))?;
        *seq += 1;
        format!("term-{}", *seq)
    };

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| DesktopError::Message(format!("failed to open pty: {e}")))?;

    let mut cmd = CommandBuilder::new(default_shell());
    cmd.cwd(&cwd_path);
    cmd.env("TERM", "xterm-256color");

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| DesktopError::Message(format!("failed to spawn shell: {e}")))?;

    let writer = pair
        .master
        .take_writer()
        .map_err(|e| DesktopError::Message(format!("failed to open pty writer: {e}")))?;
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| DesktopError::Message(format!("failed to open pty reader: {e}")))?;

    let created_at_ms = now_ms();
    let handle = TerminalHandle {
        writer,
        master: pair.master,
        child,
        cwd: cwd.clone(),
        created_at_ms,
    };

    {
        let mut terminals = state
            .terminals
            .lock()
            .map_err(|_| DesktopError::Message("terminal registry lock poisoned".into()))?;
        terminals.insert(id.clone(), handle);
    }

    let reader_id = id.clone();
    let reader_app = app.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).into_owned();
                    if reader_app
                        .emit(
                            "terminal-output",
                            &TerminalOutputEvent {
                                id: reader_id.clone(),
                                data,
                            },
                        )
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = reader_app.emit(
            "terminal-exit",
            &TerminalExitEvent {
                id: reader_id.clone(),
                exit_code: None,
            },
        );
    });

    Ok(TerminalInfo {
        id,
        cwd,
        created_at_ms,
        cwd_fallback_from,
    })
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn terminal_write(
    state: State<'_, AppState>,
    id: String,
    data: String,
) -> DesktopResult<()> {
    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| DesktopError::Message("terminal registry lock poisoned".into()))?;
    let handle = terminals
        .get_mut(&id)
        .ok_or_else(|| DesktopError::Message(format!("unknown terminal id: {id}")))?;
    handle
        .writer
        .write_all(data.as_bytes())
        .map_err(|e| DesktopError::Message(format!("failed to write to terminal: {e}")))?;
    handle
        .writer
        .flush()
        .map_err(|e| DesktopError::Message(format!("failed to flush terminal: {e}")))?;
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn terminal_resize(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
) -> DesktopResult<()> {
    let terminals = state
        .terminals
        .lock()
        .map_err(|_| DesktopError::Message("terminal registry lock poisoned".into()))?;
    let handle = terminals
        .get(&id)
        .ok_or_else(|| DesktopError::Message(format!("unknown terminal id: {id}")))?;
    handle
        .master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| DesktopError::Message(format!("failed to resize terminal: {e}")))?;
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn terminal_kill(state: State<'_, AppState>, id: String) -> DesktopResult<()> {
    let mut handle = {
        let mut terminals = state
            .terminals
            .lock()
            .map_err(|_| DesktopError::Message("terminal registry lock poisoned".into()))?;
        terminals.remove(&id)
    };
    if let Some(handle) = handle.as_mut() {
        let _ = handle.child.kill();
    }
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn terminal_list(state: State<'_, AppState>) -> DesktopResult<Vec<TerminalInfo>> {
    let terminals = state
        .terminals
        .lock()
        .map_err(|_| DesktopError::Message("terminal registry lock poisoned".into()))?;
    let mut list: Vec<TerminalInfo> = terminals
        .iter()
        .map(|(id, handle)| TerminalInfo {
            id: id.clone(),
            cwd: handle.cwd.clone(),
            created_at_ms: handle.created_at_ms,
            cwd_fallback_from: None,
        })
        .collect();
    list.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(list)
}

/// Kill every live terminal — called on app window close so shells don't
/// linger as orphans after the desktop app quits.
pub fn kill_all_terminals(state: &AppState) {
    let Ok(mut terminals) = state.terminals.lock() else {
        return;
    };
    for (_, mut handle) in terminals.drain() {
        let _ = handle.child.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cwd_is_non_empty() {
        assert!(!default_cwd().is_empty());
    }

    #[test]
    fn resolve_cwd_accepts_existing_dir() {
        let tmp = std::env::temp_dir();
        let resolved = resolve_cwd(Some(tmp.to_string_lossy().into_owned())).expect("tmp exists");
        assert!(Path::new(&resolved).is_dir());
    }
}
