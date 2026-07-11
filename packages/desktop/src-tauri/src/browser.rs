//! Browser panel commands — an embedded child webview navigated from Rust.
//!
//! Compromise: Tauri's `Webview` has no native back/forward-history
//! introspection, so `canGoBack`/`canGoForward` on `BrowserStateEvent` are
//! always emitted as `true`. The frontend's back/forward buttons stay
//! enabled unconditionally; `eval("history.back()")`/`eval("history.forward()")`
//! are harmless no-ops in the webview when there's nowhere left to go.

use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, Url, WebviewBuilder,
    WebviewUrl,
};

use crate::error::{DesktopError, DesktopResult};
use crate::state::AppState;

const BROWSER_LABEL: &str = "panel-browser";
const DEFAULT_URL: &str = "https://www.google.com";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStateEvent {
    pub url: String,
    pub title: Option<String>,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    /// Navigation/load failure, when detected. Always `None` from this native
    /// side today: Tauri/wry's `on_page_load` hook only exposes
    /// `PageLoadEvent::Started`/`Finished` (no `Failed` variant), so there is
    /// no native signal to populate this from. The frontend's load-error page
    /// is wired to this field and demoed via the preview mock's deterministic
    /// failing URL (see `browserMock.ts`'s `FAILING_MOCK_HOST`) — the native
    /// path is an honest gap, not faked with a timeout.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<BrowserLoadError>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserLoadError {
    pub host: String,
    pub message: String,
}

fn normalize_url(raw: &str) -> DesktopResult<Url> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(DesktopError::Message("url is required".into()));
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("https://{trimmed}")
    };
    Url::parse(&candidate).map_err(|e| DesktopError::Message(format!("invalid url: {e}")))
}

fn emit_state(app: &AppHandle, url: &Url, title: Option<String>, loading: bool) {
    let _ = app.emit(
        "browser-state",
        &BrowserStateEvent {
            url: url.to_string(),
            title,
            loading,
            can_go_back: true,
            can_go_forward: true,
            error: None,
        },
    );
}

#[tauri::command]
pub async fn browser_open(
    app: AppHandle,
    state: State<'_, AppState>,
    url: Option<String>,
) -> DesktopResult<()> {
    let target = normalize_url(url.as_deref().unwrap_or(DEFAULT_URL))?;

    let mut guard = state.browser_webview.lock().await;
    if let Some(webview) = guard.as_ref() {
        webview.navigate(target.clone())?;
        webview.show()?;
        emit_state(&app, &target, None, true);
        return Ok(());
    }

    let window = app
        .get_window("main")
        .ok_or_else(|| DesktopError::Message("main window not found".into()))?;

    let nav_app = app.clone();
    let load_app = app.clone();
    let title_app = app.clone();

    let builder = WebviewBuilder::new(BROWSER_LABEL, WebviewUrl::External(target.clone()))
        .on_navigation(move |url| {
            emit_state(&nav_app, &url, None, true);
            true
        })
        .on_page_load(move |_webview, payload| {
            let loading = matches!(payload.event(), tauri::webview::PageLoadEvent::Started);
            // Title isn't queryable from `Webview` directly; `on_document_title_changed`
            // below is the source of truth for title updates.
            emit_state(&load_app, payload.url(), None, loading);
        })
        .on_document_title_changed(move |webview, title| {
            if let Ok(u) = webview.url() {
                emit_state(&title_app, &u, Some(title), false);
            }
        });

    let webview = window.add_child(
        builder,
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(1.0, 1.0),
    )?;
    // Mitigates focus-steal on creation; the frontend brings it to front via
    // `browser_set_visible` once it has positioned the panel.
    webview.hide()?;

    emit_state(&app, &target, None, true);
    *guard = Some(webview);
    Ok(())
}

#[tauri::command]
pub async fn browser_navigate(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
) -> DesktopResult<()> {
    let target = normalize_url(&url)?;
    let guard = state.browser_webview.lock().await;
    let webview = guard
        .as_ref()
        .ok_or_else(|| DesktopError::Message("browser is not open".into()))?;
    webview.navigate(target.clone())?;
    emit_state(&app, &target, None, true);
    Ok(())
}

#[tauri::command]
pub async fn browser_back(state: State<'_, AppState>) -> DesktopResult<()> {
    let guard = state.browser_webview.lock().await;
    if let Some(webview) = guard.as_ref() {
        webview.eval("history.back()")?;
    }
    Ok(())
}

#[tauri::command]
pub async fn browser_forward(state: State<'_, AppState>) -> DesktopResult<()> {
    let guard = state.browser_webview.lock().await;
    if let Some(webview) = guard.as_ref() {
        webview.eval("history.forward()")?;
    }
    Ok(())
}

#[tauri::command]
pub async fn browser_reload(state: State<'_, AppState>) -> DesktopResult<()> {
    let guard = state.browser_webview.lock().await;
    if let Some(webview) = guard.as_ref() {
        webview.reload()?;
    }
    Ok(())
}

#[tauri::command]
pub async fn browser_set_bounds(
    state: State<'_, AppState>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> DesktopResult<()> {
    let guard = state.browser_webview.lock().await;
    if let Some(webview) = guard.as_ref() {
        webview.set_position(LogicalPosition::new(x, y))?;
        webview.set_size(LogicalSize::new(width.max(1.0), height.max(1.0)))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn browser_set_visible(state: State<'_, AppState>, visible: bool) -> DesktopResult<()> {
    let guard = state.browser_webview.lock().await;
    if let Some(webview) = guard.as_ref() {
        if visible {
            webview.show()?;
        } else {
            webview.hide()?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn browser_close(state: State<'_, AppState>) -> DesktopResult<()> {
    let mut guard = state.browser_webview.lock().await;
    if let Some(webview) = guard.take() {
        webview.close()?;
    }
    Ok(())
}

/// Opens DevTools for the embedded browser's child webview only — never the
/// app's main webview. No-ops (rather than erroring) if the browser hasn't
/// been opened yet, matching the other `browser_*` commands' tolerance for a
/// missing webview.
#[tauri::command]
pub async fn browser_open_devtools(state: State<'_, AppState>) -> DesktopResult<()> {
    let guard = state.browser_webview.lock().await;
    if let Some(webview) = guard.as_ref() {
        webview.open_devtools();
    }
    Ok(())
}

/// Hard reload — bypasses Tauri's `reload()` (a plain in-place reload) and
/// instead re-navigates to the current URL, which forces the webview to
/// re-fetch rather than potentially serve from its own cache. `reload()`
/// remains available as the soft-reload path (`browser_reload`); this command
/// is the "…" menu's cache-busting variant.
#[tauri::command]
pub async fn browser_hard_reload(app: AppHandle, state: State<'_, AppState>) -> DesktopResult<()> {
    let guard = state.browser_webview.lock().await;
    let webview = guard
        .as_ref()
        .ok_or_else(|| DesktopError::Message("browser is not open".into()))?;
    let current = webview.url()?;
    webview.navigate(current.clone())?;
    emit_state(&app, &current, None, true);
    Ok(())
}

/// Clears cookies, cache, and other browsing data for the embedded browser's
/// child webview via wry/Tauri's `clear_all_browsing_data` — shipped as one
/// "Clear Browsing Data" action rather than separate cookie/cache items since
/// the underlying API doesn't expose that granularity.
#[tauri::command]
pub async fn browser_clear_data(state: State<'_, AppState>) -> DesktopResult<()> {
    let guard = state.browser_webview.lock().await;
    if let Some(webview) = guard.as_ref() {
        webview.clear_all_browsing_data()?;
    }
    Ok(())
}

/// Captures a screenshot of the embedded browser's on-screen region.
///
/// macOS-only v1: shells out to `screencapture -x -R x,y,w,h` against the
/// webview's absolute screen rect (window `outer_position` + the child
/// webview's window-relative `position()`/`size()`, converted from physical
/// pixels to points via the window's scale factor — `screencapture -R` takes
/// point coordinates). Writes to a temp PNG under `std::env::temp_dir()` and
/// returns its path.
///
/// Caveat: if the app window isn't frontmost, this can capture whatever
/// occludes it — `screencapture -R` has no window-handle-scoped capture mode,
/// only a screen-region one. Acceptable for v1.
#[tauri::command]
pub async fn browser_screenshot(state: State<'_, AppState>) -> DesktopResult<String> {
    let guard = state.browser_webview.lock().await;
    let webview = guard
        .as_ref()
        .ok_or_else(|| DesktopError::Message("browser is not open".into()))?;

    let window = webview.window();
    let scale = window.scale_factor()?;
    let win_pos = window.outer_position()?;
    let view_pos = webview.position()?;
    let view_size = webview.size()?;

    let x = (win_pos.x as f64 + view_pos.x as f64) / scale;
    let y = (win_pos.y as f64 + view_pos.y as f64) / scale;
    let w = view_size.width as f64 / scale;
    let h = view_size.height as f64 / scale;

    let filename = format!(
        "flex-browser-screenshot-{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let out_path = std::env::temp_dir().join(filename);

    let status = std::process::Command::new("screencapture")
        .arg("-x")
        .arg("-R")
        .arg(format!("{x},{y},{w},{h}"))
        .arg(&out_path)
        .status()
        .map_err(|e| DesktopError::Message(format!("failed to run screencapture: {e}")))?;

    if !status.success() {
        return Err(DesktopError::Message(format!(
            "screencapture exited with status {status}"
        )));
    }

    Ok(out_path.to_string_lossy().into_owned())
}
