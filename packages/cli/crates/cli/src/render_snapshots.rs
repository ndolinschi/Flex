//! Golden snapshots of ratatui rendering via [`TestBackend`].

use agentloop_cli_core::AgentKind;
use agentloop_contracts::{
    AgentEvent, ContentBlock, MessageId, PlanEntry, PlanStatus, ToolCall, ToolCallId,
    ToolCallOrigin, ToolCallStatus, ToolCallTiming, TurnId,
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

fn custom_tool_call(
    id: &str,
    tool_name: &str,
    input: serde_json::Value,
    status: ToolCallStatus,
    result: Option<agentloop_contracts::ToolOutput>,
) -> ToolCall {
    ToolCall {
        id: ToolCallId::from(id),
        session_id: agentloop_contracts::SessionId::from("sess-test"),
        turn_id: TurnId::from("turn-1"),
        message_id: MessageId::from("msg-tool"),
        tool_name: tool_name.to_owned(),
        input,
        read_only: false,
        origin: ToolCallOrigin::Model,
        status,
        timing: ToolCallTiming::default(),
        result,
    }
}

fn bash_output(stdout: &str, stderr: &str, exit_code: i32) -> agentloop_contracts::ToolOutput {
    agentloop_contracts::ToolOutput {
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
fn tool_row_bash_running_snapshot() {
    let mut app = test_app(test_bootstrap());
    app.status.spinner = 0;
    app.chat.apply(&AgentEvent::ToolCallUpdated {
        call: custom_tool_call(
            "call-bash",
            "Bash",
            serde_json::json!({"command": "npm run build"}),
            ToolCallStatus::Running,
            None,
        ),
    });

    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("tool_row_bash_running", rendered);
}

#[test]
fn tool_row_bash_failed_tail_snapshot() {
    let mut app = test_app(test_bootstrap());
    app.chat.apply(&AgentEvent::ToolCallUpdated {
        call: custom_tool_call(
            "call-bash-fail",
            "Bash",
            serde_json::json!({"command": "cargo test"}),
            ToolCallStatus::Completed,
            Some(bash_output(
                "",
                "error[E0308]: mismatched types\n --> src/main.rs:4:5\nnote: expected `u32`\nhelp: try into()\nerror: aborting due to previous error",
                101,
            )),
        ),
    });

    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("tool_row_bash_failed_tail", rendered);
}

#[test]
fn tool_row_edit_diff_snapshot() {
    let mut app = test_app(test_bootstrap());
    app.chat.apply(&AgentEvent::ToolCallUpdated {
        call: custom_tool_call(
            "call-edit",
            "Edit",
            serde_json::json!({
                "file_path": "src/app.rs",
                "old_string": "fn main() {\n    let x = old();\n    run(x);\n    done();\n    after();\n}",
                "new_string": "fn main() {\n    let x = new();\n    run(x);\n    done();\n    later();\n}",
            }),
            ToolCallStatus::Completed,
            Some(agentloop_contracts::ToolOutput::text("edited src/app.rs")),
        ),
    });

    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("tool_row_edit_diff", rendered);
}

#[test]
fn tool_row_read_summary_snapshot() {
    let mut app = test_app(test_bootstrap());
    app.chat.apply(&AgentEvent::ToolCallUpdated {
        call: custom_tool_call(
            "call-read",
            "Read",
            serde_json::json!({"file_path": "src/main.rs"}),
            ToolCallStatus::Completed,
            Some(agentloop_contracts::ToolOutput::text(
                "fn main() {}\nfn helper() {}\nfn other() {}\nfn more() {}\nfn last() {}",
            )),
        ),
    });

    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("tool_row_read_summary", rendered);
}

#[test]
fn thinking_streaming_borderless_snapshot() {
    let mut app = test_app(test_bootstrap());
    app.caps.reasoning_visible = true;
    app.show_thinking = true;
    app.status.spinner = 0;
    app.session.turn = TurnPhase::Running {
        started: std::time::Instant::now(),
    };
    app.chat.apply(&AgentEvent::MessageStarted {
        message_id: MessageId::from("msg-borderless"),
        role: agentloop_contracts::Role::Assistant,
    });
    app.chat.apply(&AgentEvent::ThinkingDelta {
        message_id: MessageId::from("msg-borderless"),
        text: "line one\nline two\nline three\nline four\nline five\nline six".to_owned(),
    });

    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("thinking_streaming_borderless", rendered);
}

#[test]
fn thinking_collapsed_duration_snapshot() {
    let mut app = test_app(test_bootstrap());
    app.caps.reasoning_visible = true;
    app.show_thinking = true;
    app.chat.apply(&AgentEvent::MessageStarted {
        message_id: MessageId::from("msg-done"),
        role: agentloop_contracts::Role::Assistant,
    });
    app.chat.apply(&AgentEvent::ThinkingDelta {
        message_id: MessageId::from("msg-done"),
        text: "considering the options".to_owned(),
    });
    app.chat.apply(&assistant_markdown("Here is the answer."));
    app.chat.finalize_drafts();
    // Pin the measured duration for a stable snapshot.
    for item in &mut app.chat.items {
        if let crate::chat::ChatItem::Assistant { blocks, .. } = item {
            for block in blocks {
                if let crate::chat::DraftBlock::Thinking { duration_ms, .. } = block {
                    *duration_ms = Some(12_000);
                }
            }
        }
    }

    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("thinking_collapsed_duration", rendered);
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
fn status_plan_mode_shows_single_plan() {
    let mut app = test_app(test_bootstrap());
    app.kind = AgentKind::Native;
    app.session.session_mode = crate::app::SessionMode::Plan;
    app.session.turn = TurnPhase::Idle;

    let rendered = render_app(&mut app, 80, 24);
    // Plan mode forces the effective permission to "plan" too; the status bar
    // must not render the redundant "plan · plan".
    assert!(
        !rendered.contains("plan · plan"),
        "status bar duplicated plan:\n{rendered}"
    );
    assert!(
        rendered.contains("native · plan · "),
        "expected a single plan segment (kind · plan · model):\n{rendered}"
    );
}

#[test]
fn status_shows_auto_badge_for_deepseek() {
    let mut app = test_app(test_bootstrap());
    app.kind = AgentKind::Native;
    app.session.turn = TurnPhase::Idle;

    // A DeepSeek model engages the flash/pro split → a `ds-auto` badge shows.
    app.session.model = Some(agentloop_contracts::ModelRef::from(
        "deepseek/deepseek-v4-pro",
    ));
    let rendered = render_app(&mut app, 80, 24);
    assert!(
        rendered.contains("deepseek/deepseek-v4-pro · ds-auto"),
        "expected a ds-auto badge after the DeepSeek model:\n{rendered}"
    );

    // A non-DeepSeek model must not show it (and `ds-auto` must never collide
    // with the accept-edits permission label, which renders as `auto`).
    app.session.model = Some(agentloop_contracts::ModelRef::from(
        "anthropic/claude-sonnet-5",
    ));
    let rendered = render_app(&mut app, 80, 24);
    assert!(
        !rendered.contains("ds-auto"),
        "ds-auto badge must not show for non-deepseek models:\n{rendered}"
    );
}

#[test]
fn busy_line_running_snapshot() {
    let mut app = test_app(test_bootstrap());
    app.status.spinner = 0;
    app.status.turn_verb_idx = 0;
    app.status.turn_output_chars = 4_900; // ≈ 1.2k tokens
    app.session.turn = TurnPhase::Running {
        started: std::time::Instant::now(),
    };

    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("busy_line_running", rendered);
}

#[test]
fn toast_line_snapshot() {
    let mut app = test_app(test_bootstrap());
    app.toast("model set to copilot/gpt-4.1");

    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("toast_line", rendered);
}

#[test]
fn status_bar_context_snapshot() {
    let mut app = test_app(test_bootstrap());
    app.session.model = Some(agentloop_contracts::ModelRef::from(
        "anthropic/claude-sonnet-4-5",
    ));
    app.catalog = vec![agentloop_cli_core::CatalogEntry {
        provider: agentloop_contracts::ProviderId::from("anthropic"),
        model: agentloop_contracts::ModelInfo {
            id: "claude-sonnet-4-5".to_owned(),
            display_name: None,
            context_window: Some(200_000),
            reasoning: true,
            vision: false,
        },
    }];
    app.status.last_context_tokens = Some(94_000); // 47%
    app.status.total_usage.input = 12_300;
    app.status.total_usage.output = 4_100;
    app.status.last_cost_usd = Some(0.0421);

    // Wider than the default 80 so the cost suffix stays visible.
    let rendered = render_app(&mut app, 100, 24);
    assert_snapshot!("status_bar_context", rendered);
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
fn sidebar_activity_snapshot() {
    let mut app = test_app(test_bootstrap());
    app.chat.apply(&AgentEvent::ToolCallUpdated {
        call: sample_tool_call(ToolCallStatus::Running),
    });
    for event in subagent_events("running") {
        app.chat.apply(&event);
    }
    app.chat.apply(&AgentEvent::PlanUpdated {
        entries: vec![
            PlanEntry {
                content: "explore the repo".to_owned(),
                status: PlanStatus::Completed,
            },
            PlanEntry {
                content: "summarize findings".to_owned(),
                status: PlanStatus::InProgress,
            },
        ],
    });
    // Wide enough to trigger the sidebar (>= 90 cols).
    let rendered = render_app(&mut app, 110, 24);
    assert_snapshot!("sidebar_activity", rendered);
}

#[test]
fn splash_snapshot() {
    let mut app = test_app(test_bootstrap());
    let rendered = render_app(&mut app, 80, 12);
    assert_snapshot!("splash", rendered);
}

#[test]
fn overlay_theme_picker_snapshot() {
    let mut app = test_app(test_bootstrap());
    let items = crate::theme::BuiltinTheme::all()
        .iter()
        .map(|builtin| PickerItem {
            id: builtin.id().to_owned(),
            label: builtin.id().to_owned(),
            detail: (builtin.id() == "tokyonight").then(|| "current".to_owned()),
            enabled: true,
        })
        .collect();
    app.overlay = Overlay::Picker(PickerState::new("theme", items, PickerAction::SetTheme));
    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("overlay_theme_picker", rendered);
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

fn subagent_events(outcome: &str) -> Vec<AgentEvent> {
    use agentloop_contracts::{SessionId, TokenUsage, TurnStopReason, TurnSummary};
    let child = SessionId::from("child-1");
    let mut events = vec![
        AgentEvent::ToolCallUpdated {
            call: custom_tool_call(
                "call-task",
                "Task",
                serde_json::json!({"role": "searcher", "description": "map session event flow"}),
                ToolCallStatus::Running,
                None,
            ),
        },
        AgentEvent::SubagentStarted {
            child_session: child.clone(),
            task: "map session event flow".to_owned(),
            call_id: Some(ToolCallId::from("call-task")),
            role: Some("searcher".to_owned()),
        },
        AgentEvent::SubagentEvent {
            child_session: child.clone(),
            event: Box::new(AgentEvent::ToolCallUpdated {
                call: custom_tool_call(
                    "child-call-1",
                    "Grep",
                    serde_json::json!({"pattern": "emit_persistent"}),
                    ToolCallStatus::Running,
                    None,
                ),
            }),
        },
        AgentEvent::SubagentEvent {
            child_session: child.clone(),
            event: Box::new(AgentEvent::AssistantMessage {
                message_id: MessageId::from("child-m1"),
                content: Vec::new(),
                model: Some("deepseek/deepseek-chat".to_owned()),
                usage: Some(TokenUsage {
                    output: 12_400,
                    ..TokenUsage::default()
                }),
            }),
        },
    ];
    let stop_reason = match outcome {
        "done" => Some(TurnStopReason::EndTurn),
        "failed" => Some(TurnStopReason::Error),
        _ => None,
    };
    if let Some(stop_reason) = stop_reason {
        events.push(AgentEvent::SubagentCompleted {
            child_session: child,
            summary: TurnSummary {
                turn_id: TurnId::from("child-t1"),
                stop_reason,
                usage: TokenUsage {
                    output: 31_200,
                    ..TokenUsage::default()
                },
                cost_usd: None,
                num_model_calls: 3,
                num_tool_calls: 9,
                duration_ms: 42_000,
            },
        });
    }
    events
}

#[test]
fn overlay_permission_child_badge_snapshot() {
    use agentloop_contracts::{PermissionDecisionKind, PermissionRequestId, SessionId};
    let mut app = test_app(test_bootstrap());
    app.overlay = Overlay::Permission(crate::overlay::PermissionPrompt {
        id: PermissionRequestId::from("perm-1"),
        call_id: Some(ToolCallId::from("child-call-1")),
        title: "Allow `Bash`?".to_owned(),
        detail: Some("cargo test --workspace".to_owned()),
        options: vec![
            PermissionDecisionKind::AllowOnce,
            PermissionDecisionKind::AllowAlways,
            PermissionDecisionKind::Deny,
        ],
        selected: 0,
        session: Some(SessionId::from("child-1")),
        role: Some("worker".to_owned()),
    });
    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("overlay_permission_child_badge", rendered);
}

#[test]
fn subagent_tree_running_snapshot() {
    let mut app = test_app(test_bootstrap());
    for event in subagent_events("running") {
        app.chat.apply(&event);
    }
    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("subagent_tree_running", rendered);
}

#[test]
fn subagent_tree_done_snapshot() {
    let mut app = test_app(test_bootstrap());
    for event in subagent_events("done") {
        app.chat.apply(&event);
    }
    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("subagent_tree_done", rendered);
}

#[test]
fn subagent_tree_failed_snapshot() {
    let mut app = test_app(test_bootstrap());
    for event in subagent_events("failed") {
        app.chat.apply(&event);
    }
    let rendered = render_app(&mut app, 80, 24);
    assert_snapshot!("subagent_tree_failed", rendered);
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
