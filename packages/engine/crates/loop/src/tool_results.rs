//! Shared synthetic tool-result text for incomplete tool calls.

use agentloop_contracts::{ToolCallStatus, ToolOutput, ToolResultBlock};

pub(crate) fn output_or_synthetic(
    output: Option<&ToolOutput>,
    status: &ToolCallStatus,
    incomplete_message: &'static str,
    failed_as_tool_error: bool,
) -> (Vec<ToolResultBlock>, bool) {
    match output {
        Some(output) => (output.content.clone(), output.is_error),
        None => synthetic(status, incomplete_message, failed_as_tool_error),
    }
}

fn synthetic(
    status: &ToolCallStatus,
    incomplete_message: &'static str,
    failed_as_tool_error: bool,
) -> (Vec<ToolResultBlock>, bool) {
    let text = match status {
        ToolCallStatus::Denied { reason } => format!(
            "Permission denied{}",
            reason
                .as_deref()
                .map(|reason| format!(": {reason}"))
                .unwrap_or_default()
        ),
        ToolCallStatus::Failed { error } if failed_as_tool_error => {
            format!("Tool failed: {error}")
        }
        _ => incomplete_message.to_owned(),
    };
    (vec![ToolResultBlock::markdown(text)], true)
}
