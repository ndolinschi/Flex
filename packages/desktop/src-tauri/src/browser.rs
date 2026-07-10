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
