
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl ScreenRect {
    pub fn from_physical(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self {
            x: x.round() as i32,
            y: y.round() as i32,
            width: w.max(1.0).round() as u32,
            height: h.max(1.0).round() as u32,
        }
    }
}

pub fn temp_png(prefix: &str) -> PathBuf {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}-{ms}.png"))
}

#[cfg(any(target_os = "macos", windows))]
fn path_str(out: &Path) -> Result<&str, String> {
    out.to_str()
        .ok_or_else(|| "screenshot path is not valid UTF-8".to_owned())
}

pub fn capture_primary_to_png(out: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("screencapture")
            .args(["-x", path_str(out)?])
            .status()
            .map_err(|e| format!("screencapture failed: {e}"))?;
        if status.success() {
            return Ok(());
        }
        Err(format!("screencapture exited with {status}"))
    }
    #[cfg(target_os = "linux")]
    {
        if Command::new("grim")
            .arg(out)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Ok(());
        }
        let status = Command::new("import")
            .args(["-window", "root"])
            .arg(out)
            .status()
            .map_err(|e| format!("screen capture failed (install grim or ImageMagick): {e}"))?;
        if status.success() {
            return Ok(());
        }
        Err("screen capture failed — install grim (Wayland) or ImageMagick".into())
    }
    #[cfg(windows)]
    {
        windows_capture_primary(out)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = out;
        Err("screen capture is not supported on this platform".into())
    }
}

pub fn capture_region_to_png(rect: ScreenRect, scale: f64, out: &Path) -> Result<(), String> {
    if rect.width == 0 || rect.height == 0 {
        return Err("capture region has zero size".into());
    }
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };

    #[cfg(target_os = "macos")]
    {
        let x = rect.x as f64 / scale;
        let y = rect.y as f64 / scale;
        let w = rect.width as f64 / scale;
        let h = rect.height as f64 / scale;
        let status = Command::new("screencapture")
            .arg("-x")
            .arg("-R")
            .arg(format!("{x},{y},{w},{h}"))
            .arg(out)
            .status()
            .map_err(|e| format!("screencapture failed: {e}"))?;
        if status.success() {
            return Ok(());
        }
        Err(format!("screencapture exited with {status}"))
    }
    #[cfg(target_os = "linux")]
    {
        let _ = scale;
        let geom = format!("{},{} {}x{}", rect.x, rect.y, rect.width, rect.height);
        if Command::new("grim")
            .args(["-g", &geom])
            .arg(out)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Ok(());
        }
        let status = Command::new("import")
            .args([
                "-window",
                "root",
                "-crop",
                &format!("{}x{}+{}+{}", rect.width, rect.height, rect.x, rect.y),
            ])
            .arg(out)
            .status()
            .map_err(|e| format!("import failed: {e}"))?;
        if status.success() {
            return Ok(());
        }
        Err("region capture failed — install grim (Wayland) or ImageMagick".into())
    }
    #[cfg(windows)]
    {
        let _ = scale;
        windows_capture_region(rect, out)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = (rect, scale, out);
        Err("region capture is not supported on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    use super::ScreenRect;

    #[test]
    fn from_physical_rounds_and_clamps_size() {
        let r = ScreenRect::from_physical(10.4, 20.6, 0.0, -1.0);
        assert_eq!(r.x, 10);
        assert_eq!(r.y, 21);
        assert_eq!(r.width, 1);
        assert_eq!(r.height, 1);
    }
}

#[cfg(windows)]
fn windows_escape_ps_string(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(windows)]
fn windows_capture_primary(out: &Path) -> Result<(), String> {
    let path = windows_escape_ps_string(path_str(out)?);
    let script = format!(
        r#"
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bmp = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
$bmp.Save('{path}')
$g.Dispose(); $bmp.Dispose()
"#
    );
    run_powershell(&script)
}

#[cfg(windows)]
fn windows_capture_region(rect: ScreenRect, out: &Path) -> Result<(), String> {
    let path = windows_escape_ps_string(path_str(out)?);
    let script = format!(
        r#"
Add-Type -AssemblyName System.Drawing
$bmp = New-Object System.Drawing.Bitmap {w}, {h}
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen({x}, {y}, 0, 0, (New-Object System.Drawing.Size({w}, {h})))
$bmp.Save('{path}')
$g.Dispose(); $bmp.Dispose()
"#,
        x = rect.x,
        y = rect.y,
        w = rect.width,
        h = rect.height,
    );
    run_powershell(&script)
}

#[cfg(windows)]
fn run_powershell(script: &str) -> Result<(), String> {
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .status()
        .map_err(|e| format!("powershell failed: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("powershell exited with {status}"))
    }
}

#[cfg(windows)]
pub mod windows_input {
    use super::{run_powershell, windows_escape_ps_string};
    use std::process::Command;

    const INPUT_TYPEDEF: &str = r#"
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class AgentInput {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);
}
"@
"#;

    pub fn move_mouse(x: f64, y: f64) -> Result<(), String> {
        let script = format!(
            "{INPUT_TYPEDEF}\n[AgentInput]::SetCursorPos({x}, {y}) | Out-Null\n",
            x = x.round() as i32,
            y = y.round() as i32,
        );
        run_powershell(&script)
    }

    pub fn click(x: f64, y: f64, button: &str) -> Result<(), String> {
        move_mouse(x, y)?;
        let (down, up) = if button == "right" {
            (0x0008u32, 0x0010u32)
        } else {
            (0x0002u32, 0x0004u32)
        };
        let script = format!(
            "{INPUT_TYPEDEF}\n[AgentInput]::mouse_event({down},0,0,0,[UIntPtr]::Zero)\n[AgentInput]::mouse_event({up},0,0,0,[UIntPtr]::Zero)\n"
        );
        run_powershell(&script)
    }

    pub fn type_text(text: &str) -> Result<(), String> {
        let mut escaped = String::with_capacity(text.len());
        for ch in text.chars() {
            match ch {
                '+' | '^' | '%' | '~' | '(' | ')' | '{' | '}' | '[' | ']' => {
                    escaped.push('{');
                    escaped.push(ch);
                    escaped.push('}');
                }
                '\n' => escaped.push_str("{ENTER}"),
                '\t' => escaped.push_str("{TAB}"),
                _ => escaped.push(ch),
            }
        }
        let escaped = windows_escape_ps_string(&escaped);
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms\n[System.Windows.Forms.SendKeys]::SendWait('{escaped}')\n"
        );
        run_powershell(&script)
    }

    pub fn open_app(name: &str) -> Result<(), String> {
        let status = Command::new("cmd")
            .args(["/C", "start", "", name])
            .status()
            .map_err(|e| format!("start failed: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("Could not open application `{name}`"))
        }
    }
}
