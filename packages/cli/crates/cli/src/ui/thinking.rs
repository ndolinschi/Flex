//! Reasoning / thinking block rendering.

use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use crate::theme;

const STREAM_TAIL_LINES: usize = 6;

/// Render a thinking block for the chat transcript.
pub(super) fn render_thinking_lines(
    text: &str,
    collapsed: bool,
    complete: bool,
    reasoning_visible: bool,
    spinner: usize,
) -> Vec<Line<'static>> {
    if !reasoning_visible {
        return Vec::new();
    }

    if collapsed && complete {
        return vec![collapsed_line(text)];
    }

    if !complete {
        return streaming_block(text, spinner);
    }

    expanded_block(text)
}

fn collapsed_line(text: &str) -> Line<'static> {
    let seconds = estimate_thinking_seconds(text);
    Line::from(vec![
        Span::styled("∴ ", theme::THINKING),
        Span::styled(
            format!("Thought for {seconds}s (shift+ctrl+t to expand)"),
            theme::THINKING,
        ),
    ])
}

fn streaming_block(text: &str, spinner: usize) -> Vec<Line<'static>> {
    let frame = theme::spinner_frame(spinner);
    let mut lines = vec![Line::from(vec![
        Span::styled("┌ ", theme::BORDER),
        Span::styled(
            format!("Thinking {frame}"),
            theme::THINKING.add_modifier(Modifier::BOLD),
        ),
    ])];
    for line in tail_lines(text, STREAM_TAIL_LINES) {
        lines.push(Line::from(vec![
            Span::styled("│ ", theme::BORDER),
            Span::styled(line, theme::THINKING),
        ]));
    }
    lines.push(Line::from(vec![
        Span::styled("│ ", theme::BORDER),
        Span::styled("▌", theme::WARN),
    ]));
    lines.push(Line::from(Span::styled("└", theme::BORDER)));
    lines
}

fn expanded_block(text: &str) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled("┌ ", theme::BORDER),
        Span::styled("Thinking", theme::THINKING.add_modifier(Modifier::BOLD)),
    ])];
    for line in text.lines() {
        lines.push(Line::from(vec![
            Span::styled("│ ", theme::BORDER),
            Span::styled(line.to_owned(), theme::THINKING),
        ]));
    }
    lines.push(Line::from(Span::styled("└", theme::BORDER)));
    lines
}

fn tail_lines(text: &str, max: usize) -> Vec<String> {
    let mut lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
    if lines.len() > max {
        lines = lines.split_off(lines.len() - max);
    }
    lines
}

/// Rough duration estimate until the engine emits thinking timing.
fn estimate_thinking_seconds(text: &str) -> u64 {
    let lines = text.lines().count().max(1);
    (lines as u64).saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_when_reasoning_not_visible() {
        let lines = render_thinking_lines("secret", false, false, false, 0);
        assert!(lines.is_empty());
    }

    #[test]
    fn collapsed_shows_duration_line() {
        let lines = render_thinking_lines("one\ntwo\nthree", true, true, true, 0);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].spans[1].content.contains("Thought for"));
    }

    #[test]
    fn streaming_shows_border() {
        let lines = render_thinking_lines("thinking…", false, false, true, 0);
        assert!(lines.iter().any(|l| l.spans[0].content.starts_with('┌')));
        assert!(lines.iter().any(|l| l.spans[0].content.starts_with('└')));
    }
}
