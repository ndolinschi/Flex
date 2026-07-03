//! Human-readable tool call rows for the chat transcript.
//!
//! ```text
//! ⏺ Bash(npm run build)          ← status-colored glyph, spinner while running
//!   ⎿ added 128 packages in 3s   ← first result line
//!      npm notice ...            ← continuation, 5-space indent
//!   ⎿ … +47 lines (ctrl+o to expand)
//! ```

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use agentloop_contracts::{ToolCall, ToolCallStatus};

use crate::terminal_text::normalize_terminal_text;
use crate::theme;
use crate::tool_output::{
    DisplayBody, PREVIEW_MAX_LINES, call_is_expandable, collapsed_summary_line, display_body,
    parse_bash_result, tool_summary,
};

use super::diff::{DiffKind, DiffLine};
use super::highlight::Highlighter;

/// Duration is only worth showing above this threshold.
const DURATION_MIN_MS: u64 = 2000;

/// One logical tool row (may represent a collapsed failed streak).
#[derive(Debug, Clone)]
pub(super) struct ToolRow<'a> {
    pub call: &'a ToolCall,
    pub progress: Option<&'a str>,
    /// When > 1, consecutive identical failures were collapsed.
    pub failed_streak: usize,
    pub expanded: bool,
    pub focused: bool,
    /// Spinner tick for the running glyph.
    pub spinner: usize,
}

/// Build display lines for one tool row.
pub(super) fn render_tool_row(row: &ToolRow<'_>) -> Vec<Line<'static>> {
    let mut lines = vec![header_line(row)];

    if let Some(progress) = row.progress {
        lines.push(gutter_line(
            true,
            vec![Span::styled(normalize_terminal_text(progress), theme::dim())],
        ));
    }

    lines.extend(result_lines(row));
    lines
}

/// The `⏺ Tool(args)` header: status-colored glyph, plain summary, and a
/// small trailing badge only where it earns its place.
fn header_line(row: &ToolRow<'_>) -> Line<'static> {
    let call = row.call;
    let summary = tool_summary(&call.tool_name, &call.input);

    let glyph = match &call.status {
        ToolCallStatus::Running => Span::styled(
            format!("{} ", theme::spinner_frame(row.spinner)),
            theme::warn(),
        ),
        ToolCallStatus::Pending => Span::styled("⏺ ".to_owned(), theme::dim()),
        ToolCallStatus::AwaitingPermission { .. } => Span::styled("⏺ ".to_owned(), theme::warn()),
        ToolCallStatus::Completed => {
            if bash_exit_code(call).is_some_and(|code| code != 0) {
                Span::styled("⏺ ".to_owned(), theme::error())
            } else {
                Span::styled("⏺ ".to_owned(), theme::success())
            }
        }
        ToolCallStatus::Failed { .. } | ToolCallStatus::Denied { .. } => {
            Span::styled("⏺ ".to_owned(), theme::error())
        }
        ToolCallStatus::Cancelled => Span::styled("⏺ ".to_owned(), theme::dim()),
        _ => Span::styled("⏺ ".to_owned(), theme::dim()),
    };

    let summary_style = if row.focused {
        theme::selected()
    } else if matches!(call.status, ToolCallStatus::Running) {
        theme::tool_running()
    } else {
        Style::default()
    };

    let mut spans = vec![glyph, Span::styled(summary, summary_style)];

    match &call.status {
        ToolCallStatus::AwaitingPermission { .. } => {
            spans.push(Span::styled(" awaiting permission".to_owned(), theme::dim()));
        }
        ToolCallStatus::Completed => {
            if let Some(duration) = call.timing.duration_ms().filter(|ms| *ms > DURATION_MIN_MS) {
                spans.push(Span::styled(
                    format!(" {}", format_duration(duration)),
                    theme::dim(),
                ));
            }
        }
        ToolCallStatus::Failed { .. } if row.failed_streak > 1 => {
            spans.push(Span::styled(
                format!(" ×{} failed", row.failed_streak),
                theme::error(),
            ));
        }
        ToolCallStatus::Denied { .. } if row.failed_streak > 1 => {
            spans.push(Span::styled(
                format!(" ×{} denied", row.failed_streak),
                theme::error(),
            ));
        }
        ToolCallStatus::Cancelled => {
            spans.push(Span::styled(" cancelled".to_owned(), theme::dim()));
        }
        _ => {}
    }

    Line::from(spans)
}

/// The `⎿`-guttered result area: error line first, then the body preview,
/// then the expand/collapse footer.
fn result_lines(row: &ToolRow<'_>) -> Vec<Line<'static>> {
    let call = row.call;
    let mut content: Vec<Vec<Span<'static>>> = Vec::new();
    let mut footer: Option<String> = None;

    // Failure text moves from the header to the first result line.
    match &call.status {
        ToolCallStatus::Failed { error } => {
            content.push(vec![Span::styled(error.clone(), theme::error())]);
        }
        ToolCallStatus::Denied { reason } => {
            let reason = reason.clone().unwrap_or_else(|| "denied".to_owned());
            content.push(vec![Span::styled(reason, theme::error())]);
        }
        _ => {}
    }

    match display_body(&call.tool_name, &call.input, call.result.as_ref()) {
        DisplayBody::Diff(preview) => {
            let total = preview.lines.len();
            let shown = if row.expanded {
                total
            } else {
                preview.preview_len()
            };
            // Highlight context lines by the edited file's extension; `+`/`-`
            // lines keep their solid add/del color so the diff still reads as a
            // diff at a glance.
            let file_path = call.input.get("file_path").and_then(|value| value.as_str());
            let body_len = preview.lines.iter().map(|line| line.text.len() + 1).sum();
            let mut highlighter = super::highlight::for_path(file_path, body_len);
            for line in &preview.lines[..shown] {
                content.push(diff_spans(line, highlighter.as_mut()));
            }
            if !row.expanded && total > shown {
                footer = Some(format!("… +{} lines (ctrl+o to expand)", total - shown));
            } else if row.expanded && total > preview.preview_len() {
                footer = Some("(ctrl+o to collapse)".to_owned());
            }
        }
        DisplayBody::Lines(body) => {
            let summary = (!row.expanded)
                .then_some(call.result.as_ref())
                .flatten()
                .and_then(|result| collapsed_summary_line(&call.tool_name, result));
            match summary {
                Some(summary) => {
                    content.push(vec![Span::styled(summary, theme::dim())]);
                }
                None => {
                    let total = body.len();
                    let shown = if row.expanded {
                        total
                    } else {
                        total.min(PREVIEW_MAX_LINES)
                    };
                    // Bash success previews show the tail; everything else the head.
                    let tail = !row.expanded && bash_success_body(call);
                    let visible: Box<dyn Iterator<Item = &String>> = if tail {
                        Box::new(body.iter().skip(total - shown))
                    } else {
                        Box::new(body.iter().take(shown))
                    };
                    for line in visible {
                        content.push(vec![Span::styled(line.clone(), body_style(call, line))]);
                    }
                    if !row.expanded && total > shown {
                        footer = Some(format!("… +{} lines (ctrl+o to expand)", total - shown));
                    } else if row.expanded && call_is_expandable(call) {
                        footer = Some("(ctrl+o to collapse)".to_owned());
                    }
                }
            }
        }
    }

    let mut lines = Vec::with_capacity(content.len() + 1);
    for (idx, spans) in content.into_iter().enumerate() {
        lines.push(gutter_line(idx == 0, spans));
    }
    if let Some(footer) = footer {
        lines.push(gutter_line(true, vec![Span::styled(footer, theme::dim())]));
    }
    lines
}

/// Prefix a content line with the result gutter: `  ⎿ ` on anchor lines,
/// five spaces on continuations.
fn gutter_line(anchor: bool, mut spans: Vec<Span<'static>>) -> Line<'static> {
    let prefix = if anchor { "  ⎿ " } else { "     " };
    let mut all = vec![Span::styled(prefix.to_owned(), theme::dim())];
    all.append(&mut spans);
    Line::from(all)
}

fn diff_spans(line: &DiffLine, highlighter: Option<&mut Highlighter>) -> Vec<Span<'static>> {
    let no = line
        .line_no
        .map(|n| format!("{n:>3}"))
        .unwrap_or_else(|| "   ".to_owned());
    match line.kind {
        DiffKind::Del => vec![Span::styled(
            format!("{no} - {}", line.text),
            theme::diff_del(),
        )],
        DiffKind::Add => vec![Span::styled(
            format!("{no} + {}", line.text),
            theme::diff_add(),
        )],
        DiffKind::Ctx => {
            let mut spans = vec![Span::styled(format!("{no}   "), theme::dim())];
            match highlighter {
                Some(hl) => spans.extend(hl.line(&line.text)),
                None => spans.push(Span::styled(line.text.clone(), theme::dim())),
            }
            spans
        }
    }
}

/// Style for a plain body line: Bash failure `exit N` lines go red.
fn body_style(call: &ToolCall, line: &str) -> Style {
    if call.tool_name == "Bash"
        && line.starts_with("exit ")
        && bash_exit_code(call).is_some_and(|code| code != 0)
    {
        theme::error()
    } else {
        theme::dim()
    }
}

fn bash_exit_code(call: &ToolCall) -> Option<i32> {
    if call.tool_name != "Bash" {
        return None;
    }
    call.result
        .as_ref()
        .and_then(parse_bash_result)
        .and_then(|parts| parts.exit_code)
}

fn bash_success_body(call: &ToolCall) -> bool {
    call.tool_name == "Bash"
        && matches!(call.status, ToolCallStatus::Completed)
        && bash_exit_code(call).is_none_or(|code| code == 0)
        && !call.result.as_ref().is_some_and(|result| result.is_error)
}

/// Whether two tool calls are identical for failed-streak collapse.
pub(super) fn same_tool_identity(a: &ToolCall, b: &ToolCall) -> bool {
    a.tool_name == b.tool_name && a.input == b.input
}

/// Whether a tool call counts toward a failed streak.
pub(super) fn is_failed_streak_member(call: &ToolCall) -> bool {
    matches!(
        call.status,
        ToolCallStatus::Failed { .. } | ToolCallStatus::Denied { .. }
    )
}

fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::{
        MessageId, SessionId, ToolCall, ToolCallId, ToolCallOrigin, ToolCallStatus, ToolCallTiming,
        ToolOutput, TurnId,
    };

    fn sample_call(tool_name: &str, input: serde_json::Value, status: ToolCallStatus) -> ToolCall {
        ToolCall {
            id: ToolCallId::from("c1"),
            session_id: SessionId::from("s1"),
            turn_id: TurnId::from("t1"),
            message_id: MessageId::from("m1"),
            tool_name: tool_name.to_owned(),
            input,
            read_only: true,
            origin: ToolCallOrigin::Model,
            status,
            timing: ToolCallTiming::default(),
            result: None,
        }
    }

    fn bash_result(stdout: &str, stderr: &str, exit_code: i32) -> ToolOutput {
        ToolOutput {
            content: vec![agentloop_contracts::ToolResultBlock::markdown(format!(
                "exit_code: {exit_code}\n\nstdout:\n{stdout}\n\nstderr:\n{stderr}"
            ))],
            is_error: exit_code != 0,
            structured: Some(serde_json::json!({
                "exit_code": exit_code,
                "success": exit_code == 0,
            })),
        }
    }

    fn row<'a>(call: &'a ToolCall, expanded: bool) -> ToolRow<'a> {
        ToolRow {
            call,
            progress: None,
            failed_streak: 1,
            expanded,
            focused: false,
            spinner: 0,
        }
    }

    fn flat(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn running_row_shows_spinner_and_no_badge() {
        let call = sample_call(
            "Bash",
            serde_json::json!({"command": "cargo build"}),
            ToolCallStatus::Running,
        );
        let lines = render_tool_row(&row(&call, false));
        let text = flat(&lines);
        assert_eq!(text.len(), 1);
        assert!(text[0].starts_with(theme::SPINNER[0]));
        assert!(text[0].contains("Bash(cargo build)"));
        assert!(!text[0].contains("running"));
    }

    #[test]
    fn completed_hides_fast_duration() {
        let mut call = sample_call(
            "Read",
            serde_json::json!({"file_path": "src/main.rs"}),
            ToolCallStatus::Completed,
        );
        call.timing.started_at_ms = Some(0);
        call.timing.finished_at_ms = Some(500);
        call.result = Some(ToolOutput::text("done"));
        let lines = render_tool_row(&row(&call, false));
        let text = flat(&lines);
        assert!(!text[0].contains("ms"));
        assert!(!text[0].contains("completed"));
    }

    #[test]
    fn completed_shows_slow_duration() {
        let mut call = sample_call(
            "Bash",
            serde_json::json!({"command": "cargo test"}),
            ToolCallStatus::Completed,
        );
        call.timing.started_at_ms = Some(0);
        call.timing.finished_at_ms = Some(3400);
        call.result = Some(bash_result("ok", "", 0));
        let lines = render_tool_row(&row(&call, false));
        assert!(flat(&lines)[0].contains("3.4s"));
    }

    #[test]
    fn failed_error_moves_to_result_line() {
        let call = sample_call(
            "Bash",
            serde_json::json!({"command": "false"}),
            ToolCallStatus::Failed {
                error: "spawn failed".to_owned(),
            },
        );
        let lines = render_tool_row(&row(&call, false));
        let text = flat(&lines);
        assert!(!text[0].contains("spawn failed"));
        assert!(text[1].starts_with("  ⎿ "));
        assert!(text[1].contains("spawn failed"));
    }

    #[test]
    fn failed_streak_badge_shows_count() {
        let call = sample_call(
            "Bash",
            serde_json::json!({"command": "false"}),
            ToolCallStatus::Failed {
                error: "exit 1".to_owned(),
            },
        );
        let mut streak = row(&call, false);
        streak.failed_streak = 5;
        let lines = render_tool_row(&streak);
        assert!(flat(&lines)[0].contains("×5 failed"));
    }

    #[test]
    fn bash_success_preview_shows_tail() {
        let mut call = sample_call(
            "Bash",
            serde_json::json!({"command": "seq 6"}),
            ToolCallStatus::Completed,
        );
        call.result = Some(bash_result("1\n2\n3\n4\n5\n6", "", 0));
        let lines = render_tool_row(&row(&call, false));
        let text = flat(&lines);
        // Header + 4 tail lines + footer.
        assert_eq!(text.len(), 6);
        assert!(text[1].contains('3'));
        assert!(text[4].contains('6'));
        assert!(text[5].contains("+2 lines"));
        assert!(text[5].contains("ctrl+o to expand"));
    }

    #[test]
    fn bash_failure_shows_exit_head_first() {
        let mut call = sample_call(
            "Bash",
            serde_json::json!({"command": "cargo test"}),
            ToolCallStatus::Completed,
        );
        call.result = Some(bash_result("", "error[E0308]: mismatched types", 101));
        let lines = render_tool_row(&row(&call, false));
        let text = flat(&lines);
        assert!(text[1].contains("exit 101"));
        assert!(text[2].contains("error[E0308]"));
        assert!(text[2].starts_with("     "));
    }

    #[test]
    fn read_collapsed_shows_summary_line() {
        let mut call = sample_call(
            "Read",
            serde_json::json!({"file_path": "src/main.rs"}),
            ToolCallStatus::Completed,
        );
        call.result = Some(ToolOutput::text("line1\nline2\nline3\nline4\nline5"));
        let lines = render_tool_row(&row(&call, false));
        let text = flat(&lines);
        assert_eq!(text.len(), 2);
        assert_eq!(text[1], "  ⎿ Read 5 lines");
    }

    #[test]
    fn read_expanded_shows_all_lines() {
        let mut call = sample_call(
            "Read",
            serde_json::json!({"file_path": "src/main.rs"}),
            ToolCallStatus::Completed,
        );
        call.result = Some(ToolOutput::text("line1\nline2\nline3\nline4\nline5"));
        let lines = render_tool_row(&row(&call, true));
        let text = flat(&lines);
        // Header + 5 lines + collapse footer.
        assert_eq!(text.len(), 7);
        assert!(text[1].contains("line1"));
        assert!(text[6].contains("ctrl+o to collapse"));
    }

    #[test]
    fn edit_awaiting_permission_renders_diff() {
        let call = sample_call(
            "Edit",
            serde_json::json!({
                "file_path": "src/app.rs",
                "old_string": "let x = old();",
                "new_string": "let x = new();",
            }),
            ToolCallStatus::AwaitingPermission {
                request_id: agentloop_contracts::PermissionRequestId::from("p1"),
            },
        );
        let lines = render_tool_row(&row(&call, false));
        let text = flat(&lines);
        assert!(text[0].contains("awaiting permission"));
        assert!(text[1].contains("- let x = old();"));
        assert!(text[2].contains("+ let x = new();"));
    }

    #[test]
    fn bash_success_renders_without_labels() {
        let mut call = sample_call(
            "Bash",
            serde_json::json!({"command": "echo"}),
            ToolCallStatus::Completed,
        );
        call.result = Some(bash_result("hello\nworld", "", 0));
        let lines = render_tool_row(&row(&call, false));
        let text = flat(&lines).join("\n");
        assert!(!text.contains("stdout:"));
        assert!(!text.contains("exit_code"));
        assert!(text.contains("hello"));
    }

    #[test]
    fn bash_playwright_help_renders_one_line_per_row() {
        let help = concat!(
            "Usage: npx playwright [options] [command]\r\n",
            "\r\n",
            "Commands:\r\n",
            "  codegen [options] [url]\r\n",
            "  install [options] [browser...]\r\n",
        );
        let mut call = sample_call(
            "Bash",
            serde_json::json!({"command": "npx playwright --help"}),
            ToolCallStatus::Completed,
        );
        call.result = Some(bash_result(help, "", 0));
        let lines = render_tool_row(&row(&call, true));
        let text = flat(&lines);
        assert!(text[1].contains("Usage: npx playwright"));
        assert!(text.iter().any(|line| line.contains("codegen")));
        assert!(text.iter().any(|line| line.contains("install")));
        assert!(
            !text.iter().any(|line| line.contains('\r')),
            "carriage returns must not reach the renderer: {text:?}"
        );
    }
}
