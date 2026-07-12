//! `FormatOnEditHook` — run a code formatter after `Write`/`Edit`.

use std::process::Stdio;
use std::time::Duration;

use agentloop_contracts::{HookPoint, ToolResultBlock};
use agentloop_core::{Hook, HookContext, HookData, HookError, HookOutcome};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::util;

/// A formatter run is bounded: a wedged formatter must not stall a turn.
const FORMAT_TIMEOUT: Duration = Duration::from_secs(30);

/// One formatter: the extensions it handles and the command to run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatterSpec {
    /// File extensions (without the dot) this formatter applies to, e.g.
    /// `["rs"]` or `["ts", "tsx"]`.
    pub extensions: Vec<String>,
    /// Command argv. The literal token `$FILE` is replaced with the edited
    /// file's absolute path (e.g. `["rustfmt", "$FILE"]`,
    /// `["prettier", "--write", "$FILE"]`).
    pub command: Vec<String>,
    /// Extra environment variables for the formatter process.
    #[serde(default)]
    pub env: Vec<(String, String)>,
    /// When true, this spec is ignored.
    #[serde(default)]
    pub disabled: bool,
}

impl FormatterSpec {
    fn handles(&self, ext: &str) -> bool {
        !self.disabled
            && !self.command.is_empty()
            && self.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
    }
}

/// Formats files after `Write`/`Edit`. Never blocks or fails the loop: a
/// formatter that is not on `$PATH`, times out, or exits non-zero is a silent
/// no-op (a failed format must not derail a correct edit).
pub struct FormatOnEditHook {
    specs: Vec<FormatterSpec>,
    interests: Vec<HookPoint>,
}

impl FormatOnEditHook {
    pub fn new(specs: Vec<FormatterSpec>) -> Self {
        Self {
            specs,
            interests: vec![HookPoint::PostToolUse],
        }
    }

    /// Whether any enabled spec carries a command — if not, there is nothing to
    /// register and the composition root can drop the hook.
    pub fn is_active(&self) -> bool {
        self.specs
            .iter()
            .any(|spec| !spec.disabled && !spec.command.is_empty())
    }
}

#[async_trait]
impl Hook for FormatOnEditHook {
    fn interests(&self) -> &[HookPoint] {
        &self.interests
    }

    async fn on(
        &self,
        point: HookPoint,
        ctx: &mut HookContext<'_>,
    ) -> Result<HookOutcome, HookError> {
        if point != HookPoint::PostToolUse {
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
            .specs
            .iter()
            .find(|spec| spec.handles(&ext) && util::program_on_path(&spec.command[0]))
        else {
            return Ok(HookOutcome::Continue);
        };

        let argv = util::substitute_file(&spec.command, &file);
        if run_formatter(&argv, &spec.env, &file).await {
            output.content.push(ToolResultBlock::markdown(format!(
                "_Formatted `{file}` with `{}`._",
                spec.command[0]
            )));
            Ok(HookOutcome::Mutated)
        } else {
            Ok(HookOutcome::Continue)
        }
    }
}

/// Run the formatter, discarding its stdio (formatters mutate the file in
/// place; their stdout must never leak into the engine's own stdout stream).
/// Returns whether it exited successfully.
async fn run_formatter(argv: &[String], env: &[(String, String)], file: &str) -> bool {
    let Some((program, args)) = argv.split_first() else {
        return false;
    };
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    for (key, value) in env {
        cmd.env(key, value);
    }
    if let Some(dir) = util::parent_dir(file) {
        cmd.current_dir(dir);
    }
    match tokio::time::timeout(FORMAT_TIMEOUT, cmd.status()).await {
        Ok(Ok(status)) => status.success(),
        Ok(Err(err)) => {
            tracing::warn!(%err, program, "formatter failed to run; skipping");
            false
        }
        Err(_) => {
            tracing::warn!(program, "formatter timed out; skipping");
            false
        }
    }
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

    async fn run(hook: &FormatOnEditHook, call: &ToolCall) -> (HookOutcome, String) {
        let mut output = ToolOutput::text("wrote file");
        let session = SessionId::from("s");
        let outcome = {
            let mut ctx = HookContext {
                session_id: &session,
                turn_id: None,
                data: HookData::ToolResult {
                    call,
                    output: &mut output,
                },
                store: None,
                events: None,
            };
            hook.on(HookPoint::PostToolUse, &mut ctx)
                .await
                .expect("hook never errors")
        };
        (outcome, output.render_text())
    }

    #[tokio::test]
    async fn formats_matching_extension_and_notes_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("a.rs");
        std::fs::write(&file, "fn main(){}").expect("seed file");
        let hook = FormatOnEditHook::new(vec![FormatterSpec {
            extensions: vec!["rs".to_owned()],
            command: vec!["true".to_owned(), "$FILE".to_owned()],
            env: Vec::new(),
            disabled: false,
        }]);
        assert!(hook.is_active());
        let (outcome, text) = run(&hook, &write_call(file.to_str().expect("utf8"))).await;
        assert_eq!(outcome, HookOutcome::Mutated);
        assert!(text.contains("Formatted"), "{text}");
    }

    #[tokio::test]
    async fn skips_when_binary_not_on_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("a.rs");
        std::fs::write(&file, "x").expect("seed file");
        let hook = FormatOnEditHook::new(vec![FormatterSpec {
            extensions: vec!["rs".to_owned()],
            command: vec![
                "definitely-not-a-real-binary-xyz".to_owned(),
                "$FILE".to_owned(),
            ],
            env: Vec::new(),
            disabled: false,
        }]);
        let (outcome, text) = run(&hook, &write_call(file.to_str().expect("utf8"))).await;
        assert_eq!(outcome, HookOutcome::Continue);
        assert_eq!(text, "wrote file");
    }

    #[tokio::test]
    async fn skips_unmatched_extension_and_non_edit_tools() {
        let hook = FormatOnEditHook::new(vec![FormatterSpec {
            extensions: vec!["rs".to_owned()],
            command: vec!["true".to_owned()],
            env: Vec::new(),
            disabled: false,
        }]);
        let (outcome, _) = run(&hook, &write_call("/tmp/a.py")).await;
        assert_eq!(outcome, HookOutcome::Continue);
        let mut read = write_call("/tmp/a.rs");
        read.tool_name = "Read".to_owned();
        let (outcome, _) = run(&hook, &read).await;
        assert_eq!(outcome, HookOutcome::Continue);
    }
}
