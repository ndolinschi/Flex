//! `BrowserPlugin` — drive the embedded Browser panel as agent tools.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agentloop_contracts::{BlobSource, ToolOutput, ToolResultBlock};
use agentloop_core::{
    PermissionHint, Plugin, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError,
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

const CONSOLE_HOOK_JS: &str = r#"(function(){
  if (window.__agentConsoleHooked) return 'ok';
  window.__agentConsoleHooked = true;
  window.__agentConsole = window.__agentConsole || [];
  const push = (level, args) => {
    try {
      const text = Array.from(args).map(a => {
        if (typeof a === 'string') return a;
        try { return JSON.stringify(a); } catch (_) { return String(a); }
      }).join(' ');
      window.__agentConsole.push('[' + level + '] ' + text);
      if (window.__agentConsole.length > 500) {
        window.__agentConsole.splice(0, window.__agentConsole.length - 500);
      }
    } catch (_) {}
  };
  ['log','info','warn','error','debug'].forEach(level => {
    const orig = console[level].bind(console);
    console[level] = function(){ push(level, arguments); return orig.apply(console, arguments); };
  });
  return 'ok';
})()"#;

fn schema_of<T: JsonSchema>() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(T))
        .unwrap_or_else(|_| serde_json::json!({"type": "object"}))
}

fn png_output(path: PathBuf, caption: &str) -> ToolOutput {
    ToolOutput {
        content: vec![
            ToolResultBlock::markdown(caption),
            ToolResultBlock::Image {
                media_type: "image/png".into(),
                data: BlobSource::Path { path },
            },
        ],
        is_error: false,
        structured: None,
    }
}

async fn with_webview<F, T>(app: &AppHandle, f: F) -> Result<T, ToolError>
where
    F: FnOnce(&tauri::Webview) -> Result<T, ToolError>,
{
    let state = app.state::<AppState>();
    let guard = state.browser_webview.lock().await;
    let webview = guard.as_ref().ok_or_else(|| {
        ToolError::Execution(
            "Browser panel is not open. Ask the user to open the Browser tab first.".into(),
        )
    })?;
    f(webview)
}

async fn eval_string(app: &AppHandle, script: &str) -> Result<String, ToolError> {
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let tx = Arc::new(Mutex::new(Some(tx)));
    with_webview(app, |wv| {
        let tx = Arc::clone(&tx);
        wv.eval_with_callback(script, move |raw| {
            if let Ok(mut slot) = tx.lock() {
                if let Some(sender) = slot.take() {
                    let _ = sender.send(raw);
                }
            }
        })
        .map_err(|e| ToolError::Execution(e.to_string()))
    })
    .await?;
    tokio::time::timeout(std::time::Duration::from_secs(8), rx)
        .await
        .map_err(|_| ToolError::Timeout(8_000))?
        .map_err(|_| ToolError::Execution("page eval callback dropped".into()))
}

pub struct BrowserPlugin {
    app: AppHandle,
}

impl BrowserPlugin {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl Plugin for BrowserPlugin {
    fn id(&self) -> &'static str {
        "browser"
    }

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        let app = self.app.clone();
        vec![
            Arc::new(BrowserNavigateTool { app: app.clone() }),
            Arc::new(BrowserScreenshotTool { app: app.clone() }),
            Arc::new(BrowserEvalTool { app: app.clone() }),
            Arc::new(BrowserClickTool { app: app.clone() }),
            Arc::new(BrowserConsoleTool { app: app.clone() }),
            Arc::new(BrowserOpenDevtoolsTool { app }),
        ]
    }

    fn system_prompt_fragment(&self) -> Option<String> {
        Some(
            "Browser tools control the embedded Browser panel (not headless Chromium).\n\
             - BrowserNavigate loads a URL; BrowserScreenshot captures the panel.\n\
             - BrowserEval runs JS; BrowserClick clicks a CSS selector in-page.\n\
             - BrowserConsole returns captured console.log/warn/error lines.\n\
             - BrowserOpenDevtools opens the webview inspector for the user.\n\
             Prefer these when debugging a live page in the Browser tab."
                .into(),
        )
    }
}

struct BrowserNavigateTool {
    app: AppHandle,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrowserNavigateInput {
    url: String,
}

#[async_trait]
impl Tool for BrowserNavigateTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "BrowserNavigate".into(),
            description: "Navigate the embedded Browser panel to a URL.".into(),
            input_schema: schema_of::<BrowserNavigateInput>(),
            read_only: false,
            category: ToolCategory::Web,
            needs_permission: PermissionHint::Always,
        }
    }

    async fn run(
        &self,
        _ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: BrowserNavigateInput = serde_json::from_value(input).map_err(|e| {
            ToolError::InvalidInput(format!(
                "BrowserNavigate expects {{\"url\": \"https://...\"}}: {e}"
            ))
        })?;
        let target = crate::browser::normalize_url_public(&input.url)
            .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
        with_webview(&self.app, |wv| {
            wv.navigate(target.clone())
                .map_err(|e| ToolError::Execution(e.to_string()))
        })
        .await?;
        tokio::time::sleep(std::time::Duration::from_millis(350)).await;
        let _ = eval_string(&self.app, CONSOLE_HOOK_JS).await;
        Ok(ToolOutput::text(format!("Navigated to {target}")))
    }
}

struct BrowserScreenshotTool {
    app: AppHandle,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrowserScreenshotInput {}

#[async_trait]
impl Tool for BrowserScreenshotTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "BrowserScreenshot".into(),
            description: "Capture a PNG of the embedded Browser panel viewport \
                          (macOS; other platforms not yet supported)."
                .into(),
            input_schema: schema_of::<BrowserScreenshotInput>(),
            read_only: true,
            category: ToolCategory::Web,
            needs_permission: PermissionHint::Always,
        }
    }

    async fn run(
        &self,
        _ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let _: BrowserScreenshotInput =
            serde_json::from_value(input).unwrap_or(BrowserScreenshotInput {});
        let path = capture_browser_panel(&self.app).await?;
        Ok(png_output(path, "Browser panel screenshot."))
    }
}

async fn capture_browser_panel(app: &AppHandle) -> Result<PathBuf, ToolError> {
    #[cfg(target_os = "macos")]
    {
        let state = app.state::<AppState>();
        let guard = state.browser_webview.lock().await;
        let webview = guard
            .as_ref()
            .ok_or_else(|| ToolError::Execution("Browser panel is not open.".into()))?;
        let window = webview.window();
        let scale = window
            .scale_factor()
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        let win_pos = window
            .outer_position()
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        let view_pos = webview
            .position()
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        let view_size = webview
            .size()
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        let x = (win_pos.x as f64 + view_pos.x as f64) / scale;
        let y = (win_pos.y as f64 + view_pos.y as f64) / scale;
        let w = view_size.width as f64 / scale;
        let h = view_size.height as f64 / scale;
        let out = std::env::temp_dir().join(format!(
            "agent-browser-screenshot-{}.png",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ));
        let status = std::process::Command::new("screencapture")
            .arg("-x")
            .arg("-R")
            .arg(format!("{x},{y},{w},{h}"))
            .arg(&out)
            .status()
            .map_err(|e| ToolError::Execution(format!("screencapture failed: {e}")))?;
        if !status.success() {
            return Err(ToolError::Execution(format!(
                "screencapture exited with {status}"
            )));
        }
        Ok(out)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err(ToolError::Execution(
            "BrowserScreenshot is only supported on macOS currently.".into(),
        ))
    }
}

struct BrowserEvalTool {
    app: AppHandle,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrowserEvalInput {
    script: String,
}

#[async_trait]
impl Tool for BrowserEvalTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "BrowserEval".into(),
            description: "Evaluate JavaScript in the embedded Browser page and return the \
                          stringified result."
                .into(),
            input_schema: schema_of::<BrowserEvalInput>(),
            read_only: false,
            category: ToolCategory::Web,
            needs_permission: PermissionHint::Always,
        }
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: BrowserEvalInput = serde_json::from_value(input).map_err(|e| {
            ToolError::InvalidInput(format!("BrowserEval expects {{\"script\": \"...\"}}: {e}"))
        })?;
        if ctx.cancel.is_cancelled() {
            return Err(ToolError::Cancelled);
        }
        let script = format!(
            "(function(){{ try {{ const __r = (function(){{ {} }})(); \
             return typeof __r === 'string' ? __r : JSON.stringify(__r); \
             }} catch(e) {{ return 'ERROR: ' + (e && e.message ? e.message : String(e)); }} }})()",
            input.script
        );
        Ok(ToolOutput::text(eval_string(&self.app, &script).await?))
    }
}

struct BrowserClickTool {
    app: AppHandle,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrowserClickInput {
    selector: String,
}

#[async_trait]
impl Tool for BrowserClickTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "BrowserClick".into(),
            description: "Click the first element matching a CSS selector in the Browser page."
                .into(),
            input_schema: schema_of::<BrowserClickInput>(),
            read_only: false,
            category: ToolCategory::Web,
            needs_permission: PermissionHint::Always,
        }
    }

    async fn run(
        &self,
        _ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: BrowserClickInput = serde_json::from_value(input).map_err(|e| {
            ToolError::InvalidInput(format!(
                "BrowserClick expects {{\"selector\": \"...\"}}: {e}"
            ))
        })?;
        let sel = serde_json::to_string(&input.selector)
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        let script = format!(
            "(function(){{ const el = document.querySelector({sel}); \
             if (!el) return 'NOT_FOUND'; \
             el.scrollIntoView({{block:'center', inline:'nearest'}}); \
             el.click(); return 'CLICKED'; }})()"
        );
        let raw = eval_string(&self.app, &script).await?;
        if raw.contains("NOT_FOUND") {
            return Ok(ToolOutput::error(format!(
                "No element matched selector {}",
                input.selector
            )));
        }
        Ok(ToolOutput::text(format!(
            "Clicked selector {}",
            input.selector
        )))
    }
}

struct BrowserConsoleTool {
    app: AppHandle,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrowserConsoleInput {
    clear: Option<bool>,
}

#[async_trait]
impl Tool for BrowserConsoleTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "BrowserConsole".into(),
            description: "Read console.log / warn / error lines captured from the Browser page."
                .into(),
            input_schema: schema_of::<BrowserConsoleInput>(),
            read_only: true,
            category: ToolCategory::Web,
            needs_permission: PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        _ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: BrowserConsoleInput =
            serde_json::from_value(input).unwrap_or(BrowserConsoleInput { clear: Some(true) });
        let clear = input.clear.unwrap_or(true);
        let clear_js = if clear {
            "window.__agentConsole = [];"
        } else {
            ""
        };
        let script = format!(
            "(function(){{ {CONSOLE_HOOK_JS}; \
             const lines = (window.__agentConsole || []).slice(); \
             {clear_js} \
             return JSON.stringify(lines); }})()"
        );
        let raw = eval_string(&self.app, &script).await?;
        let lines: Vec<String> = serde_json::from_str(&raw).unwrap_or_default();
        if lines.is_empty() {
            return Ok(ToolOutput::text("(no console lines captured)"));
        }
        Ok(ToolOutput::text(lines.join("\n")))
    }
}

struct BrowserOpenDevtoolsTool {
    app: AppHandle,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrowserOpenDevtoolsInput {}

#[async_trait]
impl Tool for BrowserOpenDevtoolsTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "BrowserOpenDevtools".into(),
            description: "Open the embedded Browser DevTools inspector for the user.".into(),
            input_schema: schema_of::<BrowserOpenDevtoolsInput>(),
            read_only: true,
            category: ToolCategory::Web,
            needs_permission: PermissionHint::Always,
        }
    }

    async fn run(
        &self,
        _ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let _: BrowserOpenDevtoolsInput =
            serde_json::from_value(input).unwrap_or(BrowserOpenDevtoolsInput {});
        with_webview(&self.app, |wv| {
            wv.open_devtools();
            Ok(())
        })
        .await?;
        Ok(ToolOutput::text("Opened Browser DevTools."))
    }
}
