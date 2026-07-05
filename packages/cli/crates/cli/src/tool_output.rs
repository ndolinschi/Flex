//! Tool result formatting for the chat transcript (shared by state and views).

use agentloop_contracts::{ToolCall, ToolOutput};

use crate::terminal_text::terminal_lines;
use crate::ui::diff::{DiffPreview, diff_preview};

pub(crate) const PREVIEW_MAX_LINES: usize = 4;

/// Unchanged lines kept around each hunk in Edit/Write diff previews.
const DIFF_CONTEXT: usize = 1;

/// Parsed Bash tool output sections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BashParts {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// What a tool row's result area displays.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DisplayBody {
    /// Plain text lines (Bash output, Read contents, ...).
    Lines(Vec<String>),
    /// A ± diff computed from the tool *input* (Edit/Write), so it renders
    /// even while the call is awaiting permission.
    Diff(DiffPreview),
}

/// The display body for a tool call, per the per-tool preview policy.
pub(crate) fn display_body(
    tool_name: &str,
    input: &serde_json::Value,
    result: Option<&ToolOutput>,
) -> DisplayBody {
    match tool_name {
        "Edit" => {
            let old = input.get("old_string").and_then(|v| v.as_str());
            let new = input.get("new_string").and_then(|v| v.as_str());
            if let (Some(old), Some(new)) = (old, new) {
                return DisplayBody::Diff(diff_preview(old, new, DIFF_CONTEXT));
            }
        }
        "Write" => {
            if let Some(content) = input.get("content").and_then(|v| v.as_str()) {
                return DisplayBody::Diff(diff_preview("", content, DIFF_CONTEXT));
            }
        }
        _ => {}
    }
    let lines = match result {
        Some(result) => display_body_lines(tool_name, result),
        None => Vec::new(),
    };
    DisplayBody::Lines(lines)
}

/// Build a diff preview from a permission prompt's JSON tool-input detail.
pub(crate) fn diff_from_permission_detail(
    title: &str,
    detail: Option<&str>,
) -> Option<DiffPreview> {
    let detail = detail?;
    let input: serde_json::Value = serde_json::from_str(detail).ok()?;
    let tool = title
        .strip_prefix("Allow `")
        .and_then(|rest| rest.strip_suffix("`?"))
        .unwrap_or("");
    match display_body(tool, &input, None) {
        DisplayBody::Diff(preview) => Some(preview),
        DisplayBody::Lines(_) => None,
    }
}

/// One-line collapsed summary for read-ish tools (`Read 214 lines`).
pub(crate) fn collapsed_summary_line(tool_name: &str, result: &ToolOutput) -> Option<String> {
    if result.is_error {
        return None;
    }
    let count = result.render_text().lines().count();
    match tool_name {
        "Read" => Some(format!("Read {count} {}", plural(count, "line", "lines"))),
        "Grep" => Some(format!(
            "Found {count} {}",
            plural(count, "match", "matches")
        )),
        "Glob" => Some(format!("Found {count} {}", plural(count, "file", "files"))),
        _ => None,
    }
}

fn plural<'a>(count: usize, one: &'a str, many: &'a str) -> &'a str {
    if count == 1 { one } else { many }
}

/// Whether expanding a tool row would reveal more than its collapsed preview.
pub(crate) fn call_is_expandable(call: &ToolCall) -> bool {
    match display_body(&call.tool_name, &call.input, call.result.as_ref()) {
        DisplayBody::Diff(preview) => preview.lines.len() > preview.preview_len(),
        DisplayBody::Lines(lines) => {
            let has_summary = call
                .result
                .as_ref()
                .and_then(|result| collapsed_summary_line(&call.tool_name, result))
                .is_some();
            if has_summary {
                lines.len() > 1
            } else {
                lines.len() > PREVIEW_MAX_LINES
            }
        }
    }
}

/// Flatten a tool result to display lines, with Bash-specific cleanup.
pub(crate) fn display_body_lines(tool_name: &str, result: &ToolOutput) -> Vec<String> {
    if tool_name == "Bash" {
        return bash_body_lines(result);
    }
    let text = result.render_text();
    if text.is_empty() {
        return Vec::new();
    }
    terminal_lines(&text)
}

const SUMMARY_MAX_LEN: usize = 72;

/// Human-readable one-line summary for a tool invocation (`Bash(cargo test)`).
pub(crate) fn tool_summary(tool_name: &str, input: &serde_json::Value) -> String {
    let detail = match tool_name {
        "Bash" => input
            .get("command")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        "Read" | "Write" | "Edit" => input
            .get("file_path")
            .or_else(|| input.get("path"))
            .and_then(|v| v.as_str())
            .map(short_path),
        "Grep" => {
            let pattern = input.get("pattern").and_then(|v| v.as_str());
            let path = input
                .get("path")
                .or_else(|| input.get("glob"))
                .and_then(|v| v.as_str());
            match (pattern, path) {
                (Some(p), Some(path)) => Some(format!("{p} in {}", short_path(path))),
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

/// Abbreviate a filesystem path to `folder/filename` for compact tool-row
/// display: `/Users/x/proj/food-website/README.md` → `food-website/README.md`.
/// Paths of one or two components pass through unchanged.
fn short_path(path: &str) -> String {
    let parts: Vec<&str> = path
        .trim_end_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    match parts.as_slice() {
        [] => path.to_owned(),
        [only] => (*only).to_owned(),
        [.., parent, name] => format!("{parent}/{name}"),
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

/// Largest `end <= max` that is a char boundary of `text`, so a following
/// `&text[..end]` never panics on multi-byte UTF-8 (Cyrillic, em-dash, emoji,
/// pasted non-ASCII input). A raw byte-index slice would crash mid-character.
fn char_boundary_end(text: &str, max: usize) -> usize {
    let mut end = max.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

fn compact_json(value: &serde_json::Value) -> String {
    let text = value.to_string();
    if text.len() > 60 {
        format!("{}...", &text[..char_boundary_end(&text, 57)])
    } else {
        text
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_owned()
    } else {
        format!(
            "{}...",
            &text[..char_boundary_end(text, max.saturating_sub(3))]
        )
    }
}

/// Body lines for Bash: stdout only on success; stderr prominent on failure.
pub(crate) fn bash_body_lines(result: &ToolOutput) -> Vec<String> {
    let Some(parts) = parse_bash_result(result) else {
        return Vec::new();
    };

    let success = parts
        .exit_code
        .map(|code| code == 0)
        .unwrap_or(!result.is_error);

    let mut lines = Vec::new();

    if !success {
        if let Some(code) = parts.exit_code {
            lines.push(format!("exit {code}"));
        }
        let stderr = parts.stderr.trim();
        if !stderr.is_empty() {
            lines.extend(terminal_lines(stderr));
        }
        let stdout = parts.stdout.trim();
        if !stdout.is_empty() {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.extend(terminal_lines(stdout));
        }
        return lines;
    }

    let stdout = parts.stdout.trim();
    if !stdout.is_empty() {
        lines.extend(terminal_lines(stdout));
    }
    let stderr = parts.stderr.trim();
    if !stderr.is_empty() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.extend(terminal_lines(stderr));
    }
    lines
}

/// Extract stdout/stderr from a Bash tool result, hiding wire-format labels.
pub(crate) fn parse_bash_result(result: &ToolOutput) -> Option<BashParts> {
    let text = result.render_text();
    if text.is_empty() && result.structured.is_none() {
        return None;
    }

    let mut parts = parse_bash_text(&text);
    if let Some(structured) = &result.structured {
        if let Some(code) = structured.get("exit_code").and_then(json_exit_code) {
            parts.exit_code = Some(code);
        }
    }
    Some(parts)
}

fn parse_bash_text(text: &str) -> BashParts {
    let mut exit_code = None;
    let mut stdout = String::new();
    let mut stderr = String::new();

    let mut section: Option<&str> = None;
    let mut body = String::new();

    for line in text.lines() {
        if let Some(code) = line.strip_prefix("exit_code:") {
            flush_bash_section(&mut section, &mut body, &mut stdout, &mut stderr);
            let trimmed = code.trim();
            exit_code = trimmed.parse().ok();
            if trimmed == "terminated_by_signal" {
                exit_code = Some(-1);
            }
            continue;
        }
        if line == "stdout:" {
            flush_bash_section(&mut section, &mut body, &mut stdout, &mut stderr);
            section = Some("stdout");
            body.clear();
            continue;
        }
        if line == "stderr:" {
            flush_bash_section(&mut section, &mut body, &mut stdout, &mut stderr);
            section = Some("stderr");
            body.clear();
            continue;
        }
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(line);
    }
    flush_bash_section(&mut section, &mut body, &mut stdout, &mut stderr);

    if stdout.is_empty() && stderr.is_empty() && exit_code.is_none() {
        stdout = text.to_owned();
    }

    BashParts {
        exit_code,
        stdout,
        stderr,
    }
}

fn flush_bash_section(
    section: &mut Option<&str>,
    body: &mut String,
    stdout: &mut String,
    stderr: &mut String,
) {
    let Some(kind) = section.take() else {
        return;
    };
    match kind {
        "stdout" => *stdout = body.trim_end().to_owned(),
        "stderr" => *stderr = body.trim_end().to_owned(),
        _ => {}
    }
    body.clear();
}

fn json_exit_code(value: &serde_json::Value) -> Option<i32> {
    value
        .as_i64()
        .and_then(|code| i32::try_from(code).ok())
        .or_else(|| value.as_u64().and_then(|code| i32::try_from(code).ok()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::ToolOutput;

    #[test]
    fn truncate_multibyte_without_panicking() {
        // A summary whose cut byte lands inside a multi-byte char must not panic.
        let s = "Bash(— это очень длинная команда на русском языке с тире —, повторяем чтобы точно превысить лимит)";
        let out = truncate(s, 72); // must not panic
        assert!(out.ends_with("..."), "long text should be truncated: {out}");
    }

    #[test]
    fn compact_json_truncates_multibyte_without_panicking() {
        // A long value whose 57th byte lands inside a multi-byte char (em-dash
        // / Cyrillic) must not panic on the truncation slice.
        let value = serde_json::json!(
            "— это очень длинная строка на русском языке с тире —, повторяем ещё раз чтобы точно превысить лимит"
        );
        let out = compact_json(&value); // must not panic
        assert!(
            out.ends_with("..."),
            "long value should be truncated: {out}"
        );
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
    fn bash_success_hides_stdout_label() {
        let result = bash_result("hello\nworld", "", 0);
        let lines = bash_body_lines(&result);
        assert_eq!(lines, vec!["hello", "world"]);
    }

    #[test]
    fn bash_failure_shows_exit_and_stderr() {
        let result = bash_result("", "permission denied", 1);
        let lines = bash_body_lines(&result);
        assert_eq!(lines, vec!["exit 1", "permission denied"]);
    }

    fn sample_call(
        tool_name: &str,
        input: serde_json::Value,
        result: Option<ToolOutput>,
    ) -> ToolCall {
        use agentloop_contracts::{
            MessageId, SessionId, ToolCallId, ToolCallOrigin, ToolCallStatus, ToolCallTiming,
            TurnId,
        };
        ToolCall {
            id: ToolCallId::from("c1"),
            session_id: SessionId::from("s1"),
            turn_id: TurnId::from("t1"),
            message_id: MessageId::from("m1"),
            tool_name: tool_name.to_owned(),
            input,
            read_only: true,
            origin: ToolCallOrigin::Model,
            status: ToolCallStatus::Completed,
            timing: ToolCallTiming::default(),
            result,
        }
    }

    #[test]
    fn read_result_expandable_beyond_summary() {
        let call = sample_call(
            "Read",
            serde_json::json!({"file_path": "src/lib.rs"}),
            Some(ToolOutput::text("a\nb")),
        );
        assert!(call_is_expandable(&call));
        let short = sample_call(
            "Read",
            serde_json::json!({"file_path": "src/lib.rs"}),
            Some(ToolOutput::text("one line")),
        );
        assert!(!call_is_expandable(&short));
    }

    #[test]
    fn bash_result_expandable_beyond_preview() {
        let call = sample_call(
            "Bash",
            serde_json::json!({"command": "ls"}),
            Some(bash_result("a\nb\nc\nd\ne", "", 0)),
        );
        assert!(call_is_expandable(&call));
        let short = sample_call(
            "Bash",
            serde_json::json!({"command": "ls"}),
            Some(bash_result("a\nb", "", 0)),
        );
        assert!(!call_is_expandable(&short));
    }

    #[test]
    fn edit_without_result_shows_diff_from_input() {
        let call = sample_call(
            "Edit",
            serde_json::json!({
                "file_path": "src/app.rs",
                "old_string": "let x = old();",
                "new_string": "let x = new();",
            }),
            None,
        );
        let body = display_body(&call.tool_name, &call.input, None);
        let DisplayBody::Diff(preview) = body else {
            panic!("expected diff body");
        };
        assert_eq!(preview.lines.len(), 2);
        assert!(!call_is_expandable(&call));
    }

    #[test]
    fn write_input_diffs_as_whole_file_insert() {
        let content = "a\nb\nc\nd\ne\nf";
        let body = display_body(
            "Write",
            &serde_json::json!({"file_path": "new.rs", "content": content}),
            None,
        );
        let DisplayBody::Diff(preview) = body else {
            panic!("expected diff body");
        };
        assert_eq!(preview.lines.len(), 6);
        assert_eq!(preview.preview_len(), PREVIEW_MAX_LINES);
    }

    #[test]
    fn collapsed_summary_counts_lines() {
        let read = ToolOutput::text("1|a\n2|b\n3|c");
        assert_eq!(
            collapsed_summary_line("Read", &read).as_deref(),
            Some("Read 3 lines")
        );
        let single = ToolOutput::text("only");
        assert_eq!(
            collapsed_summary_line("Read", &single).as_deref(),
            Some("Read 1 line")
        );
        let grep = ToolOutput::text("src/a.rs:1:hit");
        assert_eq!(
            collapsed_summary_line("Grep", &grep).as_deref(),
            Some("Found 1 match")
        );
        assert_eq!(collapsed_summary_line("Bash", &read), None);
    }

    #[test]
    fn collapsed_summary_skips_errors() {
        let error = ToolOutput {
            content: vec![agentloop_contracts::ToolResultBlock::markdown(
                "file not found",
            )],
            is_error: true,
            structured: None,
        };
        assert_eq!(collapsed_summary_line("Read", &error), None);
    }

    #[test]
    fn bash_summary_shows_command() {
        let summary = tool_summary("Bash", &serde_json::json!({"command": "cargo test"}));
        assert_eq!(summary, "Bash(cargo test)");
    }

    #[test]
    fn read_summary_abbreviates_absolute_path() {
        let summary = tool_summary(
            "Read",
            &serde_json::json!({"file_path": "/Users/x/proj/food-website/README.md"}),
        );
        assert_eq!(summary, "Read(food-website/README.md)");
    }

    #[test]
    fn short_path_keeps_folder_and_file() {
        assert_eq!(short_path("src/main.rs"), "src/main.rs");
        assert_eq!(short_path("README.md"), "README.md");
        assert_eq!(short_path("/etc/hosts"), "etc/hosts");
        assert_eq!(short_path("/a/b/c/d.rs"), "c/d.rs");
    }
}
