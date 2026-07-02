//! Pure ratatui rendering for the app state.

mod markdown;
mod thinking;
mod tool_view;

pub use markdown::MarkdownCache;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

use agentloop_contracts::PermissionDecisionKind;

use crate::app::{App, TurnPhase, permission_mode_label, session_mode_label};
use crate::chat::{ChatItem, DraftBlock};
use crate::input::{CommandPopup, FilePopup, InputPopup};
use crate::overlay::{
    ConfirmPrompt, LoginState, Overlay, PermissionPrompt, PickerState, QuestionPrompt,
    ShellCommandOverlay, ShellCommandPhase,
};
use crate::theme;

/// Draw one full frame.
pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    let input_height = input_height(app);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(input_height),
            Constraint::Length(1),
        ])
        .split(area);

    draw_chat(frame, app, chunks[0]);
    draw_input(frame, app, chunks[1]);
    draw_status(frame, app, chunks[2]);
    draw_popup(frame, app, chunks[1]);
    draw_overlay(frame, app, area);
}

fn input_height(app: &App) -> u16 {
    let lines = app.input.textarea.lines().len().clamp(1, 6);
    (lines as u16 + 2).clamp(3, 8)
}

fn draw_chat(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let block = Block::default().borders(Borders::BOTTOM);
    let inner = block.inner(area);
    let lines = chat_lines(app, inner.width);
    if app.chat.scroll.follow {
        app.chat.scroll.offset_from_bottom = 0;
    }

    let (_, _, max_offset) = chat_viewport_metrics(&lines, area);
    app.chat.scroll.clamp_offset(max_offset);
    let scroll_top = max_offset.saturating_sub(app.chat.scroll.offset_from_bottom);

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll_top as u16, 0));
    frame.render_widget(paragraph, area);
}

/// Scroll budget for the chat pane: `(total_wrapped, viewport, max_offset)`.
fn chat_viewport_metrics(lines: &[Line<'_>], area: Rect) -> (usize, usize, usize) {
    let block = Block::default().borders(Borders::BOTTOM);
    let inner = block.inner(area);
    let total_lines = wrapped_line_count(lines, inner.width);
    let viewport_lines = inner.height as usize;
    let max_offset = total_lines.saturating_sub(viewport_lines);
    (total_lines, viewport_lines, max_offset)
}

/// Count wrapped visual rows for a set of logical lines at `width`.
///
/// Uses ratatui's [`Paragraph::line_count`] so scroll math matches render wrapping.
fn wrapped_line_count(lines: &[Line<'_>], width: u16) -> usize {
    if width < 1 {
        return 0;
    }
    Paragraph::new(Text::from(lines.to_vec()))
        .wrap(Wrap { trim: false })
        .line_count(width)
}

fn chat_lines(app: &mut App, viewport_width: u16) -> Vec<Line<'static>> {
    let thinking_visible = app.thinking_visible();
    let live_keys = app
        .chat
        .items
        .iter()
        .filter_map(|item| match item {
            ChatItem::Assistant { blocks, rev, .. } => Some(
                blocks
                    .iter()
                    .enumerate()
                    .filter(|(_, block)| matches!(block, DraftBlock::Markdown(_)))
                    .map(move |(idx, _)| (*rev, idx)),
            ),
            _ => None,
        })
        .flatten()
        .collect::<Vec<_>>();
    app.markdown_cache.retain_live(live_keys);

    let mut lines = Vec::new();
    let items = &app.chat.items;
    let mut idx = 0;
    while idx < items.len() {
        let item = &items[idx];
        match item {
            ChatItem::User { text } => {
                lines.push(Line::from(vec![
                    Span::styled("  > ", theme::USER),
                    Span::styled(text.clone(), theme::USER),
                ]));
            }
            ChatItem::Assistant {
                blocks,
                complete,
                rev,
                ..
            } if should_render_assistant(items, idx, thinking_visible) => {
                for (block_idx, block) in blocks.iter().enumerate() {
                    match block {
                        DraftBlock::Markdown(text) => {
                            lines.extend(markdown::lines_for_block(
                                &mut app.markdown_cache,
                                (*rev, block_idx),
                                text,
                                *complete,
                                "  ",
                                viewport_width,
                            ));
                        }
                        DraftBlock::Thinking { text, collapsed } => {
                            lines.extend(thinking::render_thinking_lines(
                                text,
                                *collapsed,
                                *complete,
                                thinking_visible,
                                app.status.spinner,
                            ));
                        }
                    }
                }
                if should_show_stream_cursor(blocks, *complete, thinking_visible) {
                    lines.push(Line::from(Span::styled("  ▌", theme::WARN)));
                }
            }
            ChatItem::Assistant { .. } => {}
            ChatItem::Tool {
                call,
                progress,
                expanded,
                ..
            } => {
                let failed_streak = count_failed_streak(items, idx);
                let row = tool_view::ToolRow {
                    call,
                    progress: progress.as_deref(),
                    failed_streak,
                    expanded: *expanded,
                    focused: app.chat.focused_tool == Some(idx),
                };
                lines.extend(tool_view::render_tool_row(&row));
                if failed_streak > 1 {
                    idx += failed_streak - 1;
                }
            }
            ChatItem::Plan { entries, .. } => {
                lines.push(Line::from(Span::styled("plan", theme::DIM)));
                for entry in entries {
                    lines.push(Line::from(Span::styled(
                        format!("  {:?}: {}", entry.status, entry.content),
                        theme::DIM,
                    )));
                }
            }
            ChatItem::Info { text } => {
                lines.push(Line::from(Span::styled(text.clone(), theme::DIM)));
            }
            ChatItem::Error { message } => {
                lines.push(Line::from(Span::styled(message.clone(), theme::ERROR)));
            }
            ChatItem::Subagent { task, done, .. } => {
                let marker = if *done { "done" } else { "running" };
                lines.push(Line::from(Span::styled(
                    format!("subagent {marker}: {task}"),
                    theme::DIM,
                )));
            }
        }
        if item_produces_lines(item, items, idx, thinking_visible) {
            lines.push(Line::default());
        }
        idx += 1;
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "Start with a prompt or /help.",
            theme::DIM,
        )));
    }
    lines
}

fn should_show_stream_cursor(
    blocks: &[DraftBlock],
    complete: bool,
    thinking_visible: bool,
) -> bool {
    if complete || !assistant_has_visible_body(blocks, thinking_visible) {
        return false;
    }
    matches!(blocks.last(), Some(DraftBlock::Markdown(text)) if !text.is_empty())
}

/// Whether an assistant item should contribute visible chat lines.
fn should_render_assistant(items: &[ChatItem], idx: usize, thinking_visible: bool) -> bool {
    let ChatItem::Assistant { blocks, .. } = &items[idx] else {
        return false;
    };
    if is_tool_only_assistant(items, idx) {
        return false;
    }
    assistant_has_visible_body(blocks, thinking_visible)
}

fn assistant_has_visible_body(blocks: &[DraftBlock], thinking_visible: bool) -> bool {
    blocks
        .iter()
        .any(|block| block_is_visible(block, thinking_visible))
}

fn block_is_visible(block: &DraftBlock, thinking_visible: bool) -> bool {
    match block {
        DraftBlock::Markdown(text) => !text.is_empty(),
        DraftBlock::Thinking { text, .. } => thinking_visible && !text.is_empty(),
    }
}

fn item_produces_lines(
    item: &ChatItem,
    items: &[ChatItem],
    idx: usize,
    thinking_visible: bool,
) -> bool {
    match item {
        ChatItem::Assistant { .. } => should_render_assistant(items, idx, thinking_visible),
        _ => true,
    }
}

/// Whether an assistant item has no body blocks and is followed by tool rows
/// in the same turn (no intervening user or assistant message).
fn is_tool_only_assistant(items: &[ChatItem], idx: usize) -> bool {
    let ChatItem::Assistant { blocks, .. } = &items[idx] else {
        return false;
    };
    if !blocks.is_empty() {
        return false;
    }
    let mut saw_tool = false;
    for item in items.iter().skip(idx + 1) {
        match item {
            ChatItem::Tool { .. } => saw_tool = true,
            ChatItem::User { .. } | ChatItem::Assistant { .. } => break,
            _ => {}
        }
    }
    saw_tool
}

/// Count consecutive identical failed tool rows starting at `start`.
fn count_failed_streak(items: &[ChatItem], start: usize) -> usize {
    let ChatItem::Tool { call, .. } = &items[start] else {
        return 1;
    };
    if !tool_view::is_failed_streak_member(call) {
        return 1;
    }
    let mut count = 1;
    for item in items.iter().skip(start + 1) {
        let ChatItem::Tool { call: next, .. } = item else {
            break;
        };
        if tool_view::same_tool_identity(call, next) && tool_view::is_failed_streak_member(next) {
            count += 1;
        } else {
            break;
        }
    }
    count
}

fn draw_input(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" prompt ");
    frame.render_widget(block, area);
    let inner = area.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    frame.render_widget(&app.input.textarea, inner);
}

fn draw_status(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let model = app
        .session
        .model
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "default model".to_owned());
    let running = app.session.turn.is_running();
    let busy = match app.session.turn {
        TurnPhase::Idle => "idle".to_owned(),
        TurnPhase::Running { started } => {
            format!(
                "{} streaming {}s",
                theme::spinner_frame(app.status.spinner),
                started.elapsed().as_secs()
            )
        }
    };
    let usage = app.status.total_usage;
    let session_mode = session_mode_label(app.session.session_mode);
    let permission = permission_mode_label(app.session.effective_permission_mode());
    let mut text = format!(
        "{} · {} · {} · {} · {} · tokens in/out {} / {}",
        app.kind, session_mode, permission, model, busy, usage.input, usage.output
    );
    if !app.chat.scroll.follow {
        text.push_str(" · scrolled up (End or PgDn to follow)");
    }
    if let Some(cost) = app.status.last_cost_usd {
        text.push_str(&format!(" · ${cost:.4}"));
    }
    if let Some(notice) = &app.status.notice {
        text.push_str(&format!(" · {notice}"));
    }
    if let Some(error) = &app.status.last_error {
        text.push_str(&format!(" · {error}"));
    }
    frame.render_widget(
        Paragraph::new(text).style(if running { theme::WARN } else { theme::STATUS }),
        area,
    );
}

fn draw_popup(frame: &mut Frame<'_>, app: &App, input_area: Rect) {
    let Some(popup) = &app.input.popup else {
        return;
    };
    match popup {
        InputPopup::Command(command_popup) => draw_command_popup(frame, command_popup, input_area),
        InputPopup::File(file_popup) => draw_file_popup(frame, file_popup, input_area),
    }
}

fn draw_command_popup(frame: &mut Frame<'_>, popup: &CommandPopup, input_area: Rect) {
    let height = (popup.matches.len().min(8) as u16).saturating_add(2).max(3);
    let y = input_area.y.saturating_sub(height);
    let area = Rect {
        x: input_area.x,
        y,
        width: input_area.width.min(60),
        height,
    };
    let items = popup
        .matches
        .iter()
        .enumerate()
        .take(8)
        .map(|(idx, entry)| {
            let style = if idx == popup.selected {
                theme::SELECTED
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("/{}", entry.name), style),
                Span::raw(" "),
                Span::styled(entry.description.clone(), theme::DIM),
                Span::styled(format!(" [{}]", entry.source), theme::DIM),
            ]))
        })
        .collect::<Vec<_>>();
    frame.render_widget(Clear, area);
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title(" commands ")),
        area,
    );
}

fn draw_file_popup(frame: &mut Frame<'_>, popup: &FilePopup, input_area: Rect) {
    let height = (popup.matches.len().min(8) as u16).saturating_add(2).max(3);
    let y = input_area.y.saturating_sub(height);
    let area = Rect {
        x: input_area.x,
        y,
        width: input_area.width.min(60),
        height,
    };
    let items = popup
        .matches
        .iter()
        .enumerate()
        .take(8)
        .map(|(idx, path)| {
            let style = if idx == popup.selected {
                theme::SELECTED
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled("@", theme::DIM),
                Span::styled(path.clone(), style),
            ]))
        })
        .collect::<Vec<_>>();
    frame.render_widget(Clear, area);
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title(" files ")),
        area,
    );
}

fn draw_overlay(frame: &mut Frame<'_>, app: &App, root: Rect) {
    match &app.overlay {
        Overlay::None => {}
        Overlay::Picker(picker) => draw_picker(frame, picker, root),
        Overlay::Permission(prompt) => draw_permission(frame, prompt, root),
        Overlay::Question(prompt) => draw_question(frame, prompt, root),
        Overlay::Login(state) => draw_login(frame, state, root),
        Overlay::Help => draw_help(frame, app, root),
        Overlay::ShellCommand(state) => draw_shell_command(frame, state, app, root),
        Overlay::Confirm(prompt) => draw_confirm(frame, prompt, root),
    }
}

fn draw_picker(frame: &mut Frame<'_>, picker: &PickerState, root: Rect) {
    let area = centered(root, 70, 60);
    let visible = picker.visible();
    let items = visible
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let style = if !item.enabled {
                theme::DIM
            } else if idx == picker.selected {
                theme::SELECTED
            } else {
                Style::default()
            };
            let mut spans = vec![Span::styled(item.label.clone(), style)];
            if let Some(detail) = &item.detail {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(detail.clone(), theme::DIM));
            }
            ListItem::new(Line::from(spans))
        })
        .collect::<Vec<_>>();
    frame.render_widget(Clear, area);
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme::BORDER)
                .title(format!(" {}  filter: {} ", picker.title, picker.filter)),
        ),
        area,
    );
}

fn draw_permission(frame: &mut Frame<'_>, prompt: &PermissionPrompt, root: Rect) {
    let area = centered(root, 64, 40);
    let mut lines = vec![Line::from(Span::styled(prompt.title.clone(), theme::TITLE))];
    if let Some(detail) = &prompt.detail {
        lines.push(Line::default());
        lines.push(Line::from(detail.clone()));
    }
    lines.push(Line::default());
    for (idx, option) in prompt.options.iter().enumerate() {
        let style = if idx == prompt.selected {
            theme::SELECTED
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(permission_label(*option), style)));
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "Enter/y allow, a always, Esc/n deny",
        theme::DIM,
    )));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" permission "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn permission_label(kind: PermissionDecisionKind) -> &'static str {
    match kind {
        PermissionDecisionKind::AllowOnce => "allow once",
        PermissionDecisionKind::AllowAlways => "allow always",
        PermissionDecisionKind::Deny => "deny",
        _ => "unknown",
    }
}

fn draw_question(frame: &mut Frame<'_>, prompt: &QuestionPrompt, root: Rect) {
    let area = centered(root, 70, 55);
    let Some(question) = prompt.questions.get(prompt.current) else {
        return;
    };
    let mut lines = vec![
        Line::from(Span::styled(question.header.clone(), theme::TITLE)),
        Line::from(question.question.clone()),
        Line::default(),
    ];
    for (idx, option) in question.options.iter().enumerate() {
        let picked = prompt.picks[prompt.current].contains(&idx);
        let cursor = idx == prompt.cursor;
        let marker = if picked { "[x]" } else { "[ ]" };
        let style = if cursor {
            theme::SELECTED
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} {}", option.label), style),
            option
                .description
                .as_ref()
                .map(|description| Span::styled(format!(" — {description}"), theme::DIM))
                .unwrap_or_else(|| Span::raw("")),
        ]));
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "Enter answers, Space toggles multi-select, Esc submits partial answers",
        theme::DIM,
    )));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" question "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_login(frame: &mut Frame<'_>, state: &LoginState, root: Rect) {
    let area = centered(root, 64, 35);
    let lines = match state {
        LoginState::Starting => vec![Line::from(
            "Sign in to use GitHub Copilot. Requesting a device code...",
        )],
        LoginState::CodeReady {
            user_code,
            verification_uri,
            expires_in,
            since,
        } => vec![
            Line::from("Open this URL and enter the code:"),
            Line::default(),
            Line::from(Span::styled(verification_uri.clone(), theme::TITLE)),
            Line::from(Span::styled(
                user_code.clone(),
                theme::TITLE.add_modifier(Modifier::BOLD),
            )),
            Line::default(),
            Line::from(Span::styled(
                format!(
                    "Waiting {}s / expires in {expires_in}s. Press o to open, Esc to cancel.",
                    since.elapsed().as_secs()
                ),
                theme::DIM,
            )),
        ],
        LoginState::Polling { since } => vec![Line::from(format!(
            "Waiting for GitHub confirmation... {}s",
            since.elapsed().as_secs()
        ))],
        LoginState::Verifying => vec![Line::from("Verifying Copilot access...")],
        LoginState::Failed { message } => vec![
            Line::from(Span::styled("Login failed", theme::ERROR)),
            Line::default(),
            Line::from(message.clone()),
            Line::default(),
            Line::from(Span::styled("Esc or Enter closes this dialog.", theme::DIM)),
        ],
    };
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" GitHub Copilot login "),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_shell_command(frame: &mut Frame<'_>, state: &ShellCommandOverlay, app: &App, root: Rect) {
    let area = centered(root, 80, 70);
    let mut lines = vec![Line::from(vec![
        Span::styled("$ ", theme::USER),
        Span::styled(state.command.clone(), theme::ASSISTANT),
    ])];
    lines.push(Line::default());

    match &state.phase {
        ShellCommandPhase::Running { since } => {
            let spinner = theme::spinner_frame(app.status.spinner);
            lines.push(Line::from(vec![
                Span::styled(format!("{spinner} "), theme::WARN),
                Span::styled(
                    format!("running… {}s", since.elapsed().as_secs()),
                    theme::WARN,
                ),
            ]));
            lines.push(Line::default());
            lines.push(Line::from(Span::styled("Esc cancels", theme::DIM)));
        }
        ShellCommandPhase::Done { output, exit_code } => {
            if let Some(code) = exit_code.filter(|code| *code != 0) {
                lines.push(Line::from(Span::styled(
                    format!("exit code {code}"),
                    theme::ERROR,
                )));
                lines.push(Line::default());
            }
            if output.is_empty() {
                lines.push(Line::from(Span::styled("(no output)", theme::DIM)));
            } else {
                for line in output.lines() {
                    lines.push(Line::from(line.to_owned()));
                }
            }
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                "↑/↓ scroll · Esc close",
                theme::DIM,
            )));
        }
        ShellCommandPhase::Failed { message } => {
            lines.push(Line::from(Span::styled(message.clone(), theme::ERROR)));
            lines.push(Line::default());
            lines.push(Line::from(Span::styled("Esc close", theme::DIM)));
        }
    }

    let viewport_lines = area.height.saturating_sub(2) as usize;
    let total_lines = wrapped_line_count(&lines, area.width);
    let max_scroll = total_lines.saturating_sub(viewport_lines);
    let scroll = state.scroll.min(max_scroll);

    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(theme::BORDER)
                    .title(" command "),
            )
            .wrap(Wrap { trim: false })
            .scroll((scroll as u16, 0)),
        area,
    );
}

fn draw_help(frame: &mut Frame<'_>, app: &App, root: Rect) {
    let area = centered(root, 76, 70);
    let mut lines = vec![
        Line::from(Span::styled("Keys", theme::TITLE)),
        Line::from("Enter submit · Alt+Enter/Ctrl+J newline · Esc cancel · Ctrl+C quit"),
        Line::from("@ attach files · type @ in prompt to fuzzy-search the workdir"),
        Line::from("PgUp/PgDn or ↑/↓ (empty prompt) scroll · End follow · drag to select & copy"),
        Line::from("Ctrl+Shift+C or /copy — copy transcript · Ctrl+M — toggle mouse wheel scroll"),
        Line::from(
            "Ctrl+T show/hide thinking · Shift+Ctrl+T expand/collapse · /mode code|plan · /permissions require|auto|allow-all",
        ),
        Line::from("Tab cycle tool rows · Enter/Space expand tool output (empty prompt)"),
        Line::default(),
        Line::from(Span::styled("Commands", theme::TITLE)),
    ];
    for entry in app.commands.entries() {
        let hint = entry
            .args_hint
            .as_ref()
            .map(|hint| format!(" {hint}"))
            .unwrap_or_default();
        lines.push(Line::from(vec![
            Span::styled(format!("/{}{}", entry.name, hint), theme::ASSISTANT),
            Span::raw(" — "),
            Span::raw(entry.description.clone()),
            Span::styled(format!(" [{}]", entry.source), theme::DIM),
        ]));
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "CLI commands win name collisions; engine commands are sent through to the loop.",
        theme::DIM,
    )));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(Block::default().borders(Borders::ALL).title(" help "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_confirm(frame: &mut Frame<'_>, prompt: &ConfirmPrompt, root: Rect) {
    let area = centered(root, 64, 35);
    let lines = vec![
        Line::from(Span::styled(prompt.title.clone(), theme::TITLE)),
        Line::default(),
        Line::from(prompt.message.clone()),
        Line::default(),
        Line::from(Span::styled("Enter/y confirm · Esc cancel", theme::DIM)),
    ];
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" confirm "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn centered(root: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(root);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod scroll_tests {
    use ratatui::layout::Rect;
    use ratatui::text::Line;

    use super::{chat_viewport_metrics, wrapped_line_count};

    #[test]
    fn viewport_metrics_empty_content() {
        let area = Rect::new(0, 0, 80, 20);
        let (total, viewport, max_offset) = chat_viewport_metrics(&[], area);
        assert_eq!(total, 0);
        assert_eq!(viewport, 19);
        assert_eq!(max_offset, 0);
    }

    #[test]
    fn viewport_metrics_short_content_fits() {
        let area = Rect::new(0, 0, 80, 10);
        let lines = vec![Line::from("hello"), Line::from("world")];
        let (total, viewport, max_offset) = chat_viewport_metrics(&lines, area);
        assert_eq!(total, 2);
        assert_eq!(viewport, 9);
        assert_eq!(max_offset, 0);
    }

    #[test]
    fn viewport_metrics_long_content_scrolls() {
        let area = Rect::new(0, 0, 40, 8);
        let lines: Vec<_> = (0..20).map(|i| Line::from(format!("row {i}"))).collect();
        let (total, viewport, max_offset) = chat_viewport_metrics(&lines, area);
        assert_eq!(total, 20);
        assert_eq!(viewport, 7);
        assert_eq!(max_offset, 13);
    }

    #[test]
    fn wrapped_line_count_matches_word_wrap() {
        let lines = vec![Line::from(
            "This paragraph is long enough that it should wrap across multiple terminal rows.",
        )];
        let naive = lines[0].width().div_ceil(20);
        let actual = wrapped_line_count(&lines, 20);
        assert!(
            actual >= naive,
            "word wrap should produce at least as many rows as char wrap: {actual} vs {naive}"
        );
        assert!(actual >= 4, "expected multiple wrapped rows, got {actual}");
    }

    #[test]
    fn bottom_scroll_offset_shows_last_line() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

        let marker = "ZZZZ_LAST_LINE";
        let mut lines: Vec<_> = (0..30)
            .map(|i| Line::from(format!("filler line {i}")))
            .collect();
        lines.push(Line::from(marker));

        let area = Rect::new(0, 0, 60, 12);
        let block = Block::default().borders(Borders::BOTTOM);
        let inner = block.inner(area);
        let (_, _, max_offset) = chat_viewport_metrics(&lines, area);

        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| {
                frame.render_widget(
                    Paragraph::new(lines)
                        .block(block)
                        .wrap(Wrap { trim: false })
                        .scroll((max_offset as u16, 0)),
                    area,
                );
            })
            .expect("draw");

        let buf = terminal.backend().buffer();
        let mut visible = String::new();
        for y in inner.y..inner.y + inner.height {
            for x in inner.x..inner.x + inner.width {
                visible.push_str(buf[(x, y)].symbol());
            }
            visible.push('\n');
        }
        assert!(
            visible.contains(marker),
            "last line should be visible at max scroll; got:\n{visible}"
        );
    }
}
