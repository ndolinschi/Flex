#![allow(dead_code, unreachable_pub)]

use std::path::PathBuf;

use agentloop_contracts::*;

pub fn sample_meta() -> SessionMeta {
    SessionMeta {
        id: SessionId::from("sess-1"),
        title: Some("demo".to_owned()),
        agent_id: "native".to_owned(),
        parent_id: None,
        role: None,
        depth: 0,
        provider_session_id: None,
        cwd: PathBuf::from("/tmp/demo"),
        model: Some(ModelRef::from("anthropic/claude-sonnet-4-5")),
        fallback_models: Vec::new(),
        mode: None,
        isolation: None,
        workspace_id: None,
        executor: None,
        base_cwd: None,
        reuse_workspace_id: None,
        created_at_ms: 1_000,
        updated_at_ms: 1_000,
    }
}

pub fn sample_tool_call(status: ToolCallStatus) -> ToolCall {
    let finished = status.is_terminal();
    ToolCall {
        id: ToolCallId::from("call-1"),
        session_id: SessionId::from("sess-1"),
        turn_id: TurnId::from("turn-1"),
        message_id: MessageId::from("msg-2"),
        tool_name: "Read".to_owned(),
        input: serde_json::json!({"file_path": "/tmp/demo/hello.txt"}),
        read_only: true,
        origin: ToolCallOrigin::Model,
        status,
        timing: ToolCallTiming {
            queued_at_ms: 2_000,
            permission_wait_ms: None,
            started_at_ms: Some(2_010),
            finished_at_ms: finished.then_some(2_055),
        },
        result: finished.then(|| ToolOutput::text("hello world")),
    }
}

pub fn sample_event_log() -> Vec<AgentEvent> {
    vec![
        AgentEvent::SessionCreated {
            meta: sample_meta(),
        },
        AgentEvent::TurnStarted {
            turn_id: TurnId::from("turn-1"),
        },
        AgentEvent::UserMessage {
            message_id: MessageId::from("msg-1"),
            content: vec![ContentBlock::markdown("Read hello.txt for me")],
        },
        AgentEvent::AssistantMessage {
            message_id: MessageId::from("msg-2"),
            content: vec![
                ContentBlock::Thinking {
                    text: "The user wants the file contents. I should read it.".to_owned(),
                    signature: None,
                },
                ContentBlock::markdown("Reading the file."),
                ContentBlock::ToolUse {
                    id: ToolCallId::from("call-1"),
                    name: "Read".to_owned(),
                    input: serde_json::json!({"file_path": "/tmp/demo/hello.txt"}),
                },
            ],
            model: Some("claude-sonnet-4-5".to_owned()),
            usage: Some(TokenUsage {
                input: 120,
                output: 40,
                ..Default::default()
            }),
        },
        AgentEvent::ToolCallUpdated {
            call: sample_tool_call(ToolCallStatus::Pending),
        },
        AgentEvent::ToolCallUpdated {
            call: sample_tool_call(ToolCallStatus::Running),
        },
        AgentEvent::ToolCallUpdated {
            call: sample_tool_call(ToolCallStatus::Completed),
        },
        AgentEvent::UserMessage {
            message_id: MessageId::from("msg-3"),
            content: vec![ContentBlock::ToolResult {
                tool_use_id: ToolCallId::from("call-1"),
                content: vec![ToolResultBlock::markdown("hello world")],
                is_error: false,
            }],
        },
        AgentEvent::AssistantMessage {
            message_id: MessageId::from("msg-4"),
            content: vec![ContentBlock::markdown("The file contains: `hello world`.")],
            model: Some("claude-sonnet-4-5".to_owned()),
            usage: Some(TokenUsage {
                input: 180,
                output: 25,
                ..Default::default()
            }),
        },
        AgentEvent::TurnCompleted {
            turn_id: TurnId::from("turn-1"),
            summary: TurnSummary {
                turn_id: TurnId::from("turn-1"),
                stop_reason: TurnStopReason::EndTurn,
                usage: TokenUsage {
                    input: 300,
                    output: 65,
                    ..Default::default()
                },
                cost_usd: Some(0.0042),
                num_model_calls: 2,
                num_tool_calls: 1,
                duration_ms: 3_500,
            },
        },
    ]
}
