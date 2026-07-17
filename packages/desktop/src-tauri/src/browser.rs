//! Browser panel commands — an embedded child webview navigated from Rust.
//!
//! Compromise: Tauri's `Webview` has no native back/forward-history
//! introspection, so `canGoBack`/`canGoForward` on `BrowserStateEvent` are
//! always emitted as `true`. The frontend's back/forward buttons stay
//! enabled unconditionally; `eval("history.back()")`/`eval("history.forward()")`
//! are harmless no-ops in the webview when there's nowhere left to go.
//!
//! Load failures: wry/`PageLoadEvent` only exposes Started/Finished (no Failed),
//! so after Finished we probe the document via `eval_with_callback` for
//! chrome-error / about:neterror schemes and connection-refused body text,
//! then emit `BrowserStateEvent.error` for the frontend load-error UI.
//!
//! Design Mode: an injected page script highlights elements on hover and, on
//! click, stashes a JSON payload then navigates to `agentloop-design://…`.
//! `on_navigation` cancels that scheme, reads the payload via
//! `eval_with_callback`, and emits `browser-design-event` for the Composer.

use std::sync::{Arc, Mutex};

use tauri::utils::config::Color;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Rect, State, Url, WebviewBuilder,
    WebviewUrl,
};

use crate::error::{DesktopError, DesktopResult};
use crate::state::AppState;

const BROWSER_LABEL: &str = "panel-browser";
const DEFAULT_URL: &str = "https://www.google.com";
const DESIGN_SCHEME: &str = "agentloop-design";

/// Runs in the child webview after `PageLoadEvent::Finished`. Returns a JSON
/// object `{ "error": bool, "message"?: string }` for `eval_with_callback`.
const DETECT_LOAD_ERROR_JS: &str = r#"(function () {
  try {
    var href = String(location.href || "");
    var title = String(document.title || "").toLowerCase();
    var body = "";
    try {
      body = String((document.body && (document.body.innerText || document.body.textContent)) || "")
        .slice(0, 4000)
        .toLowerCase();
    } catch (_) {}
    var schemeError =
      href.indexOf("chrome-error:") === 0 ||
      href.indexOf("chrome://error") === 0 ||
      href.indexOf("about:neterror") === 0 ||
      href.indexOf("chromewebdata") !== -1;
    var hints = [
      "err_connection_refused",
      "err_name_not_resolved",
      "err_internet_disconnected",
      "err_timed_out",
      "err_address_unreachable",
      "err_connection_reset",
      "err_connection_timed_out",
      "err_network_changed",
      "err_tunnel_connection_failed",
      "err_ssl_protocol_error",
      "err_cert_",
      "dns_probe_finished",
      "this site can't be reached",
      "this site can’t be reached",
      "can't be reached",
      "cannot be reached",
      "refused to connect",
      "took too long to respond",
      "server not found",
      "failed to open page",
      "safari can't open",
      "safari can’t open",
      "safari can't find",
      "safari can’t find",
      "not connected to the internet",
      "webpage not available",
      "unable to connect",
      "connection refused",
      "site can't be reached",
      "site can’t be reached"
    ];
    var hit = schemeError;
    if (!hit) {
      for (var i = 0; i < hints.length; i++) {
        if (body.indexOf(hints[i]) !== -1 || title.indexOf(hints[i]) !== -1) {
          hit = true;
          break;
        }
      }
    }
    if (!hit) return { error: false };
    var message = "Connection failed";
    for (var j = 0; j < hints.length; j++) {
      if (body.indexOf(hints[j]) !== -1) {
        message = hints[j];
        break;
      }
    }
    return { error: true, message: message };
  } catch (e) {
    return { error: false };
  }
})()"#;

/// Injected when Design Mode is enabled. Idempotent: tears down any prior
/// install first. Click → stash payload on `window.__agentloopDesign` →
/// navigate to `agentloop-design://select` (cancelled by `on_navigation`).
/// Escape → `agentloop-design://exit`.
const DESIGN_MODE_INJECT_JS: &str = r##"(function () {
  try {
    if (window.__agentloopDesignApi && typeof window.__agentloopDesignApi.teardown === "function") {
      window.__agentloopDesignApi.teardown();
    }
  } catch (_) {}
  var STYLE_ID = "__agentloop-design-style";
  var LABEL_ID = "__agentloop-design-label";
  var ATTR = "data-agentloop-design-hl";
  var last = null;
  var active = true;
  var labelEl = null;

  function ensureStyle() {
    if (document.getElementById(STYLE_ID)) return;
    var s = document.createElement("style");
    s.id = STYLE_ID;
    s.textContent = [
      "[" + ATTR + "]{outline:2px solid #3b82f6 !important;outline-offset:2px !important;cursor:crosshair !important;}",
      "#" + LABEL_ID + "{position:fixed;z-index:99999;isolation:isolate;pointer-events:none;max-width:min(360px,calc(100vw - 16px));",
      "padding:3px 8px;border-radius:4px;background:#1d4ed8;color:#fff;font:600 11px/1.35 ui-sans-serif,system-ui,-apple-system,sans-serif;",
      "box-shadow:0 2px 8px rgba(0,0,0,.28);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;",
      "transform:translateY(-100%);margin-top:-6px;}",
      "#" + LABEL_ID + "[data-below='1']{transform:none;margin-top:6px;}"
    ].join("");
    (document.head || document.documentElement).appendChild(s);
  }

  function ensureLabel() {
    if (labelEl && labelEl.isConnected) return labelEl;
    labelEl = document.getElementById(LABEL_ID);
    if (labelEl) return labelEl;
    labelEl = document.createElement("div");
    labelEl.id = LABEL_ID;
    labelEl.setAttribute("aria-hidden", "true");
    // Append to <body>, not <html>: a bare div under documentElement renders
    // inconsistently in WKWebView. Fall back to documentElement pre-body.
    (document.body || document.documentElement).appendChild(labelEl);
    return labelEl;
  }

  function hideLabel() {
    if (!labelEl) return;
    labelEl.style.display = "none";
  }

  function showLabel(el, name) {
    ensureStyle();
    var tip = ensureLabel();
    tip.textContent = name;
    tip.style.display = "block";
    var r = el.getBoundingClientRect();
    var top = r.top;
    var below = top < 28;
    tip.setAttribute("data-below", below ? "1" : "0");
    var left = Math.max(8, Math.min(r.left, window.innerWidth - 16));
    tip.style.left = left + "px";
    tip.style.top = (below ? r.bottom : r.top) + "px";
  }

  function clearHl() {
    if (last) {
      try { last.removeAttribute(ATTR); } catch (_) {}
      last = null;
    }
    hideLabel();
  }

  function pickTarget(el) {
    if (!el || el.nodeType !== 1) return null;
    if (el === document.documentElement || el === document.body) return null;
    if (el.id === STYLE_ID || el.id === LABEL_ID) return null;
    if (el.closest && el.closest("#" + LABEL_ID)) return null;
    return el;
  }

  function cssEscape(v) {
    if (window.CSS && CSS.escape) return CSS.escape(v);
    return String(v).replace(/[^a-zA-Z0-9_-]/g, "\\$&");
  }

  function buildSelector(el) {
    if (el.id && /^[A-Za-z][\w:-]*$/.test(el.id)) return '#' + cssEscape(el.id);
    var testId = el.getAttribute("data-testid");
    if (testId) return "[data-testid=\"" + String(testId).replace(/"/g, '\\"') + "\"]";
    var parts = [];
    var cur = el;
    for (var depth = 0; cur && cur.nodeType === 1 && depth < 6; depth++) {
      if (cur === document.body || cur === document.documentElement) break;
      var tag = cur.tagName.toLowerCase();
      if (cur.id && /^[A-Za-z][\w:-]*$/.test(cur.id)) {
        parts.unshift('#' + cssEscape(cur.id));
        break;
      }
      var parent = cur.parentElement;
      if (!parent) {
        parts.unshift(tag);
        break;
      }
      var siblings = parent.children;
      var same = 0;
      var idx = 0;
      for (var i = 0; i < siblings.length; i++) {
        if (siblings[i].tagName === cur.tagName) {
          same++;
          if (siblings[i] === cur) idx = same;
        }
      }
      parts.unshift(same > 1 ? tag + ":nth-of-type(" + idx + ")" : tag);
      cur = parent;
    }
    return parts.join(" > ");
  }

  function buildXPath(el) {
    var parts = [];
    var cur = el;
    while (cur && cur.nodeType === 1 && cur !== document.documentElement) {
      var tag = cur.tagName.toLowerCase();
      var parent = cur.parentElement;
      if (!parent) {
        parts.unshift(tag);
        break;
      }
      var siblings = Array.prototype.filter.call(parent.children, function (c) {
        return c.tagName === cur.tagName;
      });
      var ix = siblings.indexOf(cur) + 1;
      parts.unshift(siblings.length > 1 ? tag + "[" + ix + "]" : tag);
      cur = parent;
    }
    return "/" + parts.join("/");
  }

  function keyAttrs(el) {
    var keys = ["href", "name", "type", "role", "aria-label", "data-testid", "placeholder", "title", "alt", "for", "value"];
    var out = {};
    for (var i = 0; i < keys.length; i++) {
      var v = el.getAttribute(keys[i]);
      if (v != null && v !== "") out[keys[i]] = String(v).slice(0, 200);
    }
    return out;
  }

  function keyStyles(el) {
    var cs = window.getComputedStyle(el);
    return {
      display: cs.display,
      color: cs.color,
      backgroundColor: cs.backgroundColor,
      font: cs.font,
      width: cs.width,
      height: cs.height,
      padding: cs.padding,
      margin: cs.margin
    };
  }

  function describe(el) {
    var r = el.getBoundingClientRect();
    var html = "";
    try { html = String(el.outerHTML || "").slice(0, 2000); } catch (_) {}
    var classes = "";
    try {
      classes = typeof el.className === "string" ? el.className : (el.className && el.className.baseVal) || "";
    } catch (_) {}
    return {
      url: String(location.href || ""),
      tag: el.tagName.toLowerCase(),
      id: el.id || null,
      classes: classes || null,
      selector: buildSelector(el),
      xpath: buildXPath(el),
      attributes: keyAttrs(el),
      outerHtml: html,
      styles: keyStyles(el),
      rect: { x: r.x, y: r.y, width: r.width, height: r.height }
    };
  }

  function chipName(el) {
    var tag = el.tagName.toLowerCase();
    var aria = el.getAttribute("aria-label");
    if (aria) return tag + ' "' + String(aria).slice(0, 40) + '"';
    if (el.id) return tag + '#' + el.id;
    var testId = el.getAttribute("data-testid");
    if (testId) return tag + "[data-testid=" + testId + "]";
    var cls = "";
    try {
      cls = typeof el.className === "string" ? el.className.trim().split(/\s+/)[0] : "";
    } catch (_) {}
    if (cls) return tag + "." + cls;
    return "<" + tag + ">";
  }

  function signal(kind, payload) {
    try {
      window.__agentloopDesign = payload || { type: kind };
      var href = "agentloop-design://" + kind;
      // Main-frame navigation is the reliable channel: wry's on_navigation
      // policy hook fires for it and cancels the scheme (see is_design_scheme),
      // so the page never actually navigates away. Sub-frame (iframe)
      // navigations to a custom scheme are NOT reliably delivered to
      // on_navigation on WKWebView — that was the "click adds no chip" bug.
      try {
        location.href = href;
      } catch (_) {
        try {
          var frame = document.createElement("iframe");
          frame.style.cssText = "display:none;width:0;height:0;border:0;position:absolute";
          frame.setAttribute("aria-hidden", "true");
          frame.src = href;
          (document.documentElement || document.body).appendChild(frame);
          setTimeout(function () {
            try { frame.remove(); } catch (_) {}
          }, 0);
        } catch (_) {}
      }
    } catch (_) {}
  }

  function onMove(e) {
    if (!active) return;
    var t = pickTarget(e.target);
    if (t === last) {
      if (t) try { showLabel(t, chipName(t)); } catch (_) {}
      return;
    }
    clearHl();
    if (!t) return;
    ensureStyle();
    try { t.setAttribute(ATTR, "1"); } catch (_) {}
    last = t;
    try { showLabel(t, chipName(t)); } catch (_) {}
  }

  function onClick(e) {
    if (!active) return;
    var t = pickTarget(e.target);
    if (!t) return;
    e.preventDefault();
    e.stopPropagation();
    if (typeof e.stopImmediatePropagation === "function") e.stopImmediatePropagation();
    var name = "element";
    try { name = chipName(t); } catch (_) {}
    var element = { url: String(location.href || ""), tag: t.tagName ? t.tagName.toLowerCase() : "", selector: "" };
    try { element = describe(t); } catch (_) {}
    // Selection MUST fire first — never let a label/describe failure below
    // swallow the click (that was the "clicking does nothing" bug).
    signal("select", {
      type: "select",
      additive: !!e.shiftKey,
      name: name,
      element: element
    });
    try { showLabel(t, "Added · " + name); } catch (_) {}
  }

  function onKey(e) {
    if (!active) return;
    if (e.key === "Escape") {
      e.preventDefault();
      signal("exit", { type: "exit" });
    }
  }

  function onScroll() {
    if (!active || !last) return;
    showLabel(last, chipName(last));
  }

  function teardown() {
    active = false;
    clearHl();
    document.removeEventListener("mousemove", onMove, true);
    document.removeEventListener("click", onClick, true);
    document.removeEventListener("keydown", onKey, true);
    window.removeEventListener("scroll", onScroll, true);
    var s = document.getElementById(STYLE_ID);
    if (s) try { s.remove(); } catch (_) {}
    var tip = document.getElementById(LABEL_ID);
    if (tip) try { tip.remove(); } catch (_) {}
    labelEl = null;
    try { delete window.__agentloopDesignApi; } catch (_) { window.__agentloopDesignApi = null; }
  }

  ensureStyle();
  document.addEventListener("mousemove", onMove, true);
  document.addEventListener("click", onClick, true);
  document.addEventListener("keydown", onKey, true);
  window.addEventListener("scroll", onScroll, true);
  window.__agentloopDesignApi = { teardown: teardown, active: function () { return active; } };
  return true;
})()"##;

const DESIGN_MODE_TEARDOWN_JS: &str = r#"(function () {
  try {
    if (window.__agentloopDesignApi && typeof window.__agentloopDesignApi.teardown === "function") {
      window.__agentloopDesignApi.teardown();
    }
  } catch (_) {}
  return true;
})()"#;

const DESIGN_MODE_READ_PAYLOAD_JS: &str = r#"(function () {
  try {
    var p = window.__agentloopDesign || null;
    window.__agentloopDesign = null;
    return p;
  } catch (_) {
    return null;
  }
})()"#;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStateEvent {
    pub url: String,
    pub title: Option<String>,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    /// Navigation/load failure when detected after Finished (error-scheme URL
    /// or connection-refused body via eval). `None` on loading pulses and
    /// successful finishes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<BrowserLoadError>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserLoadError {
    pub host: String,
    pub message: String,
}

/// Frontend-facing Design Mode event (select / exit).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BrowserDesignEvent {
    #[serde(rename = "select", rename_all = "camelCase")]
    Select {
        additive: bool,
        name: String,
        element: serde_json::Value,
    },
    #[serde(rename = "exit")]
    Exit,
}

#[derive(Debug, serde::Deserialize)]
struct DetectLoadErrorResult {
    error: bool,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct DesignSelectPayload {
    #[serde(default)]
    #[allow(dead_code)]
    r#type: Option<String>,
    #[serde(default)]
    additive: bool,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    element: Option<serde_json::Value>,
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

fn host_of(url: &Url) -> String {
    match (url.host_str(), url.port()) {
        (Some(host), Some(port)) => format!("{host}:{port}"),
        (Some(host), None) => host.to_owned(),
        _ => url.to_string(),
    }
}

fn looks_like_error_url(url: &Url) -> bool {
    let s = url.as_str();
    let scheme = url.scheme();
    scheme == "chrome-error"
        || s.starts_with("chrome-error:")
        || s.starts_with("chrome://error")
        || s.contains("chromewebdata")
        || (scheme == "about" && s.contains("neterror"))
}

fn is_design_scheme(url: &Url) -> bool {
    url.scheme() == DESIGN_SCHEME
}

fn design_mode_enabled(app: &AppHandle) -> bool {
    app.try_state::<AppState>()
        .and_then(|s| s.browser_design_mode.lock().ok().map(|g| *g))
        .unwrap_or(false)
}

fn set_design_mode_flag(app: &AppHandle, enabled: bool) {
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut g) = state.browser_design_mode.lock() {
            *g = enabled;
        }
    }
}

fn inject_design_mode(webview: &tauri::Webview) {
    let _ = webview.eval(DESIGN_MODE_INJECT_JS);
}

fn teardown_design_mode(webview: &tauri::Webview) {
    let _ = webview.eval(DESIGN_MODE_TEARDOWN_JS);
}

fn parse_eval_json<T: serde::de::DeserializeOwned>(raw: &str) -> Option<T> {
    serde_json::from_str(raw).ok().or_else(|| {
        serde_json::from_str::<String>(raw)
            .ok()
            .and_then(|inner| serde_json::from_str(&inner).ok())
    })
}

/// Handle `agentloop-design://…` navigations: cancel them, read the stashed
/// payload, emit `browser-design-event`. Called from `on_navigation`.
fn handle_design_navigation(app: &AppHandle, url: &Url) {
    let kind = url.host_str().unwrap_or("select");
    tracing::debug!(target: "browser", kind, "design-mode navigation intercepted");
    let Some(webview) = app.get_webview(BROWSER_LABEL) else {
        return;
    };

    if kind == "exit" {
        set_design_mode_flag(app, false);
        teardown_design_mode(&webview);
        let _ = app.emit("browser-design-event", &BrowserDesignEvent::Exit);
        return;
    }

    // Default: select (and any unknown kind treated as select).
    let emit_app = app.clone();
    let _ = webview.eval_with_callback(DESIGN_MODE_READ_PAYLOAD_JS, move |raw| {
        let parsed: Option<DesignSelectPayload> = parse_eval_json(&raw);
        let Some(payload) = parsed else {
            tracing::warn!(target: "browser", "design-mode select payload failed to parse");
            return;
        };
        let element = payload.element.unwrap_or(serde_json::Value::Null);
        let name = payload
            .name
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "element".into());
        let _ = emit_app.emit(
            "browser-design-event",
            &BrowserDesignEvent::Select {
                additive: payload.additive,
                name,
                element,
            },
        );
    });
}

fn emit_state(
    app: &AppHandle,
    url: &Url,
    title: Option<String>,
    loading: bool,
    error: Option<BrowserLoadError>,
) {
    let _ = app.emit(
        "browser-state",
        &BrowserStateEvent {
            url: url.to_string(),
            title,
            loading,
            can_go_back: true,
            can_go_forward: true,
            error,
        },
    );
}

fn display_url(last_requested: &Mutex<Option<Url>>, fallback: &Url) -> Url {
    last_requested
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .filter(|u| !looks_like_error_url(u))
        .unwrap_or_else(|| fallback.clone())
}

fn probe_and_emit_finished(
    app: AppHandle,
    webview: tauri::Webview,
    url: Url,
    last_requested: Arc<Mutex<Option<Url>>>,
    last_error: Arc<Mutex<Option<BrowserLoadError>>>,
) {
    let emit_error = {
        let app = app.clone();
        let last_requested = Arc::clone(&last_requested);
        let last_error = Arc::clone(&last_error);
        move |page_url: Url, message: String| {
            let shown = display_url(&last_requested, &page_url);
            let host = host_of(&shown);
            let err = BrowserLoadError {
                host: host.clone(),
                message: if message.is_empty()
                    || message.starts_with("err_")
                    || message.contains("refused")
                {
                    format!("{host} refused to connect")
                } else {
                    format!("{host}: {message}")
                },
            };
            if let Ok(mut g) = last_error.lock() {
                *g = Some(err.clone());
            }
            emit_state(&app, &shown, None, false, Some(err));
        }
    };

    if looks_like_error_url(&url) {
        emit_error(url, "connection failed".into());
        return;
    }

    let app_ok = app.clone();
    let last_error_ok = Arc::clone(&last_error);
    let url_for_cb = url.clone();
    let url_ok = url.clone();
    // Only the first completion (callback or timeout) may emit — avoids a
    // late probe result flipping success→error or double-emitting Finished.
    let settled = Arc::new(Mutex::new(false));
    let settled_cb = Arc::clone(&settled);
    let settled_timeout = Arc::clone(&settled);

    if webview
        .eval_with_callback(DETECT_LOAD_ERROR_JS, move |raw| {
            {
                let Ok(mut g) = settled_cb.lock() else { return };
                if *g {
                    return;
                }
                *g = true;
            }
            let parsed: Option<DetectLoadErrorResult> =
                serde_json::from_str(&raw).ok().or_else(|| {
                    // Some platforms double-encode the return value as a JSON string.
                    serde_json::from_str::<String>(&raw)
                        .ok()
                        .and_then(|inner| serde_json::from_str(&inner).ok())
                });
            match parsed {
                Some(result) if result.error => {
                    emit_error(
                        url_for_cb.clone(),
                        result.message.unwrap_or_else(|| "connection failed".into()),
                    );
                }
                _ => {
                    if let Ok(mut g) = last_error_ok.lock() {
                        *g = None;
                    }
                    emit_state(&app_ok, &url_ok, None, false, None);
                }
            }
        })
        .is_err()
    {
        // Eval unavailable — still clear the spinner rather than hang.
        if let Ok(mut g) = last_error.lock() {
            *g = None;
        }
        emit_state(&app, &url, None, false, None);
        return;
    }

    // If the probe callback never returns (CSP / hung eval), still emit a
    // successful Finished so the frontend doesn't stay on loading forever.
    let app_timeout = app.clone();
    let url_timeout = url;
    let last_error_timeout = Arc::clone(&last_error);
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(2_500));
        {
            let Ok(mut g) = settled_timeout.lock() else {
                return;
            };
            if *g {
                return;
            }
            *g = true;
        }
        if let Ok(mut g) = last_error_timeout.lock() {
            *g = None;
        }
        emit_state(&app_timeout, &url_timeout, None, false, None);
    });
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn browser_open(
    app: AppHandle,
    state: State<'_, AppState>,
    url: Option<String>,
) -> DesktopResult<()> {
    let target = normalize_url(url.as_deref().unwrap_or(DEFAULT_URL))?;

    let mut guard = state.browser_webview.lock().await;
    if let Some(webview) = guard.as_ref() {
        // Do not show here — the frontend reveals the webview only after
        // applying bounds for the content area below the toolbar. Showing
        // early reuses stale bounds that can cover the browser chrome.
        let _ = webview.set_background_color(Some(Color(255, 255, 255, 255)));
        webview.navigate(target.clone())?;
        emit_state(&app, &target, None, true, None);
        return Ok(());
    }

    let window = app
        .get_window("main")
        .ok_or_else(|| DesktopError::Message("main window not found".into()))?;

    let nav_app = app.clone();
    let load_app = app.clone();
    let title_app = app.clone();
    let last_requested: Arc<Mutex<Option<Url>>> = Arc::new(Mutex::new(Some(target.clone())));
    let last_error: Arc<Mutex<Option<BrowserLoadError>>> = Arc::new(Mutex::new(None));
    let nav_last_requested = Arc::clone(&last_requested);
    let nav_last_error = Arc::clone(&last_error);
    let load_last_requested = Arc::clone(&last_requested);
    let load_last_error = Arc::clone(&last_error);
    let title_last_requested = Arc::clone(&last_requested);
    let title_last_error = Arc::clone(&last_error);

    let builder = WebviewBuilder::new(BROWSER_LABEL, WebviewUrl::External(target.clone()))
        // Opaque page underlay — without this, short document bodies show the
        // app's black panel through the WKWebView and look like a layout gap.
        .background_color(Color(255, 255, 255, 255))
        .on_navigation(move |url| {
            if is_design_scheme(url) {
                handle_design_navigation(&nav_app, url);
                return false;
            }
            if looks_like_error_url(url) {
                let shown = display_url(&nav_last_requested, url);
                emit_state(&nav_app, &shown, None, true, None);
                return true;
            }
            if let Ok(mut g) = nav_last_requested.lock() {
                *g = Some(url.clone());
            }
            if let Ok(mut g) = nav_last_error.lock() {
                *g = None;
            }
            emit_state(&nav_app, url, None, true, None);
            true
        })
        .on_page_load(move |webview, payload| {
            let url = payload.url().clone();
            match payload.event() {
                tauri::webview::PageLoadEvent::Started => {
                    if let Ok(mut g) = load_last_error.lock() {
                        *g = None;
                    }
                    emit_state(&load_app, &url, None, true, None);
                }
                tauri::webview::PageLoadEvent::Finished => {
                    // Re-inject Design Mode after every Finished so SPA /
                    // full navigations keep the picker while the flag is on.
                    if design_mode_enabled(&load_app) {
                        inject_design_mode(&webview);
                    }
                    probe_and_emit_finished(
                        load_app.clone(),
                        webview,
                        url,
                        Arc::clone(&load_last_requested),
                        Arc::clone(&load_last_error),
                    );
                }
            }
        })
        .on_document_title_changed(move |webview, title| {
            if let Ok(u) = webview.url() {
                // Preserve any in-flight loadError — title pulses must not
                // clobber the Finished-path error for the frontend.
                let err = title_last_error.lock().ok().and_then(|g| g.clone());
                if looks_like_error_url(&u) || err.is_some() {
                    let shown = if looks_like_error_url(&u) {
                        display_url(&title_last_requested, &u)
                    } else {
                        u.clone()
                    };
                    emit_state(&title_app, &shown, Some(title), false, err);
                    return;
                }
                emit_state(&title_app, &u, Some(title), false, None);
            }
        });

    let scale = window.scale_factor().unwrap_or(1.0);
    let win = window
        .inner_size()
        .ok()
        .map(|s| s.to_logical::<f64>(scale))
        .unwrap_or_else(|| LogicalSize::new(1280.0, 800.0));
    // Never create at 1×1 — that poisons wry's rate-based autoresize and the
    // first visible frame. Prefer last frontend bounds; else right-half guess.
    let (x, y, w, h) = state
        .browser_bounds
        .lock()
        .ok()
        .and_then(|g| *g)
        .unwrap_or_else(|| {
            let chrome = 70.0;
            (
                (win.width * 0.55).max(0.0),
                chrome,
                (win.width * 0.45).max(1.0),
                (win.height - chrome).max(1.0),
            )
        });

    let webview = window.add_child(
        builder,
        LogicalPosition::new(x, y),
        LogicalSize::new(w.max(1.0), h.max(1.0)),
    )?;
    // Mitigates focus-steal on creation; the frontend brings it to front via
    // `browser_set_visible` once it has positioned the panel.
    webview.hide()?;
    let _ = apply_bounds(&webview, x, y, w, h);

    emit_state(&app, &target, None, true, None);
    *guard = Some(webview);
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err)]
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
    emit_state(&app, &target, None, true, None);
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn browser_back(state: State<'_, AppState>) -> DesktopResult<()> {
    let guard = state.browser_webview.lock().await;
    let webview = guard
        .as_ref()
        .ok_or_else(|| DesktopError::Message("browser is not open".into()))?;
    webview.eval("history.back()")?;
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn browser_forward(state: State<'_, AppState>) -> DesktopResult<()> {
    let guard = state.browser_webview.lock().await;
    let webview = guard
        .as_ref()
        .ok_or_else(|| DesktopError::Message("browser is not open".into()))?;
    webview.eval("history.forward()")?;
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn browser_reload(state: State<'_, AppState>) -> DesktopResult<()> {
    let guard = state.browser_webview.lock().await;
    let webview = guard
        .as_ref()
        .ok_or_else(|| DesktopError::Message("browser is not open".into()))?;
    webview.reload()?;
    Ok(())
}

fn apply_bounds(
    webview: &tauri::Webview,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> DesktopResult<()> {
    // Trust the frontend-measured slot rect — stretching to the window bottom
    // made the child webview bleed over the resize sash and adjacent panes.
    let y = y.max(0.0);
    let width = width.max(1.0);
    let height = height.max(1.0);
    let window = webview.window();
    let scale = window.scale_factor().unwrap_or(1.0);
    let win_h = window
        .inner_size()
        .map(|s| s.to_logical::<f64>(scale).height)
        .unwrap_or(height + y);

    // Atomic bounds — never set_position then set_size. On macOS the child
    // NSView is bottom-anchored; splitting the calls lets an intermediate
    // size change slide the top edge over the React toolbar. `set_bounds`
    // also updates wry's rate-based autoresize.
    webview.set_bounds(Rect {
        position: LogicalPosition::new(x, y).into(),
        size: LogicalSize::new(width, height).into(),
    })?;

    if let (Ok(pos), Ok(size)) = (webview.position(), webview.size()) {
        let applied = serde_json::json!({
            "requested": { "x": x, "y": y, "w": width, "h": height },
            "windowInnerH": win_h,
            "appliedLogical": {
                "x": pos.x as f64 / scale,
                "y": pos.y as f64 / scale,
                "w": size.width as f64 / scale,
                "h": size.height as f64 / scale,
            },
            "scale": scale,
        });
        tracing::info!(target: "browser_bounds", %applied, "applied child webview bounds");
        let _ = std::fs::write(
            std::env::temp_dir().join("flex-browser-bounds.json"),
            applied.to_string(),
        );
        // Also mirror to /tmp — macOS temp_dir is under /var/folders and
        // is easy to miss when grepping for the probe file during layout QA.
        #[cfg(unix)]
        let _ = std::fs::write("/tmp/flex-browser-bounds.json", applied.to_string());
    }
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn browser_set_bounds(
    state: State<'_, AppState>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> DesktopResult<()> {
    if let Ok(mut g) = state.browser_bounds.lock() {
        *g = Some((x, y, width, height));
    }
    let guard = state.browser_webview.lock().await;
    if let Some(webview) = guard.as_ref() {
        apply_bounds(webview, x, y, width, height)?;
    }
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn browser_set_visible(state: State<'_, AppState>, visible: bool) -> DesktopResult<()> {
    let bounds = state.browser_bounds.lock().ok().and_then(|g| *g);
    let guard = state.browser_webview.lock().await;
    if let Some(webview) = guard.as_ref() {
        if visible {
            // Re-assert bounds before reveal so the webview can never flash
            // at a stale (e.g. bottom-anchored, post-resize) position.
            if let Some((x, y, w, h)) = bounds {
                apply_bounds(webview, x, y, w, h)?;
            }
            webview.show()?;
        } else {
            webview.hide()?;
        }
    }
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn browser_close(app: AppHandle, state: State<'_, AppState>) -> DesktopResult<()> {
    set_design_mode_flag(&app, false);
    let mut guard = state.browser_webview.lock().await;
    if let Some(webview) = guard.take() {
        webview.close()?;
    }
    Ok(())
}

/// Enable or disable Design Mode (element picker) in the embedded browser.
/// Persists a flag so Finished-load handlers re-inject after navigations.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn browser_set_design_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> DesktopResult<()> {
    set_design_mode_flag(&app, enabled);
    let guard = state.browser_webview.lock().await;
    if let Some(webview) = guard.as_ref() {
        if enabled {
            inject_design_mode(webview);
        } else {
            teardown_design_mode(webview);
        }
    }
    Ok(())
}

/// Apply temporary CSS property overrides to an element matched by `selector`
/// in the embedded browser. Used by the Components tab CSS panel for live
/// preview — does not write source files.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn browser_apply_style_overrides(
    state: State<'_, AppState>,
    selector: String,
    styles: std::collections::HashMap<String, String>,
) -> DesktopResult<()> {
    let guard = state.browser_webview.lock().await;
    let webview = guard
        .as_ref()
        .ok_or_else(|| DesktopError::Message("browser is not open".into()))?;
    let selector_json = serde_json::to_string(&selector)
        .map_err(|e| DesktopError::Message(format!("selector encode: {e}")))?;
    let styles_json = serde_json::to_string(&styles)
        .map_err(|e| DesktopError::Message(format!("styles encode: {e}")))?;
    let js = format!(
        r#"(function(){{
  try {{
    var sel = {selector_json};
    var styles = {styles_json};
    var el = document.querySelector(sel);
    if (!el) return;
    Object.keys(styles).forEach(function(k) {{
      var v = styles[k];
      if (v === null || v === undefined || v === "") {{
        el.style.removeProperty(k);
      }} else {{
        el.style.setProperty(k, String(v));
      }}
    }});
  }} catch (e) {{}}
}})()"#
    );
    let _ = webview.eval(&js);
    Ok(())
}

/// Opens DevTools for the embedded browser's child webview only — never the
/// app's main webview. Errors if the browser hasn't been opened yet so the
/// frontend can toast instead of silently no-op'ing (Cursor parity).
///
/// On macOS, WebKit's inspector defaults to docking into the parent window
/// (full-width, shoving the right panel). After `show` we call private
/// `_WKInspector::detach` so it opens as a floating window instead.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn browser_open_devtools(state: State<'_, AppState>) -> DesktopResult<()> {
    let guard = state.browser_webview.lock().await;
    let webview = guard
        .as_ref()
        .cloned()
        .ok_or_else(|| DesktopError::Message("browser is not open".into()))?;
    drop(guard);
    webview.open_devtools();
    #[cfg(target_os = "macos")]
    {
        // WebKit creates and DOCKS the _WKInspector asynchronously after
        // open_devtools(); a single deferred detach often no-ops (the inspector
        // view doesn't exist yet), leaving it docked full-width over the panel.
        // Retry with backoff so detach lands after the inspector is up.
        std::thread::spawn(move || {
            const DELAYS_MS: &[u64] = &[0, 200, 500, 1000];
            let mut last_nil = false;
            for &delay in DELAYS_MS {
                if delay > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(delay));
                }
                let detached = std::sync::atomic::AtomicBool::new(false);
                let _ = webview.with_webview(|platform| {
                    detached.store(
                        detach_macos_inspector(platform.inner()),
                        std::sync::atomic::Ordering::Relaxed,
                    );
                });
                if detached.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                last_nil = true;
            }
            if last_nil {
                tracing::warn!(
                    target: "browser",
                    "WKWebView _inspector was nil after retries — devtools may stay docked"
                );
            }
        });
    }
    Ok(())
}

/// Detach the WKWebView inspector into a floating window (private API).
/// Returns `true` when `_inspector` was found and detach was invoked.
#[cfg(target_os = "macos")]
fn detach_macos_inspector(wk_webview: *mut std::ffi::c_void) -> bool {
    if wk_webview.is_null() {
        return false;
    }
    unsafe {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        let view: &AnyObject = &*wk_webview.cast::<AnyObject>();
        let inspector: *mut AnyObject = msg_send![view, _inspector];
        if inspector.is_null() {
            return false;
        }
        let insp: &AnyObject = &*inspector;
        // show is usually already called by open_devtools; detach undocks it.
        let _: () = msg_send![insp, show];
        let _: () = msg_send![insp, detach];
        true
    }
}

/// Hard reload — bypasses Tauri's `reload()` (a plain in-place reload) and
/// instead re-navigates to the current URL, which forces the webview to
/// re-fetch rather than potentially serve from its own cache. `reload()`
/// remains available as the soft-reload path (`browser_reload`); this command
/// is the "…" menu's cache-busting variant.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn browser_hard_reload(app: AppHandle, state: State<'_, AppState>) -> DesktopResult<()> {
    let guard = state.browser_webview.lock().await;
    let webview = guard
        .as_ref()
        .ok_or_else(|| DesktopError::Message("browser is not open".into()))?;
    let current = webview.url()?;
    webview.navigate(current.clone())?;
    emit_state(&app, &current, None, true, None);
    Ok(())
}

/// Clears cookies, cache, and other browsing data for the embedded browser's
/// child webview via wry/Tauri's `clear_all_browsing_data` — shipped as one
/// "Clear Browsing Data" action rather than separate cookie/cache items since
/// the underlying API doesn't expose that granularity. Errors when no child
/// webview is open (frontend disables the menu item; this is the safety net).
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn browser_clear_data(state: State<'_, AppState>) -> DesktopResult<()> {
    let guard = state.browser_webview.lock().await;
    let webview = guard
        .as_ref()
        .ok_or_else(|| DesktopError::Message("browser is not open".into()))?;
    webview.clear_all_browsing_data()?;
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
///
/// `screencapture` is a macOS-only binary, so the real implementation is
/// `#[cfg(target_os = "macos")]`-gated; every other platform gets the stub
/// below, which returns a `DesktopResult` error (surfaced to the frontend as
/// a toast — see the module's error-path convention) rather than failing to
/// compile or panicking at runtime.
#[cfg(target_os = "macos")]
#[tracing::instrument(level = "debug", skip_all, err)]
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

/// Non-macOS stub: no equivalent region-capture binary is wired up yet
/// (Windows would need a Win32/GDI capture, Linux would need portal/X11
/// grab). Returns a clear, user-visible error instead of silently no-op'ing.
#[cfg(not(target_os = "macos"))]
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn browser_screenshot(_state: State<'_, AppState>) -> DesktopResult<String> {
    Err(DesktopError::Message(
        "Screenshots are not supported on this platform yet".into(),
    ))
}
