//! Markdown projection: render any transcript as one continuous, readable
//! markdown document. This is the debug view, the runner's `--render md`
//! output, and the seed-history text for delegated agents that only accept
//! plain prompts. Deterministic — snapshot-tested.

use std::fmt::Write as _;

use crate::content::Role;
use crate::reduce::{Transcript, TranscriptBlock};
use crate::tool_call::{ToolCall, ToolCallStatus};

/// Render a transcript to a single markdown document.
pub fn transcript_to_markdown(transcript: &Transcript) -> String {
    let mut out = String::new();
    for (index, item) in transcript.items.iter().enumerate() {
        if transcript.boundary_index == Some(index) {
            if let Some(compaction) = &transcript.compaction {
                let _ = writeln!(out, "---\n");
                let _ = writeln!(
                    out,
                    "> **Context compacted** ({}) — earlier conversation summarized:\n>",
                    compaction.strategy
                );
                for line in compaction.summary_markdown.lines() {
                    let _ = writeln!(out, "> {line}");
                }
                out.push('\n');
            }
        }

        let role = match item.role {
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::System => "System",
        };
        let _ = writeln!(out, "## {role}\n");

        for block in &item.blocks {
            match block {
                TranscriptBlock::Markdown { text } => {
                    let _ = writeln!(out, "{text}\n");
                }
                TranscriptBlock::Thinking { text, .. } => {
                    let _ = writeln!(out, "```thinking");
                    let _ = writeln!(out, "{text}");
                    let _ = writeln!(out, "```\n");
                }
                TranscriptBlock::ToolCall(call) => {
                    render_tool_call(&mut out, call);
                }
                TranscriptBlock::Image { media_type, .. } => {
                    let _ = writeln!(out, "_[image: {media_type}]_\n");
                }
                TranscriptBlock::File {
                    name, media_type, ..
                } => {
                    let _ = writeln!(out, "_[file: {name} ({media_type})]_\n");
                }
                TranscriptBlock::Opaque { provider, .. } => {
                    let _ = writeln!(out, "_[provider-specific block: {provider}]_\n");
                }
            }
        }
    }
    out
}

fn render_tool_call(out: &mut String, call: &ToolCall) {
    let status = match &call.status {
        ToolCallStatus::Pending => "pending".to_owned(),
        ToolCallStatus::AwaitingPermission { .. } => "awaiting permission".to_owned(),
        ToolCallStatus::Running => "running".to_owned(),
        ToolCallStatus::Completed => match call.timing.duration_ms() {
            Some(ms) => format!("completed in {ms}ms"),
            None => "completed".to_owned(),
        },
        ToolCallStatus::Failed { error } => format!("failed: {error}"),
        ToolCallStatus::Denied { reason } => match reason {
            Some(reason) => format!("denied: {reason}"),
            None => "denied".to_owned(),
        },
        ToolCallStatus::Cancelled => "cancelled".to_owned(),
    };
    let _ = writeln!(out, "**Tool: {}** · {status}\n", call.tool_name);

    let input =
        serde_json::to_string_pretty(&call.input).unwrap_or_else(|_| call.input.to_string());
    let _ = writeln!(out, "```json\n{input}\n```\n");

    if let Some(result) = &call.result {
        let heading = if result.is_error {
            "Result (error):"
        } else {
            "Result:"
        };
        let _ = writeln!(out, "{heading}\n");
        let _ = writeln!(out, "````\n{}\n````\n", result.render_text());
    }
}
