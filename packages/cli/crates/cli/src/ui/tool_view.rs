//! Human-readable tool call rows for the chat transcript.

use ratatui::text::{Line, Span};

use agentloop_contracts::{ToolCall, ToolCallStatus, ToolOutput};

use crate::theme;
use crate::tool_output::{PREVIEW_MAX_LINES, display_body_lines, parse_bash_result};

const SUMMARY_MAX_LEN: usize = 72;

/// One logical tool row (may represent a collapsed failed streak).
#[derive(Debug, Clone)]
pub(super) struct ToolRow<'a> {
    pub call: &'a ToolCall,
    pub progress: Option<&'a str>,
    /// When > 1, consecutive identical failures were collapsed.
    pub failed_streak: usize,
    pub expanded: bool,
    pub focused: bool,
}

/// Build display lines for one tool row.
pub(super) fn render_tool_row(row: &ToolRow<'_>) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let summary = tool_summary(&row.call.tool_name, &row.call.input);
    let status = status_badge(
        &row.call.tool_name,
        &row.call.status,
        &row.call.timing,
        row.failed_streak,
        row.call.result.as_ref(),
    );

    let header_style = if row.focused {
        theme::SELECTED
    } else {
        theme::TOOL
    };
    lines.push(Line::from(vec![
        Span::styled("⏺ ", header_style),
        Span::styled(summary, header_style),
        Span::raw(" "),
        status,
    ]));

    if let Some(progress) = row.progress {
        lines.push(Line::from(Span::styled(
            format!("  {progress}"),
            theme::DIM,
        )));
    }

    if let Some(result) = &row.call.result {
        lines.extend(result_display_lines(
            &row.call.tool_name,
            result,
            &row.call.status,
            row.expanded,
        ));
    }

    lines
}

/// Human-readable one-line summary for a tool invocation.
pub(super) fn tool_summary(tool_name: &str, input: &serde_json::Value) -> String {
    let detail = match tool_name {
        "Bash" => input
            .get("command")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        "Read" | "Write" | "Edit" => input
            .get("file_path")
            .or_else(|| input.get("path"))
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        "Grep" => {
            let pattern = input.get("pattern").and_then(|v| v.as_str());
            let path = input
                .get("path")
                .or_else(|| input.get("glob"))
                .and_then(|v| v.as_str());
            match (pattern, path) {
                (Some(p), Some(path)) => Some(format!("{p} in {path}")),
                (Some(p), None) => Some(p.to_owned()),
                _ => None,
            }
        }
        "Glob" => input
            .get("glob")
            .or_else(|| input.get("pattern"))
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        "WebFetch" => input.get("url").and_then(|v| v.as_str()).map(webfetch_host),
        _ => first_scalar_field(input),
    };

    match detail {
        Some(detail) => truncate(&format!("{tool_name}({detail})"), SUMMARY_MAX_LEN),
        None => truncate(
            &format!("{tool_name}({})", compact_json(input)),
            SUMMARY_MAX_LEN,
        ),
    }
}

/// Whether a tool result has more lines than the collapsed preview shows.
pub(super) fn result_display_lines(
    tool_name: &str,
    result: &ToolOutput,
    status: &ToolCallStatus,
    expanded: bool,
) -> Vec<Line<'static>> {
    let body_lines = display_body_lines(tool_name, result);
    if body_lines.is_empty() {
        return Vec::new();
    }

    let total = body_lines.len();
    let shown = if expanded {
        total
    } else {
        total.min(PREVIEW_MAX_LINES)
    };

    let mut lines = Vec::with_capacity(shown + 1);
    for line in &body_lines[..shown] {
        lines.push(Line::from(Span::styled(format!("  {line}"), theme::DIM)));
    }

    if !expanded && total > PREVIEW_MAX_LINES {
        let more = total - PREVIEW_MAX_LINES;
        lines.push(Line::from(vec![
            Span::styled(format!("  … (+{more} more)"), theme::DIM),
            Span::styled(" · Enter/Space expand", theme::DIM),
        ]));
    } else if expanded && total > PREVIEW_MAX_LINES {
        lines.push(Line::from(Span::styled(
            "  · Enter/Space collapse",
            theme::DIM,
        )));
    }

    if tool_name == "Bash" && matches!(status, ToolCallStatus::Failed { .. }) {
        if let Some(parts) = parse_bash_result(result) {
            if let Some(code) = parts.exit_code.filter(|code| *code != 0) {
                if parts.stderr.trim().is_empty() && parts.stdout.trim().is_empty() {
                    lines.insert(
                        0,
                        Line::from(Span::styled(format!("  exit {code}"), theme::ERROR)),
                    );
                }
            }
        }
    }

    lines
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

fn status_badge(
    tool_name: &str,
    status: &ToolCallStatus,
    timing: &agentloop_contracts::ToolCallTiming,
    failed_streak: usize,
    result: Option<&ToolOutput>,
) -> Span<'static> {
    let duration = timing
        .duration_ms()
        .map(format_duration)
        .unwrap_or_default();

    let (label, style) = match status {
        ToolCallStatus::Pending => ("pending", theme::DIM),
        ToolCallStatus::AwaitingPermission { .. } => ("awaiting permission", theme::WARN),
        ToolCallStatus::Running => ("running", theme::WARN),
        ToolCallStatus::Completed => {
            if tool_name == "Bash" {
                if let Some(result) = result {
                    if let Some(parts) = parse_bash_result(result) {
                        if let Some(code) = parts.exit_code.filter(|code| *code != 0) {
                            return Span::styled(format!("exit {code} {duration}"), theme::ERROR);
                        }
                    }
                }
            }
            if duration.is_empty() {
                ("completed", theme::SUCCESS)
            } else {
                return Span::styled(format!("completed {duration}"), theme::SUCCESS);
            }
        }
        ToolCallStatus::Failed { error } => {
            if failed_streak > 1 {
                return Span::styled(format!("×{failed_streak} failed: {error}"), theme::ERROR);
            }
            return Span::styled(format!("failed: {error}"), theme::ERROR);
        }
        ToolCallStatus::Denied { reason } => {
            let reason = reason.as_deref().unwrap_or("denied");
            if failed_streak > 1 {
                return Span::styled(format!("×{failed_streak} denied: {reason}"), theme::ERROR);
            }
            return Span::styled(format!("denied: {reason}"), theme::ERROR);
        }
        ToolCallStatus::Cancelled => ("cancelled", theme::ERROR),
        _ => ("unknown", theme::DIM),
    };
    Span::styled(label.to_owned(), style)
}

fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

fn webfetch_host(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(url)
        .to_owned()
}

fn first_scalar_field(input: &serde_json::Value) -> Option<String> {
    input.as_object().and_then(|obj| {
        obj.values().find_map(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            serde_json::Value::Bool(b) => Some(b.to_string()),
            _ => None,
        })
    })
}

fn compact_json(value: &serde_json::Value) -> String {
    let text = value.to_string();
    if text.len() > 60 {
        format!("{}...", &text[..57])
    } else {
        text
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_owned()
    } else {
        format!("{}...", &text[..max.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::{
        MessageId, SessionId, ToolCall, ToolCallId, ToolCallOrigin, ToolCallStatus, ToolCallTiming,
        TurnId,
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

    #[test]
    fn bash_summary_shows_command() {
        let summary = tool_summary(
            "Bash",
            &serde_json::json!({"command": "cd packages/cli && cargo test"}),
        );
        assert!(summary.contains("Bash("));
        assert!(summary.contains("cargo test"));
    }

    #[test]
    fn read_summary_shows_path() {
        let summary = tool_summary("Read", &serde_json::json!({"file_path": "src/main.rs"}));
        assert_eq!(summary, "Read(src/main.rs)");
    }

    #[test]
    fn result_preview_truncates_extra_lines() {
        let result = ToolOutput::text("line1\nline2\nline3\nline4\nline5");
        let lines = result_display_lines("Read", &result, &ToolCallStatus::Completed, false);
        assert_eq!(lines.len(), 4);
        assert!(lines[3].spans.iter().any(|s| s.content.contains("+2 more")));
        assert!(lines[3].spans.iter().any(|s| s.content.contains("expand")));
    }

    #[test]
    fn result_expanded_shows_all_lines() {
        let result = ToolOutput::text("line1\nline2\nline3\nline4\nline5");
        let lines = result_display_lines("Read", &result, &ToolCallStatus::Completed, true);
        assert_eq!(lines.len(), 6);
        assert!(lines[5].spans[0].content.contains("collapse"));
    }

    #[test]
    fn bash_success_renders_without_labels() {
        let result = bash_result("hello\nworld", "", 0);
        let rendered = result_display_lines("Bash", &result, &ToolCallStatus::Completed, false);
        let text: String = rendered
            .iter()
            .flat_map(|line| line.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(!text.contains("stdout:"));
        assert!(!text.contains("exit_code"));
        assert!(text.contains("hello"));
    }

    #[test]
    fn bash_nonzero_exit_badge_on_completed_status() {
        let mut call = sample_call(
            "Bash",
            serde_json::json!({"command": "false"}),
            ToolCallStatus::Completed,
        );
        call.result = Some(bash_result("", "error", 1));
        let row = ToolRow {
            call: &call,
            progress: None,
            failed_streak: 1,
            expanded: false,
            focused: false,
        };
        let lines = render_tool_row(&row);
        assert!(lines[0].spans.iter().any(|s| s.content.contains("exit 1")));
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
        let row = ToolRow {
            call: &call,
            progress: None,
            failed_streak: 5,
            expanded: false,
            focused: false,
        };
        let lines = render_tool_row(&row);
        assert!(
            lines[0]
                .spans
                .iter()
                .any(|s| s.content.contains("×5 failed"))
        );
    }
}
