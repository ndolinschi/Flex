//! Style constants shared by every view. One place to retheme.

use ratatui::style::{Color, Modifier, Style};

/// User message prefix and text.
pub const USER: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
/// Assistant message accent (the marker glyph; body text stays default).
pub const ASSISTANT: Style = Style::new().fg(Color::White);
/// Fenced code block body.
pub const CODE: Style = Style::new().fg(Color::Green);
/// Fenced code block border / language tag.
pub const CODE_BORDER: Style = Style::new().fg(Color::DarkGray);
/// Dim secondary text: info lines, hints, collapsed thinking.
pub const DIM: Style = Style::new().fg(Color::DarkGray);
/// Thinking/reasoning body text.
pub const THINKING: Style = Style::new()
    .fg(Color::DarkGray)
    .add_modifier(Modifier::ITALIC);
/// Errors, denied/failed tool calls.
pub const ERROR: Style = Style::new().fg(Color::Red);
/// Completed tool calls, success notices.
pub const SUCCESS: Style = Style::new().fg(Color::Green);
/// Pending/awaiting states, notices.
pub const WARN: Style = Style::new().fg(Color::Yellow);
/// Tool header line.
pub const TOOL: Style = Style::new().fg(Color::Magenta);
/// Status-bar base style.
pub const STATUS: Style = Style::new().fg(Color::Gray);
/// Highlighted row in pickers and dialogs.
pub const SELECTED: Style = Style::new()
    .fg(Color::Black)
    .bg(Color::Cyan)
    .add_modifier(Modifier::BOLD);
/// Overlay/popup borders.
pub const BORDER: Style = Style::new().fg(Color::DarkGray);
/// Overlay titles.
pub const TITLE: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);

/// Spinner animation frames, indexed by tick count.
pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// The spinner frame for a tick counter.
pub fn spinner_frame(tick: usize) -> &'static str {
    SPINNER[tick % SPINNER.len()]
}
