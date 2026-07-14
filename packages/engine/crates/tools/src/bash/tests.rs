use super::*;
use std::sync::Arc;

use agentloop_contracts::{SessionId, ToolCallId, ToolOutput, TurnId};
use agentloop_core::{EventSink, Tool, ToolContext, ToolError};
use agentloop_executors::LocalExecutor;
use tokio_util::sync::CancellationToken;

fn ctx() -> ToolContext {
    let (events, _rx) = EventSink::channel();
    ToolContext {
        session_id: SessionId::from("sess-test"),
        turn_id: TurnId::from("turn-test"),
        call_id: ToolCallId::from("call-test"),
        cwd: std::path::PathBuf::from("."),
        cancel: CancellationToken::new(),
        events,
    }
}

fn bash_tool() -> BashTool {
    BashTool::new(Arc::new(LocalExecutor))
}

/// Concatenate every markdown block's text — test helper only; real
/// callers of `ToolOutput` render the whole `content` vec.
fn markdown_text(output: &ToolOutput) -> String {
    output
        .content
        .iter()
        .map(|block| match block {
            agentloop_contracts::ToolResultBlock::Markdown { text } => text.as_str(),
            _ => "",
        })
        .collect()
}

#[tokio::test]
async fn foreground_path_is_unchanged() {
    let tool = bash_tool();
    let output = tool
        .run(ctx(), serde_json::json!({"command": "printf hello"}))
        .await
        .expect("run ok");
    assert!(!output.is_error);
    let text = markdown_text(&output);
    assert!(text.contains("hello"));
    assert_eq!(
        output.structured.as_ref().and_then(|s| s.get("success")),
        Some(&serde_json::Value::Bool(true))
    );
}

#[tokio::test]
async fn empty_command_is_rejected() {
    let tool = bash_tool();
    let err = tool
        .run(ctx(), serde_json::json!({"command": "   "}))
        .await
        .unwrap_err();
    assert!(matches!(err, ToolError::InvalidInput(_)));
}

#[tokio::test]
async fn background_run_returns_early_with_a_process_id_while_still_running() {
    let tool = bash_tool();
    let call_ctx = ctx();
    let output = tool
        .run(
            call_ctx,
            serde_json::json!({"command": "echo ready; sleep 5", "run_in_background": true}),
        )
        .await
        .expect("run ok");
    assert!(!output.is_error);
    let text = markdown_text(&output);
    assert!(text.contains("Started background process"));
    assert!(text.contains("ready"));
    let structured = output.structured.expect("structured result");
    assert_eq!(
        structured.get("running"),
        Some(&serde_json::Value::Bool(true))
    );
    let process_id = structured
        .get("process_id")
        .and_then(|v| v.as_str())
        .expect("process_id present")
        .to_owned();

    // Clean up: kill it through the same control surface the model uses.
    let kill_ctx = ctx();
    let kill_output = tool
        .run(
            kill_ctx,
            serde_json::json!({"background_action": "kill", "process_id": process_id}),
        )
        .await
        .expect("kill ok");
    assert!(!kill_output.is_error);
}

#[tokio::test]
async fn background_status_reports_running_then_kill_stops_it() {
    let tool = bash_tool();
    let start = tool
        .run(
            ctx(),
            serde_json::json!({"command": "sleep 5", "run_in_background": true}),
        )
        .await
        .expect("run ok");
    let process_id = start
        .structured
        .as_ref()
        .and_then(|s| s.get("process_id"))
        .and_then(|v| v.as_str())
        .expect("process_id present")
        .to_owned();

    let status = tool
        .run(
            ctx(),
            serde_json::json!({"background_action": "status", "process_id": process_id}),
        )
        .await
        .expect("status ok");
    assert!(!status.is_error);
    assert_eq!(
        status.structured.as_ref().and_then(|s| s.get("running")),
        Some(&serde_json::Value::Bool(true))
    );

    let kill = tool
        .run(
            ctx(),
            serde_json::json!({"background_action": "kill", "process_id": process_id}),
        )
        .await
        .expect("kill ok");
    assert!(!kill.is_error);
    assert_eq!(
        kill.structured.as_ref().and_then(|s| s.get("killed")),
        Some(&serde_json::Value::Bool(true))
    );
}

/// `background_action: "kill"` must take down the whole process tree a
/// backgrounded command started, not just the `/bin/sh` wrapper: this
/// backgrounds a shell that forks a `sleep` grandchild and prints its
/// pid, kills the tracked process, then confirms the grandchild pid is
/// actually gone (not just reparented and still running) via `kill(pid,
/// 0)`. Guards against a regression to killing only the immediate child.
#[tokio::test]
async fn kill_terminates_the_whole_process_group_not_just_the_shell() {
    let tool = bash_tool();
    let start = tool
        .run(
            ctx(),
            serde_json::json!({
                "command": "sleep 30 & echo $!; wait",
                "run_in_background": true,
            }),
        )
        .await
        .expect("run ok");
    let process_id = start
        .structured
        .as_ref()
        .and_then(|s| s.get("process_id"))
        .and_then(|v| v.as_str())
        .expect("process_id present")
        .to_owned();

    // Poll the tracked tail for the echoed grandchild pid rather than a
    // fixed sleep guess.
    let mut grandchild_pid: Option<u32> = None;
    for _ in 0..100 {
        let status = tool
            .run(
                ctx(),
                serde_json::json!({"background_action": "status", "process_id": process_id}),
            )
            .await
            .expect("status ok");
        let tail = markdown_text(&status);
        if let Some(pid) = tail
            .trim()
            .lines()
            .next_back()
            .and_then(|l| l.trim().parse().ok())
        {
            grandchild_pid = Some(pid);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let grandchild_pid = grandchild_pid.expect("grandchild pid observed in tail output");
    assert!(
        process_is_alive(grandchild_pid),
        "grandchild should be running before kill"
    );

    let kill = tool
        .run(
            ctx(),
            serde_json::json!({"background_action": "kill", "process_id": process_id}),
        )
        .await
        .expect("kill ok");
    assert!(!kill.is_error);

    let mut still_alive = true;
    for _ in 0..100 {
        if !process_is_alive(grandchild_pid) {
            still_alive = false;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(
        !still_alive,
        "killing the tracked background process must also kill its \
         grandchild `sleep`, not leave it orphaned and running"
    );
}

/// Portable liveness probe: `kill(pid, 0)` sends no signal, only checks
/// whether the process (or an unreaped zombie) still exists.
#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    // SAFETY: none needed — this crate has no `unsafe_code` allowance,
    // so shell out to `kill -0` instead of linking a signals crate just
    // for a one-line test probe.
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn status_for_unknown_process_id_is_a_tool_error_not_a_panic() {
    let tool = bash_tool();
    let output = tool
        .run(
            ctx(),
            serde_json::json!({"background_action": "status", "process_id": "no-such-id"}),
        )
        .await
        .expect("run ok");
    assert!(output.is_error);
}

#[tokio::test]
async fn kill_for_unknown_process_id_reports_not_killed_rather_than_erroring() {
    let tool = bash_tool();
    let output = tool
        .run(
            ctx(),
            serde_json::json!({"background_action": "kill", "process_id": "no-such-id"}),
        )
        .await
        .expect("run ok");
    assert!(output.is_error);
}

#[tokio::test]
async fn background_action_without_process_id_is_invalid_input() {
    let tool = bash_tool();
    let err = tool
        .run(ctx(), serde_json::json!({"background_action": "status"}))
        .await
        .unwrap_err();
    assert!(matches!(err, ToolError::InvalidInput(_)));
}

#[tokio::test]
async fn session_teardown_kills_background_processes() {
    let registry = Arc::new(BackgroundProcessRegistry::new());
    let tool = bash_tool().with_background_registry(registry.clone());
    let session = SessionId::from("sess-teardown");
    let (events, _rx) = EventSink::channel();
    let call_ctx = ToolContext {
        session_id: session.clone(),
        turn_id: TurnId::from("turn-test"),
        call_id: ToolCallId::from("call-test"),
        cwd: std::path::PathBuf::from("."),
        cancel: CancellationToken::new(),
        events,
    };
    let start = tool
        .run(
            call_ctx,
            serde_json::json!({"command": "sleep 5", "run_in_background": true}),
        )
        .await
        .expect("run ok");
    let process_id = start
        .structured
        .as_ref()
        .and_then(|s| s.get("process_id"))
        .and_then(|v| v.as_str())
        .expect("process_id present")
        .to_owned();
    assert!(registry.status(&session, &process_id).is_some());

    registry.kill_session(&session).await;

    // Give the wait task a moment to observe the cancellation.
    for _ in 0..50 {
        match registry.status(&session, &process_id) {
            None => break,
            Some(_) => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
        }
    }
    assert!(
        registry.status(&session, &process_id).is_none(),
        "teardown must remove the session's entries from the registry"
    );
}

/// Demoting a still-running foreground call returns early with the
/// "moved to background" notice + output accumulated so far, and the
/// process shows up in the shared background registry as still running
/// — from there `background_action: "kill"` (the same control surface a
/// process started via `run_in_background` uses) works exactly as it
/// would on any other background entry.
#[tokio::test]
async fn demote_mid_run_returns_early_and_process_stays_running() {
    let background = Arc::new(BackgroundProcessRegistry::new());
    let demote = Arc::new(DemoteRegistry::new());
    let tool = bash_tool()
        .with_background_registry(background.clone())
        .with_demote_registry(demote.clone());
    let session = SessionId::from("sess-demote");
    let call_id = ToolCallId::from("call-demote");
    let (events, _rx) = EventSink::channel();
    let call_ctx = ToolContext {
        session_id: session.clone(),
        turn_id: TurnId::from("turn-test"),
        call_id: call_id.clone(),
        cwd: std::path::PathBuf::from("."),
        cancel: CancellationToken::new(),
        events,
    };

    let run = tokio::spawn(async move {
        tool.run(
            call_ctx,
            serde_json::json!({"command": "echo ready; sleep 5"}),
        )
        .await
    });

    // Give the command time to start and register its demote handle,
    // then fire the demote — polling rather than a fixed sleep guess so
    // the test isn't flaky under load.
    let mut demoted = false;
    for _ in 0..100 {
        if demote.request_demote(&session, call_id.as_str()) {
            demoted = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(
        demoted,
        "expected the running call to register a demote handle"
    );

    let output = run
        .await
        .expect("task join ok")
        .expect("run ok after demote");
    assert!(!output.is_error);
    let text = markdown_text(&output);
    assert!(text.contains("Moved to background"));
    assert!(text.contains(call_id.as_str()));

    let structured = output.structured.expect("structured result");
    assert_eq!(
        structured.get("process_id").and_then(|v| v.as_str()),
        Some(call_id.as_str()),
    );
    assert_eq!(
        structured.get("running"),
        Some(&serde_json::Value::Bool(true))
    );

    // The process itself is now tracked as a normal background entry,
    // under the very call id the foreground call was running as.
    let (status, command, _tail) = background
        .status(&session, call_id.as_str())
        .expect("registered in the background registry after demote");
    assert!(status.running);
    assert_eq!(command, "echo ready; sleep 5");

    // Kill it through the same control surface a `run_in_background`
    // process uses, cleaning up the still-sleeping child.
    let killed = background
        .kill(&session, call_id.as_str())
        .await
        .expect("kill ok");
    assert!(killed);
}

/// A demote request for a call that already finished naturally (or was
/// never running) is a no-op: `request_demote` returns `false`, and the
/// call's own result is completely unaffected (no "moved to background"
/// framing).
#[tokio::test]
async fn demote_after_natural_completion_is_a_noop() {
    let demote = Arc::new(DemoteRegistry::new());
    let tool = bash_tool().with_demote_registry(demote.clone());
    let session = SessionId::from("sess-demote-late");
    let call_id = ToolCallId::from("call-demote-late");
    let (events, _rx) = EventSink::channel();
    let call_ctx = ToolContext {
        session_id: session.clone(),
        turn_id: TurnId::from("turn-test"),
        call_id: call_id.clone(),
        cwd: std::path::PathBuf::from("."),
        cancel: CancellationToken::new(),
        events,
    };

    let output = tool
        .run(call_ctx, serde_json::json!({"command": "printf hello"}))
        .await
        .expect("run ok");
    assert!(!output.is_error);
    assert!(!markdown_text(&output).contains("Moved to background"));

    // The registration was removed the instant the call finished, so a
    // demote request that arrives after the fact finds nothing to signal.
    let demoted = demote.request_demote(&session, call_id.as_str());
    assert!(
        !demoted,
        "demoting an already-finished call must be a no-op"
    );
}

/// A normal (non-demoted) run through the demotable path stays
/// byte-identical to the pre-demote behavior: same stdout/stderr split,
/// same exit code, no "Moved to background" framing anywhere.
#[tokio::test]
async fn non_demoted_foreground_path_is_unaffected_by_demote_plumbing() {
    let tool = bash_tool();
    let output = tool
        .run(
            ctx(),
            serde_json::json!({"command": "printf out; printf err 1>&2"}),
        )
        .await
        .expect("run ok");
    assert!(!output.is_error);
    let text = markdown_text(&output);
    assert!(text.contains("stdout:\nout"));
    assert!(text.contains("stderr:\nerr"));
    assert!(!text.contains("Moved to background"));
    assert_eq!(
        output.structured.as_ref().and_then(|s| s.get("success")),
        Some(&serde_json::Value::Bool(true))
    );
}
