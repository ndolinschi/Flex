//! Style constants shared by every view. One place to retheme.

use ratatui::style::{Color, Modifier, Style};

/// User message prefix and text.
pub const USER: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
/// User message body in the transcript (`> …` rows).
pub const USER_TEXT: Style = Style::new().add_modifier(Modifier::DIM);
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
/// Added lines in Edit/Write diffs.
pub const DIFF_ADD: Style = Style::new().fg(Color::Green);
/// Deleted lines in Edit/Write diffs.
pub const DIFF_DEL: Style = Style::new().fg(Color::Red);
/// Tool row summary while the call is running.
pub const TOOL_RUNNING: Style = Style::new().fg(Color::White);
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

/// Pulse animation frames for the busy line and thinking marker.
pub const PULSE: [&str; 8] = ["·", "✢", "✳", "✻", "✽", "✻", "✳", "✢"];

/// The pulse frame for a tick counter.
pub fn pulse_frame(tick: usize) -> &'static str {
    PULSE[tick % PULSE.len()]
}

/// The thinking block marker glyph.
pub const THINKING_MARK: &str = "✻";

/// Busy-line verbs; one is picked per turn and kept stable for that turn.
pub const SPINNER_VERBS: &[&str] = &[
    "Pondering",
    "Brewing",
    "Wrangling",
    "Percolating",
    "Conjuring",
    "Marinating",
    "Noodling",
    "Vibing",
    "Scheming",
    "Tinkering",
    "Ruminating",
    "Cogitating",
    "Assembling",
    "Distilling",
    "Untangling",
    "Spelunking",
    "Grokking",
    "Weaving",
    "Simmering",
    "Herding",
    "Mulling",
    "Whittling",
    "Sketching",
    "Charting",
    "Rummaging",
    "Stitching",
    "Polishing",
    "Deciphering",
    "Incubating",
    "Composing",
];

/// The busy-line verb for a per-turn index.
pub fn spinner_verb(idx: usize) -> &'static str {
    SPINNER_VERBS[idx % SPINNER_VERBS.len()]
}
