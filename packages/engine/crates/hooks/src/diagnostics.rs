//! `DiagnosticsHook` — feed compiler/linter diagnostics back to the model after
//! `Write`/`Edit`, so it can fix breakage it introduced. Availability-gated:
//! disabled by default, and skipped entirely when no configured check matches
//! the edited file's language or the check binary is not on `$PATH`.

use std::process::Stdio;
use std::time::Duration;

use agentloop_contracts::{HookPoint, ToolResultBlock};
use agentloop_core::{Hook, HookContext, HookData, HookError, HookOutcome};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::util;

/// A diagnostics run is bounded so a slow check cannot stall a turn.
const CHECK_TIMEOUT: Duration = Duration::from_secs(60);
const DEFAULT_MAX_LINES: usize = 40;

fn default_max_lines() -> usize {
    DEFAULT_MAX_LINES
}

/// A per-language diagnostics command (a compiler or linter) run after edits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckSpec {
    /// File extensions (without the dot) this check applies to.
    pub extensions: Vec<String>,
    /// Command argv; `$FILE` is replaced with the edited file's absolute path
    /// (e.g. `["cargo", "check", "--message-format=short"]`,
    /// `["ruff", "check", "$FILE"]`, `["tsc", "--noEmit"]`).
    pub command: Vec<String>,
    /// Extra environment variables for the check process.
    #[serde(default)]
    pub env: Vec<(String, String)>,
    /// When true, this spec is ignored.
    #[serde(default)]
    pub disabled: bool,
}

impl CheckSpec {
    fn handles(&self, ext: &str) -> bool {
        !self.disabled
            && !self.command.is_empty()
            && self.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
    }
}

/// Diagnostics feedback configuration. Disabled by default; when enabled, a
/// matching check runs after each edit and its failure output is appended to
/// the tool result for the model to act on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsConfig {
    /// Master switch. When false the hook is inert regardless of `checks`.
    #[serde(default)]
    pub enabled: bool,
    /// Per-language check commands.
    #[serde(default)]
    pub checks: Vec<CheckSpec>,
    /// Cap on the number of diagnostic lines appended to a tool result.
    #[serde(default = "default_max_lines")]
    pub max_lines: usize,
}

impl Default for DiagnosticsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            checks: Vec::new(),
            max_lines: DEFAULT_MAX_LINES,
        }
    }
}

/// Availability-gated diagnostics hook. It is a silent no-op when diagnostics
/// are disabled, no check matches the file's language, or the check binary is
/// not on `$PATH` — honoring "run the correctness step only when available".
pub struct DiagnosticsHook {
    config: DiagnosticsConfig,
    interests: Vec<HookPoint>,
}

impl DiagnosticsHook {
    pub fn new(config: DiagnosticsConfig) -> Self {
        Self {
            config,
            interests: vec![HookPoint::PostToolUse],
        }
    }

    /// Whether diagnostics are enabled and at least one check is configured.
    pub fn is_active(&self) -> bool {
        self.config.enabled
            && self
                .config
                .checks
                .iter()
                .any(|check| !check.disabled && !check.command.is_empty())
    }
}

#[async_trait]
impl Hook for DiagnosticsHook {
    fn interests(&self) -> &[HookPoint] {
        &self.interests
    }

    async fn on(
        &self,
        point: HookPoint,
        ctx: &mut HookContext<'_>,
    ) -> Result<HookOutcome, HookError> {
        if point != HookPoint::PostToolUse || !self.config.enabled {
            return Ok(HookOutcome::Continue);
        }
        let HookData::ToolResult { call, output } = &mut ctx.data else {
            return Ok(HookOutcome::Continue);
        };
        if !util::is_edit_tool(&call.tool_name) {
            return Ok(HookOutcome::Continue);
        }
        let Some(file) = util::edited_file(&call.input).map(str::to_owned) else {
            return Ok(HookOutcome::Continue);
        };
        let Some(ext) = util::extension_of(&file) else {
            return Ok(HookOutcome::Continue);
        };
        let Some(spec) = self
            .config
            .checks
            .iter()
            .find(|check| check.handles(&ext) && util::program_on_path(&check.command[0]))
        else {
            return Ok(HookOutcome::Continue);
        };

        let argv = util::substitute_file(&spec.command, &file);
        match run_check(&argv, &spec.env, &file).await {
            Some(diagnostics) if !diagnostics.trim().is_empty() => {
                let tail = tail_lines(&diagnostics, self.config.max_lines);
                output.content.push(ToolResultBlock::markdown(format!(
                    "Diagnostics after editing `{file}` (`{}`):\n```\n{tail}\n```\n\
                     Fix these before continuing.",
                    spec.command[0]
                )));
                Ok(HookOutcome::Mutated)
            }
            _ => Ok(HookOutcome::Continue),
        }
    }
}

/// Run the check (capturing stdio) and return its combined output when it
/// reports a problem (non-zero exit); `None` when it passes, cannot be run, or
/// times out. Diagnostics are advisory feedback, never a loop failure.
async fn run_check(argv: &[String], env: &[(String, String)], file: &str) -> Option<String> {
    let (program, args) = argv.split_first()?;
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for (key, value) in env {
        cmd.env(key, value);
    }
    if let Some(dir) = util::parent_dir(file) {
        cmd.current_dir(dir);
    }
    let out = match tokio::time::timeout(CHECK_TIMEOUT, cmd.output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(err)) => {
            tracing::warn!(%err, program, "diagnostics command failed to run");
            return None;
        }
        Err(_) => {
            tracing::warn!(program, "diagnostics command timed out");
            return None;
        }
    };
    if out.status.success() {
        return None;
    }
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.stderr.is_empty() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(&String::from_utf8_lossy(&out.stderr));
    }
    Some(combined)
}

/// Keep the last `max_lines` lines (diagnostics tools put the summary last),
/// marking any truncation explicitly.
fn tail_lines(text: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if max_lines == 0 || lines.len() <= max_lines {
        return text.trim_end().to_owned();
    }
    let mut out = String::from("[... earlier output truncated ...]\n");
    out.push_str(&lines[lines.len() - max_lines..].join("\n"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::{
        MessageId, SessionId, ToolCall, ToolCallId, ToolCallOrigin, ToolCallStatus, ToolCallTiming,
        ToolOutput, TurnId,
    };

    fn write_call(file: &str) -> ToolCall {
        ToolCall {
            id: ToolCallId::from("t"),
            session_id: SessionId::from("s"),
            turn_id: TurnId::from("turn"),
            message_id: MessageId::from("m"),
            tool_name: "Write".to_owned(),
            input: serde_json::json!({ "file_path": file }),
            read_only: false,
            origin: ToolCallOrigin::Model,
            status: ToolCallStatus::Running,
            timing: ToolCallTiming::default(),
            result: None,
        }
    }

    async fn run(hook: &DiagnosticsHook, file: &str) -> (HookOutcome, String) {
        let call = write_call(file);
        let mut output = ToolOutput::text("wrote file");
        let session = SessionId::from("s");
        let outcome = {
            let mut ctx = HookContext {
                session_id: &session,
                turn_id: None,
                data: HookData::ToolResult {
                    call: &call,
                    output: &mut output,
                },
                store: None,
            };
            hook.on(HookPoint::PostToolUse, &mut ctx)
                .await
                .expect("hook never errors")
        };
        (outcome, output.render_text())
    }

    fn shell_check(script: &str) -> CheckSpec {
        CheckSpec {
            extensions: vec!["rs".to_owned()],
            command: vec!["sh".to_owned(), "-c".to_owned(), script.to_owned()],
            env: Vec::new(),
            disabled: false,
        }
    }

    #[tokio::test]
    async fn appends_diagnostics_on_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("a.rs");
        std::fs::write(&file, "x").expect("seed");
        let hook = DiagnosticsHook::new(DiagnosticsConfig {
            enabled: true,
            checks: vec![shell_check("echo 'error: boom'; exit 1")],
            max_lines: DEFAULT_MAX_LINES,
        });
        assert!(hook.is_active());
        let (outcome, text) = run(&hook, file.to_str().expect("utf8")).await;
        assert_eq!(outcome, HookOutcome::Mutated);
        assert!(text.contains("error: boom"), "{text}");
        assert!(text.contains("Diagnostics after editing"), "{text}");
    }

    #[tokio::test]
    async fn no_append_when_check_passes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("a.rs");
        std::fs::write(&file, "x").expect("seed");
        let hook = DiagnosticsHook::new(DiagnosticsConfig {
            enabled: true,
            checks: vec![shell_check("exit 0")],
            max_lines: DEFAULT_MAX_LINES,
        });
        let (outcome, text) = run(&hook, file.to_str().expect("utf8")).await;
        assert_eq!(outcome, HookOutcome::Continue);
        assert_eq!(text, "wrote file");
    }

    #[tokio::test]
    async fn disabled_config_is_inert() {
        let hook = DiagnosticsHook::new(DiagnosticsConfig {
            enabled: false,
            checks: vec![shell_check("echo bad; exit 1")],
            max_lines: DEFAULT_MAX_LINES,
        });
        assert!(!hook.is_active());
        let (outcome, text) = run(&hook, "/tmp/a.rs").await;
        assert_eq!(outcome, HookOutcome::Continue);
        assert_eq!(text, "wrote file");
    }

    #[test]
    fn tail_lines_keeps_last_and_marks_truncation() {
        let text = "a\nb\nc\nd";
        assert_eq!(
            tail_lines(text, 2),
            "[... earlier output truncated ...]\nc\nd"
        );
        assert_eq!(tail_lines(text, 10), "a\nb\nc\nd");
    }
}
