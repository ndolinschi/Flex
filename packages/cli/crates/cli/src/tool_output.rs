//! Tool result formatting for the chat transcript (shared by state and views).

use agentloop_contracts::ToolOutput;

pub(crate) const PREVIEW_MAX_LINES: usize = 3;

/// Parsed Bash tool output sections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BashParts {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Whether a tool result has more display lines than the collapsed preview.
pub(crate) fn result_is_expandable(tool_name: &str, result: &ToolOutput) -> bool {
    display_body_lines(tool_name, result).len() > PREVIEW_MAX_LINES
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
    text.lines().map(str::to_owned).collect()
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
            lines.extend(stderr.lines().map(str::to_owned));
        }
        let stdout = parts.stdout.trim();
        if !stdout.is_empty() {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.extend(stdout.lines().map(str::to_owned));
        }
        return lines;
    }

    let stdout = parts.stdout.trim();
    if !stdout.is_empty() {
        lines.extend(stdout.lines().map(str::to_owned));
    }
    let stderr = parts.stderr.trim();
    if !stderr.is_empty() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.extend(stderr.lines().map(str::to_owned));
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

    #[test]
    fn result_is_expandable_when_truncated() {
        let result = ToolOutput::text("a\nb\nc\nd");
        assert!(result_is_expandable("Read", &result));
        let short = ToolOutput::text("one line");
        assert!(!result_is_expandable("Read", &short));
    }
}
