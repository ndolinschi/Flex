//! Test helpers for rendering snapshots.

use std::path::PathBuf;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use agentloop_cli_core::AgentKind;
use agentloop_contracts::{AgentCaps, Hello, ModelRef, SessionId};

use crate::app::App;
use crate::events::SessionBootstrap;
use crate::files::FileIndex;
use crate::ui;

/// Build a test app with an empty file index.
pub fn test_app(bootstrap: SessionBootstrap) -> App {
    App::new(bootstrap, PathBuf::from("."), FileIndex::default())
}

/// Minimal session bootstrap for render tests.
pub fn test_bootstrap() -> SessionBootstrap {
    SessionBootstrap {
        kind: AgentKind::Native,
        hello: Hello::new(AgentCaps::default()),
        session: SessionId::from("sess-test"),
        providers: vec!["anthropic".to_owned(), "copilot".to_owned()],
        model: Some(ModelRef::from("anthropic/claude-sonnet-4-5")),
        transcript: None,
        trace: Vec::new(),
        permission_mode: None,
    }
}

/// Render `app` into a fixed-size terminal and return the buffer as text.
pub fn render_app(app: &mut App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| ui::draw(frame, app))
        .expect("draw frame");
    buffer_string(terminal.backend().buffer())
}

/// Serialize a ratatui buffer to a stable, line-oriented string.
pub fn buffer_string(buffer: &Buffer) -> String {
    let mut lines = Vec::new();
    for y in 0..buffer.area.height {
        let mut row = String::new();
        for x in 0..buffer.area.width {
            row.push_str(buffer[(x, y)].symbol());
        }
        lines.push(row.trim_end().to_owned());
    }
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}
