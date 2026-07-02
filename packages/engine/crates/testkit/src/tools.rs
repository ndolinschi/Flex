//! Simple [`Tool`] implementations for exercising the loop's tool pipeline:
//! a happy-path echo, a deterministic failure, and a cancellable sleeper.

use std::time::Duration;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_core::contracts::ToolOutput;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

/// Derive a JSON Schema for a tool input type (mirrors what `typed_tool`
/// does; a schema derivation failure degrades to an open object).
fn schema_of<I: JsonSchema>() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(I))
        .unwrap_or_else(|_| serde_json::json!({"type": "object"}))
}

fn parse_input<I: serde::de::DeserializeOwned>(
    tool_name: &str,
    example: &str,
    input: serde_json::Value,
) -> Result<I, ToolError> {
    serde_json::from_value(input).map_err(|err| {
        ToolError::InvalidInput(format!(
            "Input for `{tool_name}` does not match its schema: {err}. \
             Call it as {example} and retry."
        ))
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
struct EchoInput {
    /// The text to echo back verbatim.
    text: String,
}

/// `echo` — returns the provided text unchanged. The happy-path round-trip
/// tool: what the model sends is exactly what comes back.
#[derive(Debug, Default, Clone, Copy)]
pub struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "echo".to_owned(),
            description: "Echo the given text back verbatim. \
                          Example: {\"text\": \"ping\"} returns `ping`."
                .to_owned(),
            input_schema: schema_of::<EchoInput>(),
            read_only: true,
            category: ToolCategory::Other,
            needs_permission: PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        _ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let parsed: EchoInput = parse_input("echo", "{\"text\": \"<string>\"}", input)?;
        Ok(ToolOutput::text(parsed.text))
    }
}

/// `fail` — always returns a tool-level execution error. Use it to test how
/// the loop feeds failed results back to the model.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailingTool;

#[async_trait]
impl Tool for FailingTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "fail".to_owned(),
            description: "Always fails with an execution error. \
                          Takes no input: call it as {}."
                .to_owned(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            read_only: true,
            category: ToolCategory::Other,
            needs_permission: PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        _ctx: ToolContext,
        _input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        Err(ToolError::Execution("deliberate test failure".to_owned()))
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SlowInput {
    /// How long to sleep, in milliseconds.
    ms: u64,
}

/// `slow` — sleeps for the requested duration, honoring cancellation. Use it
/// to test interrupts racing in-flight tool calls.
#[derive(Debug, Default, Clone, Copy)]
pub struct SlowTool;

#[async_trait]
impl Tool for SlowTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "slow".to_owned(),
            description: "Sleep for the given number of milliseconds, then return \"slept\". \
                          Example: {\"ms\": 50}."
                .to_owned(),
            input_schema: schema_of::<SlowInput>(),
            read_only: true,
            category: ToolCategory::Other,
            needs_permission: PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let parsed: SlowInput = parse_input("slow", "{\"ms\": <milliseconds>}", input)?;
        tokio::select! {
            _ = ctx.cancel.cancelled() => Err(ToolError::Cancelled),
            _ = tokio::time::sleep(Duration::from_millis(parsed.ms)) => {
                Ok(ToolOutput::text("slept"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_core::EventSink;
    use agentloop_core::contracts::{SessionId, ToolCallId, TurnId};
    use tokio_util::sync::CancellationToken;

    fn ctx_with(cancel: CancellationToken) -> ToolContext {
        ToolContext {
            session_id: SessionId::generate(),
            turn_id: TurnId::generate(),
            call_id: ToolCallId::generate(),
            cwd: std::env::temp_dir(),
            cancel,
            events: EventSink::disconnected(),
        }
    }

    fn ctx() -> ToolContext {
        ctx_with(CancellationToken::new())
    }

    #[tokio::test]
    async fn echo_returns_the_input_text() {
        let output = EchoTool
            .run(ctx(), serde_json::json!({"text": "ping"}))
            .await
            .expect("echo succeeds on valid input");
        assert!(!output.is_error);
        assert_eq!(output.render_text(), "ping");
    }

    #[tokio::test]
    async fn echo_teaches_on_invalid_input() {
        let err = EchoTool
            .run(ctx(), serde_json::json!({"txet": "typo"}))
            .await
            .expect_err("missing `text` field must fail");
        match err {
            ToolError::InvalidInput(message) => {
                assert!(message.contains("echo"), "names the tool: {message}");
                assert!(
                    message.contains("retry"),
                    "teaches the next step: {message}"
                );
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn failing_tool_always_fails() {
        let err = FailingTool
            .run(ctx(), serde_json::json!({}))
            .await
            .expect_err("fail tool must fail");
        assert!(matches!(
            err,
            ToolError::Execution(message) if message == "deliberate test failure"
        ));
    }

    #[tokio::test]
    async fn slow_tool_sleeps_then_returns() {
        let output = SlowTool
            .run(ctx(), serde_json::json!({"ms": 1}))
            .await
            .expect("short sleep completes");
        assert_eq!(output.render_text(), "slept");
    }

    #[tokio::test]
    async fn slow_tool_honors_cancellation() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let err = SlowTool
            .run(ctx_with(cancel), serde_json::json!({"ms": 60_000}))
            .await
            .expect_err("cancelled sleep must not complete");
        assert!(matches!(err, ToolError::Cancelled));
    }

    #[tokio::test]
    async fn descriptors_expose_expected_names_and_read_only() {
        for (tool, name) in [
            (&EchoTool as &dyn Tool, "echo"),
            (&FailingTool as &dyn Tool, "fail"),
            (&SlowTool as &dyn Tool, "slow"),
        ] {
            let descriptor = tool.descriptor();
            assert_eq!(descriptor.name, name);
            assert!(descriptor.read_only);
        }
    }
}
