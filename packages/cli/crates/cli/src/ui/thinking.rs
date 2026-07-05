//! Reasoning / thinking block rendering: borderless, dim italic, with a
//! one-line collapsed form once the thought completes.

use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use crate::chat::thinking_seconds;
use crate::theme;

const STREAM_TAIL_LINES: usize = 4;

/// Render a thinking block for the chat transcript.
pub(super) fn render_thinking_lines(
    text: &str,
    collapsed: bool,
    complete: bool,
    reasoning_visible: bool,
    spinner: usize,
    duration_ms: Option<u64>,
) -> Vec<Line<'static>> {
    if !reasoning_visible {
        return Vec::new();
    }

    // Never render a bare "Thinking…" placeholder before any delta arrives.
    if !complete && text.trim().is_empty() {
        return Vec::new();
    }

    if collapsed && complete {
        return vec![collapsed_line(text, duration_ms)];
    }

    if !complete {
        return streaming_block(text, spinner);
    }

    expanded_block(text)
}

fn collapsed_line(text: &str, duration_ms: Option<u64>) -> Line<'static> {
    let seconds = thinking_seconds(duration_ms, text);
    Line::from(vec![
        Span::styled(format!("{} ", theme::THINKING_MARK), theme::thinking()),
        Span::styled(
            format!("Thought for {seconds}s (ctrl+t to expand)"),
            theme::thinking(),
        ),
    ])
}

fn streaming_block(text: &str, spinner: usize) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!("{} ", theme::pulse_frame(spinner)),
            theme::thinking(),
        ),
        Span::styled("Thinking…", theme::thinking().add_modifier(Modifier::BOLD)),
    ])];
    for line in tail_lines(text, STREAM_TAIL_LINES) {
        lines.push(Line::from(Span::styled(
            format!("  {line}"),
            theme::thinking(),
        )));
    }
    lines
}

fn expanded_block(text: &str) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled(format!("{} ", theme::THINKING_MARK), theme::thinking()),
        Span::styled("Thinking", theme::thinking().add_modifier(Modifier::BOLD)),
    ])];
    for line in text.lines() {
        lines.push(Line::from(Span::styled(
            format!("  {line}"),
            theme::thinking(),
        )));
    }
    lines
}

fn tail_lines(text: &str, max: usize) -> Vec<String> {
    let mut lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
    if lines.len() > max {
        lines = lines.split_off(lines.len() - max);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_when_reasoning_not_visible() {
        let lines = render_thinking_lines("secret", false, false, false, 0, None);
        assert!(lines.is_empty());
    }

    #[test]
    fn empty_streaming_text_renders_nothing() {
        let lines = render_thinking_lines("", false, false, true, 0, None);
        assert!(lines.is_empty());
        let whitespace = render_thinking_lines("  \n ", false, false, true, 0, None);
        assert!(whitespace.is_empty());
    }

    #[test]
    fn collapsed_shows_real_duration() {
        let lines = render_thinking_lines("one\ntwo\nthree", true, true, true, 0, Some(12_400));
        assert_eq!(lines.len(), 1);
        assert!(lines[0].spans[1].content.contains("Thought for 12s"));
        assert!(lines[0].spans[1].content.contains("ctrl+t to expand"));
    }

    #[test]
    fn collapsed_estimates_without_duration() {
        let lines = render_thinking_lines("one\ntwo\nthree", true, true, true, 0, None);
        assert!(lines[0].spans[1].content.contains("Thought for 4s"));
    }

    #[test]
    fn streaming_is_borderless_and_tails_four_lines() {
        let text = "a\nb\nc\nd\ne\nf";
        let lines = render_thinking_lines(text, false, false, true, 3, None);
        // Header + 4 tail lines, no border or cursor rows.
        assert_eq!(lines.len(), 5);
        assert!(lines[0].spans[0].content.starts_with(theme::PULSE[3]));
        assert!(lines[0].spans[1].content.contains("Thinking…"));
        assert!(lines[1].spans[0].content.contains('c'));
        assert!(lines[4].spans[0].content.contains('f'));
        let flat: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(!flat.contains('┌'));
        assert!(!flat.contains('│'));
        assert!(!flat.contains('▌'));
    }

    #[test]
    fn expanded_is_borderless() {
        let lines = render_thinking_lines("full thought", false, true, true, 0, None);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].spans[0].content.starts_with(theme::THINKING_MARK));
        assert!(lines[1].spans[0].content.contains("full thought"));
    }
}
