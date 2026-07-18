//! Animated always-on-top agent cursor overlay (ChatGPT computer-use style).
//!
//! A transparent, click-through window hosts a single glowing pointer that
//! animates between screen coordinates while Computer tools drive the host.

use std::time::Duration;

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::error::{DesktopError, DesktopResult};

const WINDOW_LABEL: &str = "agent-cursor";

const OVERLAY_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8"/>
<style>
  html, body {
    margin: 0; width: 100%; height: 100%;
    background: transparent; overflow: hidden;
    pointer-events: none; user-select: none;
  }
  #cursor {
    position: fixed;
    left: 0; top: 0;
    width: 28px; height: 28px;
    margin-left: -4px; margin-top: -4px;
    border-radius: 50%;
    background: radial-gradient(circle at 35% 35%, #fff 0%, #6b9eff 45%, #3b6fd9 100%);
    box-shadow:
      0 0 0 2px rgba(255,255,255,0.85),
      0 0 18px 4px rgba(107,158,255,0.65),
      0 4px 12px rgba(0,0,0,0.35);
    transform: translate3d(-100px, -100px, 0) scale(1);
    transition: transform 380ms cubic-bezier(0.22, 1, 0.36, 1);
    opacity: 0;
  }
  #cursor.visible { opacity: 1; }
  #cursor.click {
    animation: pulse 280ms ease-out;
  }
  @keyframes pulse {
    0% { transform: translate3d(var(--x), var(--y), 0) scale(1); }
    40% { transform: translate3d(var(--x), var(--y), 0) scale(0.72); }
    100% { transform: translate3d(var(--x), var(--y), 0) scale(1); }
  }
  #ring {
    position: fixed;
    left: 0; top: 0;
    width: 44px; height: 44px;
    margin-left: -12px; margin-top: -12px;
    border-radius: 50%;
    border: 2px solid rgba(107,158,255,0.55);
    transform: translate3d(-100px, -100px, 0) scale(0.4);
    opacity: 0;
    pointer-events: none;
  }
  #ring.flash {
    animation: ring 420ms ease-out;
  }
  @keyframes ring {
    0% { opacity: 0.9; transform: translate3d(var(--x), var(--y), 0) scale(0.35); }
    100% { opacity: 0; transform: translate3d(var(--x), var(--y), 0) scale(1.35); }
  }
</style>
</head>
<body>
  <div id="ring"></div>
  <div id="cursor"></div>
  <script>
    const cursor = document.getElementById('cursor');
    const ring = document.getElementById('ring');
    let x = -100, y = -100;
    function apply(nx, ny, click) {
      x = nx; y = ny;
      const t = `translate3d(${x}px, ${y}px, 0)`;
      document.documentElement.style.setProperty('--x', x + 'px');
      document.documentElement.style.setProperty('--y', y + 'px');
      cursor.style.transform = t + ' scale(1)';
      ring.style.transform = t + ' scale(0.4)';
      cursor.classList.add('visible');
      if (click) {
        cursor.classList.remove('click');
        ring.classList.remove('flash');
        void cursor.offsetWidth;
        cursor.classList.add('click');
        ring.classList.add('flash');
      }
    }
    window.__agentCursorMove = (nx, ny, click) => apply(nx, ny, !!click);
    window.__agentCursorHide = () => {
      cursor.classList.remove('visible');
      cursor.style.transform = 'translate3d(-100px,-100px,0)';
    };
  </script>
</body>
</html>"#;

async fn ensure_overlay(app: &AppHandle) -> DesktopResult<()> {
    if app.get_webview_window(WINDOW_LABEL).is_some() {
        return Ok(());
    }
    let html_path = std::env::temp_dir().join("agent-cursor-overlay.html");
    std::fs::write(&html_path, OVERLAY_HTML)
        .map_err(|e| DesktopError::Message(format!("agent cursor overlay write: {e}")))?;
    let url = tauri::Url::from_file_path(&html_path)
        .map_err(|_| DesktopError::Message("agent cursor overlay: invalid file url".into()))?;
    let win = WebviewWindowBuilder::new(app, WINDOW_LABEL, WebviewUrl::CustomProtocol(url))
        .title("Agent cursor")
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .resizable(false)
        .inner_size(1.0, 1.0)
        .build()
        .map_err(|e| DesktopError::Message(format!("agent cursor overlay: {e}")))?;

    // Cover the primary monitor so screen coordinates map 1:1.
    if let Ok(Some(monitor)) = app.primary_monitor() {
        let size = monitor.size();
        let pos = monitor.position();
        let _ = win.set_position(tauri::PhysicalPosition::new(pos.x, pos.y));
        let _ = win.set_size(tauri::PhysicalSize::new(size.width, size.height));
        let _ = win.set_ignore_cursor_events(true);
    }
    let _ = win.show();
    // Give the HTML a beat to boot before the first eval.
    tokio::time::sleep(Duration::from_millis(60)).await;
    Ok(())
}

/// Show the agent cursor and animate it to `(x, y)` in screen points.
pub async fn show_agent_cursor(app: &AppHandle, x: f64, y: f64) -> DesktopResult<()> {
    ensure_overlay(app).await?;
    move_agent_cursor(app, x, y, false).await
}

/// Move (and optionally pulse-click) the agent cursor.
pub async fn move_agent_cursor(app: &AppHandle, x: f64, y: f64, click: bool) -> DesktopResult<()> {
    ensure_overlay(app).await?;
    let Some(win) = app.get_webview_window(WINDOW_LABEL) else {
        return Err(DesktopError::Message("agent cursor overlay missing".into()));
    };
    let _ = win.show();
    let js = format!(
        "window.__agentCursorMove && window.__agentCursorMove({x}, {y}, {});",
        if click { "true" } else { "false" }
    );
    win.eval(&js)
        .map_err(|e| DesktopError::Message(format!("agent cursor eval: {e}")))?;
    // Match the CSS transition so callers can sequence OS clicks after motion.
    tokio::time::sleep(Duration::from_millis(if click { 420 } else { 400 })).await;
    Ok(())
}

pub async fn hide_agent_cursor(app: &AppHandle) -> DesktopResult<()> {
    let Some(win) = app.get_webview_window(WINDOW_LABEL) else {
        return Ok(());
    };
    let _ = win.eval("window.__agentCursorHide && window.__agentCursorHide();");
    tokio::time::sleep(Duration::from_millis(120)).await;
    let _ = win.hide();
    Ok(())
}
