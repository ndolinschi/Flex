
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use agentloop_contracts::{BlobSource, ToolOutput, ToolResultBlock};
use agentloop_core::{
    PermissionHint, Plugin, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError,
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tauri::AppHandle;

use super::cursor_overlay::{hide_agent_cursor, move_agent_cursor, show_agent_cursor};

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

pub struct ComputerPlugin {
    app: AppHandle,
}

impl ComputerPlugin {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl Plugin for ComputerPlugin {
    fn id(&self) -> &'static str {
        "computer"
    }

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        let app = self.app.clone();
        vec![
            Arc::new(ComputerScreenshotTool { app: app.clone() }),
            Arc::new(ComputerMoveTool { app: app.clone() }),
            Arc::new(ComputerClickTool { app: app.clone() }),
            Arc::new(ComputerTypeTool { app: app.clone() }),
            Arc::new(ComputerOpenAppTool { app }),
        ]
    }

    fn system_prompt_fragment(&self) -> Option<String> {
        Some(
            "Computer tools drive the host desktop (ChatGPT-style computer use).\n\
             - ComputerScreenshot captures the primary display.\n\
             - ComputerMove / ComputerClick animate a visible agent cursor, then \
               move/click the real OS pointer (screen coordinates in points).\n\
             - ComputerType types text via the keyboard; ComputerOpenApp launches \
               an application by name.\n\
             Always screenshot after risky UI actions to verify. Coordinates are \
             absolute screen points from the top-left of the primary monitor."
                .into(),
        )
    }

    fn force_ask_tools(&self) -> Vec<String> {
        vec![
            "ComputerClick".into(),
            "ComputerType".into(),
            "ComputerOpenApp".into(),
        ]
    }
}

struct ComputerScreenshotTool {
    app: AppHandle,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ComputerScreenshotInput {}

#[async_trait]
impl Tool for ComputerScreenshotTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "ComputerScreenshot".into(),
            description: "Capture a PNG of the primary display and return it as an image.".into(),
            input_schema: schema_of::<ComputerScreenshotInput>(),
            read_only: true,
            category: ToolCategory::Other,
            needs_permission: PermissionHint::Always,
        }
    }

    async fn run(
        &self,
        _ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let _: ComputerScreenshotInput =
            serde_json::from_value(input).unwrap_or(ComputerScreenshotInput {});
        let path = capture_screen().await?;
        let _ = &self.app;
        Ok(png_output(path, "Primary display screenshot."))
    }
}

struct ComputerMoveTool {
    app: AppHandle,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ComputerMoveInput {
    x: f64,
    y: f64,
}

#[async_trait]
impl Tool for ComputerMoveTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "ComputerMove".into(),
            description: "Animate the agent cursor to (x, y) and move the OS mouse there \
                          without clicking."
                .into(),
            input_schema: schema_of::<ComputerMoveInput>(),
            read_only: false,
            category: ToolCategory::Other,
            needs_permission: PermissionHint::Always,
        }
    }

    async fn run(
        &self,
        _ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: ComputerMoveInput = serde_json::from_value(input).map_err(|e| {
            ToolError::InvalidInput(format!(
                "ComputerMove expects {{\"x\": number, \"y\": number}}: {e}"
            ))
        })?;
        show_agent_cursor(&self.app, input.x, input.y)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        os_move_mouse(input.x, input.y)?;
        Ok(ToolOutput::text(format!(
            "Moved agent cursor to ({}, {})",
            input.x, input.y
        )))
    }
}

struct ComputerClickTool {
    app: AppHandle,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ComputerClickInput {
    x: f64,
    y: f64,
    button: Option<String>,
}

#[async_trait]
impl Tool for ComputerClickTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "ComputerClick".into(),
            description: "Animate the agent cursor to (x, y), pulse a click, then perform a \
                          real OS mouse click."
                .into(),
            input_schema: schema_of::<ComputerClickInput>(),
            read_only: false,
            category: ToolCategory::Other,
            needs_permission: PermissionHint::Always,
        }
    }

    async fn run(
        &self,
        _ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: ComputerClickInput = serde_json::from_value(input).map_err(|e| {
            ToolError::InvalidInput(format!(
                "ComputerClick expects {{\"x\": number, \"y\": number}}: {e}"
            ))
        })?;
        let button = input.button.as_deref().unwrap_or("left");
        move_agent_cursor(&self.app, input.x, input.y, true)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        os_click(input.x, input.y, button)?;
        let _ = hide_agent_cursor(&self.app).await;
        Ok(ToolOutput::text(format!(
            "Clicked {button} at ({}, {})",
            input.x, input.y
        )))
    }
}

struct ComputerTypeTool {
    app: AppHandle,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ComputerTypeInput {
    text: String,
}

#[async_trait]
impl Tool for ComputerTypeTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "ComputerType".into(),
            description: "Type text into the currently focused OS application via the keyboard."
                .into(),
            input_schema: schema_of::<ComputerTypeInput>(),
            read_only: false,
            category: ToolCategory::Other,
            needs_permission: PermissionHint::Always,
        }
    }

    async fn run(
        &self,
        _ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: ComputerTypeInput = serde_json::from_value(input).map_err(|e| {
            ToolError::InvalidInput(format!("ComputerType expects {{\"text\": \"...\"}}: {e}"))
        })?;
        let _ = &self.app;
        os_type_text(&input.text)?;
        Ok(ToolOutput::text(format!(
            "Typed {} characters.",
            input.text.chars().count()
        )))
    }
}

struct ComputerOpenAppTool {
    app: AppHandle,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ComputerOpenAppInput {
    name: String,
}

#[async_trait]
impl Tool for ComputerOpenAppTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "ComputerOpenApp".into(),
            description: "Open a desktop application by name (e.g. \"Safari\", \"Terminal\")."
                .into(),
            input_schema: schema_of::<ComputerOpenAppInput>(),
            read_only: false,
            category: ToolCategory::Other,
            needs_permission: PermissionHint::Always,
        }
    }

    async fn run(
        &self,
        _ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: ComputerOpenAppInput = serde_json::from_value(input).map_err(|e| {
            ToolError::InvalidInput(format!(
                "ComputerOpenApp expects {{\"name\": \"Safari\"}}: {e}"
            ))
        })?;
        let _ = &self.app;
        os_open_app(&input.name)?;
        Ok(ToolOutput::text(format!("Opened {}", input.name)))
    }
}

async fn capture_screen() -> Result<PathBuf, ToolError> {
    let path = crate::screen_capture::temp_png("agent-computer-screenshot");
    crate::screen_capture::capture_primary_to_png(&path).map_err(ToolError::Execution)?;
    Ok(path)
}

fn os_move_mouse(x: f64, y: f64) -> Result<(), ToolError> {
    #[cfg(target_os = "macos")]
    {
        if which("cliclick") {
            let status = Command::new("cliclick")
                .arg(format!("m:{x},{y}"))
                .status()
                .map_err(|e| ToolError::Execution(e.to_string()))?;
            if status.success() {
                return Ok(());
            }
        }
        if which("swift") {
            let script = format!(
                r#"import Cocoa
let p = CGPoint(x: {x}, y: {y})
CGWarpMouseCursorPosition(p)
CGAssociateMouseAndMouseCursorPosition(1)
"#
            );
            let status = Command::new("swift")
                .args(["-e", &script])
                .status()
                .map_err(|e| ToolError::Execution(e.to_string()))?;
            if status.success() {
                return Ok(());
            }
        }
        Err(ToolError::Execution(
            "Mouse move needs `cliclick` (brew install cliclick) or `swift` on PATH, \
             plus Accessibility permission for the app."
                .into(),
        ))
    }
    #[cfg(target_os = "linux")]
    {
        if which("xdotool") {
            let status = Command::new("xdotool")
                .args(["mousemove", &format!("{x}"), &format!("{y}")])
                .status()
                .map_err(|e| ToolError::Execution(e.to_string()))?;
            if status.success() {
                return Ok(());
            }
        }
        Err(ToolError::Execution(
            "Mouse move needs `xdotool` on Linux".into(),
        ))
    }
    #[cfg(windows)]
    {
        crate::screen_capture::windows_input::move_mouse(x, y).map_err(ToolError::Execution)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = (x, y);
        Err(ToolError::Execution(
            "ComputerMove is not supported on this platform".into(),
        ))
    }
}

fn os_click(x: f64, y: f64, button: &str) -> Result<(), ToolError> {
    #[cfg(not(windows))]
    {
        os_move_mouse(x, y)?;
    }
    #[cfg(target_os = "macos")]
    {
        if which("cliclick") {
            let arg = if button == "right" {
                format!("rc:{x},{y}")
            } else {
                format!("c:{x},{y}")
            };
            let status = Command::new("cliclick")
                .arg(arg)
                .status()
                .map_err(|e| ToolError::Execution(e.to_string()))?;
            if status.success() {
                return Ok(());
            }
        }
        Err(ToolError::Execution(
            "Mouse click needs `cliclick` (brew install cliclick) and Accessibility permission."
                .into(),
        ))
    }
    #[cfg(target_os = "linux")]
    {
        let btn = if button == "right" { "3" } else { "1" };
        if which("xdotool") {
            let status = Command::new("xdotool")
                .args(["click", btn])
                .status()
                .map_err(|e| ToolError::Execution(e.to_string()))?;
            if status.success() {
                return Ok(());
            }
        }
        Err(ToolError::Execution(
            "Mouse click needs `xdotool` on Linux".into(),
        ))
    }
    #[cfg(windows)]
    {
        crate::screen_capture::windows_input::click(x, y, button).map_err(ToolError::Execution)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = (x, y, button);
        Err(ToolError::Execution(
            "ComputerClick is not supported on this platform".into(),
        ))
    }
}

fn os_type_text(text: &str) -> Result<(), ToolError> {
    #[cfg(target_os = "macos")]
    {
        if which("cliclick") {
            let escaped = text.replace(':', "\\:");
            let status = Command::new("cliclick")
                .arg(format!("t:{escaped}"))
                .status()
                .map_err(|e| ToolError::Execution(e.to_string()))?;
            if status.success() {
                return Ok(());
            }
        }
        let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(r#"tell application "System Events" to keystroke "{escaped}""#);
        let status = Command::new("osascript")
            .args(["-e", &script])
            .status()
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        if status.success() {
            return Ok(());
        }
        Err(ToolError::Execution(
            "Typing failed — grant Accessibility permission or install cliclick.".into(),
        ))
    }
    #[cfg(target_os = "linux")]
    {
        if which("xdotool") {
            let status = Command::new("xdotool")
                .args(["type", "--", text])
                .status()
                .map_err(|e| ToolError::Execution(e.to_string()))?;
            if status.success() {
                return Ok(());
            }
        }
        Err(ToolError::Execution(
            "ComputerType needs `xdotool` on Linux".into(),
        ))
    }
    #[cfg(windows)]
    {
        crate::screen_capture::windows_input::type_text(text).map_err(ToolError::Execution)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = text;
        Err(ToolError::Execution(
            "ComputerType is not supported on this platform".into(),
        ))
    }
}

fn os_open_app(name: &str) -> Result<(), ToolError> {
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open")
            .args(["-a", name])
            .status()
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        if status.success() {
            return Ok(());
        }
        let status = Command::new("open")
            .arg(name)
            .status()
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        if status.success() {
            return Ok(());
        }
        Err(ToolError::Execution(format!(
            "Could not open application `{name}`"
        )))
    }
    #[cfg(target_os = "linux")]
    {
        let status = Command::new("gtk-launch").arg(name).status();
        if status.map(|s| s.success()).unwrap_or(false) {
            return Ok(());
        }
        let status = Command::new(name)
            .spawn()
            .map_err(|e| ToolError::Execution(format!("Could not launch `{name}`: {e}")))?;
        let _ = status;
        Ok(())
    }
    #[cfg(windows)]
    {
        crate::screen_capture::windows_input::open_app(name).map_err(ToolError::Execution)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = name;
        Err(ToolError::Execution(
            "ComputerOpenApp is not supported on this platform".into(),
        ))
    }
}

fn which(bin: &str) -> bool {
    Command::new("which")
        .arg(bin)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
