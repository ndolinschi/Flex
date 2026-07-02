//! Golden snapshots of ratatui rendering via [`TestBackend`].

use agentloop_cli_core::AgentKind;
use agentloop_contracts::{
    AgentEvent, ContentBlock, MessageId, ToolCall, ToolCallId, ToolCallOrigin, ToolCallStatus,
    ToolCallTiming, TurnId,
};
use insta::assert_snapshot;

use crate::app::TurnPhase;
use crate::overlay::{
    Overlay, PickerAction, PickerItem, PickerState, ShellCommandOverlay, ShellCommandPhase,
};
use crate::testing::{render_app, test_app, test_bootstrap};

fn assistant_markdown(text: &str) -> AgentEvent {
    AgentEvent::AssistantMessage {
        message_id: MessageId::from("msg-asst"),
        content: vec![ContentBlock::markdown(text)],
        model: Some("anthropic/claude-sonnet-4-5".to_owned()),
        usage: None,
    }
}

fn sample_tool_call(status: ToolCallStatus) -> ToolCall {
    let finished = status.is_terminal();
    ToolCall {
        id: ToolCallId::from("call-1"),
        session_id: agentloop_contracts::SessionId::from("sess-test"),
        turn_id: TurnId::from("turn-1"),
        message_id: MessageId::from("msg-tool"),
        tool_name: "Read".to_owned(),
        input: serde_json::json!({"file_path": "src/main.rs"}),
        read_only: true,
        origin: ToolCallOrigin::Model,
        status,
        timing: ToolCallTiming {
            queued_at_ms: 1_000,
            permission_wait_ms: None,
            started_at_ms: Some(1_010),
            finished_at_ms: finished.then_some(1_050),
        },
        result: finished.then(|| agentloop_contracts::ToolOutput::text("done")),
    }
}

#[test]
fn chat_markdown_snapshot() {
    let mut app = test_app(test_bootstrap());
    app.chat.apply(&AgentEvent::UserMessage {
        message_id: MessageId::from("msg-user"),
        content: vec![ContentBlock::markdown("show me a heading")],
    });
    app.chat.apply(&assistant_markdown(
        "# Demo\n\nThis is **bold** and `inline`.\n\n```rust\nfn main() {}\n```",
    ));
    app.chat.finalize_drafts();

    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("chat_markdown", rendered);
}

#[test]
fn chat_table_snapshot() {
    let mut app = test_app(test_bootstrap());
    app.chat.apply(&AgentEvent::UserMessage {
        message_id: MessageId::from("msg-user"),
        content: vec![ContentBlock::markdown("show a table")],
    });
    app.chat.apply(&assistant_markdown(
        "Summary:\n\n| Name | Count |\n| --- | ---: |\n| alpha | 1 |\n| beta | 42 |\n",
    ));
    app.chat.finalize_drafts();

    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("chat_table", rendered);
}

#[test]
fn chat_tool_row_snapshot() {
    let mut app = test_app(test_bootstrap());
    app.chat.apply(&AgentEvent::ToolCallUpdated {
        call: sample_tool_call(ToolCallStatus::Running),
    });
    app.chat.apply(&AgentEvent::ToolCallUpdated {
        call: sample_tool_call(ToolCallStatus::Completed),
    });

    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("chat_tool_row", rendered);
}

#[test]
fn chat_thinking_snapshot() {
    let mut app = test_app(test_bootstrap());
    app.caps.reasoning_visible = true;
    app.show_thinking = true;
    app.chat.apply(&AgentEvent::MessageStarted {
        message_id: MessageId::from("msg-think"),
        role: agentloop_contracts::Role::Assistant,
    });
    app.chat.apply(&AgentEvent::ThinkingDelta {
        message_id: MessageId::from("msg-think"),
        text: "Let me reason about this step by step.\nFirst, check the module layout.".to_owned(),
    });
    app.chat.apply(&assistant_markdown("Here is the answer."));
    app.chat.finalize_drafts();

    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("chat_thinking", rendered);
}

#[test]
fn chat_thinking_streaming_snapshot() {
    let mut app = test_app(test_bootstrap());
    app.caps.reasoning_visible = true;
    app.show_thinking = true;
    app.session.turn = TurnPhase::Running {
        started: std::time::Instant::now(),
    };
    app.chat.apply(&AgentEvent::MessageStarted {
        message_id: MessageId::from("msg-stream"),
        role: agentloop_contracts::Role::Assistant,
    });
    app.chat.apply(&AgentEvent::ThinkingDelta {
        message_id: MessageId::from("msg-stream"),
        text: "Let me reason about this step by step.\nFirst, check the module layout.".to_owned(),
    });

    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("chat_thinking_streaming", rendered);
}

#[test]
fn status_idle_snapshot() {
    let mut app = test_app(test_bootstrap());
    app.kind = AgentKind::Native;
    app.session.model = Some(agentloop_contracts::ModelRef::from(
        "anthropic/claude-sonnet-4-5",
    ));
    app.session.permission_mode = agentloop_contracts::PermissionMode::AcceptEdits;
    app.session.session_mode = crate::app::SessionMode::Code;
    app.session.turn = TurnPhase::Idle;
    app.status.total_usage.input = 120;
    app.status.total_usage.output = 45;

    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("status_idle", rendered);
}

#[test]
fn overlay_model_picker_snapshot() {
    let mut app = test_app(test_bootstrap());
    app.overlay = Overlay::Picker(PickerState::new(
        "models",
        vec![
            PickerItem {
                id: "anthropic/claude-sonnet-4-5".to_owned(),
                label: "claude-sonnet-4-5".to_owned(),
                detail: Some("anthropic".to_owned()),
                enabled: true,
            },
            PickerItem {
                id: "copilot/gpt-4.1".to_owned(),
                label: "gpt-4.1".to_owned(),
                detail: Some("copilot".to_owned()),
                enabled: true,
            },
            PickerItem {
                id: "ollama/llama3".to_owned(),
                label: "llama3".to_owned(),
                detail: Some("ollama".to_owned()),
                enabled: true,
            },
        ],
        PickerAction::SetModel,
    ));

    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("overlay_model_picker", rendered);
}

#[test]
fn overlay_shell_command_done_snapshot() {
    let mut app = test_app(test_bootstrap());
    app.overlay = Overlay::ShellCommand(ShellCommandOverlay {
        command: "echo hello".to_owned(),
        phase: ShellCommandPhase::Done {
            output: "hello\nworld".to_owned(),
            exit_code: Some(0),
        },
        scroll: 0,
    });

    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("overlay_shell_command_done", rendered);
}

#[test]
fn chat_follow_shows_last_line() {
    let mut app = test_app(test_bootstrap());
    let marker = "ZZZZ_LAST_LINE_MARKER";
    let body = (0..60)
        .map(|i| format!("Line {i}: filler text for scroll testing"))
        .chain(std::iter::once(format!(
            "## {marker}\n\nFinal paragraph after heading."
        )))
        .collect::<Vec<_>>()
        .join("\n");
    app.chat.apply(&AgentEvent::UserMessage {
        message_id: MessageId::from("msg-user"),
        content: vec![ContentBlock::markdown("scroll test")],
    });
    app.chat.apply(&assistant_markdown(&body));
    app.chat.finalize_drafts();
    app.chat.scroll.scroll_to_bottom();

    let rendered = render_app(&mut app, 80, 30);
    assert!(
        rendered.contains(marker),
        "follow mode should show the last heading; rendered:\n{rendered}"
    );
    assert!(
        rendered.contains("Final paragraph after heading"),
        "content below the last heading should be visible; rendered:\n{rendered}"
    );
}
