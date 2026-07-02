//! Pure ratatui rendering for the app state.

pub(crate) mod diff;
mod markdown;
mod thinking;
mod tool_view;

pub use markdown::MarkdownCache;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

use agentloop_contracts::{PermissionDecisionKind, Question};

use crate::app::{App, TurnPhase, permission_mode_label, session_mode_label};
use crate::chat::{ChatItem, DraftBlock};
use crate::files::{MENTION_PREVIEW_MAX_LINES, MentionPreview};
use crate::input::{
    CommandPopup, FilePopup, InputPopup, POPUP_LIST_MAX_ROWS, popup_list_scroll_offset,
};
use crate::overlay::{
    ConfirmPrompt, LoginState, McpExplorerPhase, McpExplorerState, McpInstallMode, McpInstallState,
    McpListState, Overlay, PermissionPrompt, PickerState, QuestionPrompt, ShellCommandOverlay,
    ShellCommandPhase,
};
use crate::terminal_text::terminal_lines;
use crate::theme;

/// Draw one full frame: chat, an optional notification line (busy pulse or
/// newest toast), the input box, and the status bar.
pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    let input_height = input_height(app);
    let notify = notification_line_visible(app);
    let mut constraints = vec![Constraint::Min(1)];
    if notify {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(input_height));
    constraints.push(Constraint::Length(1));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let input_area = chunks[if notify { 2 } else { 1 }];
    draw_chat(frame, app, chunks[0]);
    if notify {
        draw_notification_line(frame, app, chunks[1]);
    }
    draw_input(frame, app, input_area);
    draw_status(frame, app, chunks[if notify { 3 } else { 2 }]);
    draw_popup(frame, app, input_area);
    draw_overlay(frame, app, area);
}

/// The notification line stays reserved while a turn runs or a toast is
/// alive, so the layout doesn't jitter between busy and toast states.
fn notification_line_visible(app: &App) -> bool {
    app.session.turn.is_running() || !app.status.toasts.is_empty()
}

/// Busy line (priority) or the newest toast.
fn draw_notification_line(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let line = if let TurnPhase::Running { started } = app.session.turn {
        let glyph = theme::pulse_frame(app.status.spinner);
        let verb = theme::spinner_verb(app.status.turn_verb_idx);
        let tokens = app.status.turn_output_chars / 4;
        Line::from(vec![
            Span::styled(format!("{glyph} {verb}… "), theme::WARN),
            Span::styled(
                format!(
                    "({}s · ↑ {} tokens · esc to interrupt)",
                    started.elapsed().as_secs(),
                    fmt_k(tokens)
                ),
                theme::DIM,
            ),
        ])
    } else if let Some(toast) = app.status.toasts.back() {
        Line::from(Span::styled(toast.text.clone(), theme::DIM))
    } else {
        return;
    };
    frame.render_widget(Paragraph::new(line), area);
}

/// `1234` → `1.2k`, `12_300_000` → `12.3M`.
fn fmt_k(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Thinking budget label for the status bar (`8192` → `8k`).
pub(crate) fn fmt_thinking_budget_k(tokens: u32) -> String {
    if tokens >= 1024 && tokens % 1024 == 0 {
        format!("{}k", tokens / 1024)
    } else if tokens >= 1000 {
        format!("{}k", tokens / 1000)
    } else {
        tokens.to_string()
    }
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
                for (line_idx, line) in terminal_lines(text).into_iter().enumerate() {
                    if line_idx == 0 {
                        lines.push(Line::from(vec![
                            Span::styled("> ", theme::DIM),
                            Span::styled(line.to_owned(), theme::USER_TEXT),
                        ]));
                    } else {
                        lines.push(Line::from(Span::styled(
                            format!("  {line}"),
                            theme::USER_TEXT,
                        )));
                    }
                }
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
                        DraftBlock::Thinking {
                            text,
                            collapsed,
                            duration_ms,
                            ..
                        } => {
                            lines.extend(thinking::render_thinking_lines(
                                text,
                                *collapsed,
                                *complete,
                                thinking_visible,
                                app.status.spinner,
                                *duration_ms,
                            ));
                        }
                    }
                }
                if should_show_stream_cursor(blocks, *complete, thinking_visible) {
                    lines.push(Line::from(Span::styled("  ▌", theme::DIM)));
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
                    spinner: app.status.spinner,
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
                        format!("  {} {}", plan_marker(entry.status), entry.content),
                        theme::DIM,
                    )));
                }
            }
            ChatItem::Info { text } => {
                // The interrupt marker reads as a soft error, not plain info.
                let style = if text == crate::app::INTERRUPT_NOTE {
                    theme::ERROR.add_modifier(Modifier::DIM)
                } else {
                    theme::DIM
                };
                lines.push(Line::from(Span::styled(text.clone(), style)));
            }
            ChatItem::Error { headline, detail } => {
                lines.push(Line::from(Span::styled(
                    format!("✗ {headline}"),
                    theme::ERROR,
                )));
                if let Some(detail) = detail {
                    lines.push(Line::from(Span::styled(
                        detail.clone(),
                        theme::ERROR.add_modifier(Modifier::DIM),
                    )));
                }
            }
            ChatItem::Subagent { task, done, .. } => {
                let marker = if *done { "done" } else { "running" };
                lines.push(Line::from(Span::styled(
                    format!("subagent {marker}: {task}"),
                    theme::DIM,
                )));
            }
        }
        if item_produces_lines(item, items, idx, thinking_visible) && !tight_group(items, idx) {
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

/// Consecutive Info lines and consecutive Tool rows group tightly: no blank
/// line between them.
fn tight_group(items: &[ChatItem], idx: usize) -> bool {
    let Some(next) = items.get(idx + 1) else {
        return false;
    };
    matches!(
        (&items[idx], next),
        (ChatItem::Info { .. }, ChatItem::Info { .. })
            | (ChatItem::Tool { .. }, ChatItem::Tool { .. })
    )
}

/// Checkbox marker for one plan entry.
fn plan_marker(status: agentloop_contracts::PlanStatus) -> &'static str {
    use agentloop_contracts::PlanStatus;
    match status {
        PlanStatus::Pending => "☐",
        PlanStatus::InProgress => "◐",
        PlanStatus::Completed => "☑",
        _ => "☐",
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

/// One line: `native · code · auto · <model> · 47% context · ↑12.3k ↓4.1k`
/// plus scrolled-up and cost suffixes. Busy state lives on the notification
/// line, errors in the transcript — neither renders here.
fn draw_status(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let model = app
        .session
        .model
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "default model".to_owned());
    let usage = app.status.total_usage;
    let session_mode = session_mode_label(app.session.session_mode);
    let permission = permission_mode_label(app.session.effective_permission_mode());

    let mut segments = vec![
        app.kind.to_string(),
        session_mode.to_owned(),
        permission.to_owned(),
    ];
    if let Some(budget) = app.thinking_budget.filter(|_| app.caps.reasoning_visible) {
        segments.push(format!("think:{}", fmt_thinking_budget_k(budget)));
    }
    segments.push(model);

    let mut spans = vec![Span::styled(segments.join(" · "), theme::STATUS)];
    if let Some((pct, style)) = context_percent(app) {
        spans.push(Span::styled(" · ", theme::STATUS));
        spans.push(Span::styled(format!("{pct}% context"), style));
    }
    if app.mcp_enabled > 0 {
        spans.push(Span::styled(
            format!(" · mcp:{}", app.mcp_enabled),
            theme::STATUS,
        ));
    }
    spans.push(Span::styled(
        format!(" · ↑{} ↓{}", fmt_k(usage.input), fmt_k(usage.output)),
        theme::STATUS,
    ));
    if !app.chat.scroll.follow {
        spans.push(Span::styled(
            " · scrolled up (End to follow)",
            theme::STATUS,
        ));
    }
    if let Some(cost) = app.status.last_cost_usd {
        spans.push(Span::styled(format!(" · ${cost:.4}"), theme::STATUS));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Context usage as a percentage of the current model's window, colored by
/// pressure. `None` until a turn completes or when the window is unknown.
fn context_percent(app: &App) -> Option<(u8, Style)> {
    let tokens = app.status.last_context_tokens?;
    let window = u64::from(context_window(app)?);
    if window == 0 {
        return None;
    }
    let pct = (tokens.saturating_mul(100) / window).min(100) as u8;
    let style = if pct < 50 {
        theme::SUCCESS
    } else if pct < 80 {
        theme::WARN
    } else {
        theme::ERROR
    };
    Some((pct, style))
}

/// The current model's context window from the catalog, falling back to
/// static model discovery.
fn context_window(app: &App) -> Option<u32> {
    let model = app.session.model.as_ref()?;
    if let Some(window) = app
        .catalog
        .iter()
        .find(|entry| entry.model_ref().0 == model.0)
        .and_then(|entry| entry.model.context_window)
    {
        return Some(window);
    }
    if let agentloop_contracts::ModelDiscovery::Static { models } = &app.caps.models {
        let (_, name) = model.split();
        return models
            .iter()
            .find(|info| info.id == name || info.id == model.0)
            .and_then(|info| info.context_window);
    }
    None
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

fn popup_list_layout(
    anchor_y: u16,
    anchor_x: u16,
    width: u16,
    match_count: usize,
) -> Option<(Rect, usize)> {
    if anchor_y == 0 {
        return None;
    }
    let list_rows = match_count.min(POPUP_LIST_MAX_ROWS);
    let desired_height = (list_rows as u16).saturating_add(2).max(3);
    let height = desired_height.min(anchor_y);
    if height < 3 {
        return None;
    }
    let visible_rows = height.saturating_sub(2) as usize;
    let area = Rect {
        x: anchor_x,
        y: anchor_y.saturating_sub(height),
        width: width.min(60),
        height,
    };
    Some((area, visible_rows.max(1)))
}

fn draw_command_popup(frame: &mut Frame<'_>, popup: &CommandPopup, input_area: Rect) {
    let Some((area, visible_rows)) = popup_list_layout(
        input_area.y,
        input_area.x,
        input_area.width,
        popup.matches.len(),
    ) else {
        return;
    };
    let scroll_offset = popup_list_scroll_offset(popup.selected, visible_rows, popup.matches.len());
    let items = popup
        .matches
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_rows)
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
    let position = format!(" {}/{} ", popup.selected + 1, popup.matches.len());
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" commands{position}")),
        ),
        area,
    );
}

fn draw_file_popup(frame: &mut Frame<'_>, popup: &FilePopup, input_area: Rect) {
    let mut anchor_y = input_area.y;
    if let Some(preview) = &popup.preview {
        anchor_y = draw_mention_preview(frame, preview, input_area.x, anchor_y, input_area.width);
    }
    if popup.matches.is_empty() {
        return;
    }
    let Some((area, visible_rows)) = popup_list_layout(
        anchor_y,
        input_area.x,
        input_area.width,
        popup.matches.len(),
    ) else {
        return;
    };
    let scroll_offset = popup_list_scroll_offset(popup.selected, visible_rows, popup.matches.len());
    let items = popup
        .matches
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_rows)
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
    let position = format!(" {}/{} ", popup.selected + 1, popup.matches.len());
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" files{position}")),
        ),
        area,
    );
}

fn draw_mention_preview(
    frame: &mut Frame<'_>,
    preview: &MentionPreview,
    x: u16,
    anchor_y: u16,
    width: u16,
) -> u16 {
    if anchor_y == 0 {
        return anchor_y;
    }
    let content_rows = if preview.error.is_some() {
        1usize
    } else {
        preview.lines.len().max(1)
    };
    let note_rows = usize::from(preview.truncated) + usize::from(preview.error.is_some());
    let body_rows = (content_rows + note_rows).min(MENTION_PREVIEW_MAX_LINES + 2);
    let height = (body_rows as u16).saturating_add(2).min(anchor_y);
    if height < 3 {
        return anchor_y;
    }
    let y = anchor_y.saturating_sub(height);
    let area = Rect {
        x,
        y,
        width: width.min(72),
        height,
    };
    let title = format!(" {} — {} ", preview.path, preview.label);
    let mut lines = Vec::new();
    if let Some(err) = &preview.error {
        lines.push(Line::from(Span::styled(err.clone(), theme::WARN)));
    } else if preview.lines.is_empty() {
        lines.push(Line::from(Span::styled("(empty range)", theme::DIM)));
    } else {
        for (num, line) in &preview.lines {
            lines.push(Line::from(vec![
                Span::styled(format!("{num:>4} "), theme::DIM),
                Span::raw(line.clone()),
            ]));
        }
        if preview.truncated {
            let hidden = preview.total_lines.saturating_sub(preview.lines.len());
            lines.push(Line::from(Span::styled(
                format!("… {hidden} more lines"),
                theme::DIM,
            )));
        }
    }
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: false }),
        area,
    );
    y
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
        Overlay::McpList(state) => draw_mcp_list(frame, state, root),
        Overlay::McpExplorer(state) => draw_mcp_explorer(frame, state, root),
        Overlay::McpInstall(state) => draw_mcp_install(frame, state, root),
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
        lines.push(Line::from(Span::styled(
            permission_label(*option, idx + 1),
            style,
        )));
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "enter confirm · 1-3 select · y allow · a always · esc/n deny",
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

fn permission_label(kind: PermissionDecisionKind, number: usize) -> String {
    let text = match kind {
        PermissionDecisionKind::AllowOnce => "Yes",
        PermissionDecisionKind::AllowAlways => "Yes, don't ask again",
        PermissionDecisionKind::Deny => "No (esc)",
        _ => "unknown",
    };
    format!("{number}. {text}")
}

fn draw_question(frame: &mut Frame<'_>, prompt: &QuestionPrompt, root: Rect) {
    let area = centered(root, 70, 60);
    let Some(question) = prompt.questions.get(prompt.current) else {
        return;
    };
    let multi = question.multi_select;
    let mut lines = vec![
        Line::from(Span::styled(question.header.clone(), theme::TITLE)),
        Line::from(question.question.clone()),
        Line::default(),
    ];
    for (idx, option) in question.options.iter().enumerate() {
        let picked = prompt.picks[prompt.current].contains(&idx);
        let cursor = idx == prompt.cursor && !prompt.custom_mode;
        let number = idx + 1;
        let marker = if multi {
            if picked { "[x]" } else { "[ ]" }
        } else if picked {
            "(*)"
        } else if cursor {
            "(>)"
        } else {
            "( )"
        };
        let style = if cursor || picked {
            theme::SELECTED
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{number}. {marker} {}", option.label), style),
            option
                .description
                .as_ref()
                .map(|description| Span::styled(format!(" — {description}"), theme::DIM))
                .unwrap_or_else(|| Span::raw("")),
        ]));
    }
    if question.allow_custom {
        lines.push(Line::default());
        let custom_style = if prompt.custom_mode {
            theme::SELECTED
        } else {
            theme::DIM
        };
        let custom_text = if prompt.custom_input.is_empty() {
            "type your answer…".to_owned()
        } else {
            prompt.custom_input.clone()
        };
        lines.push(Line::from(vec![
            Span::styled("Other: ", custom_style),
            Span::styled(custom_text, custom_style),
        ]));
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        question_hints(question),
        theme::DIM,
    )));
    if prompt.questions.len() > 1 {
        lines.push(Line::from(Span::styled(
            format!(
                "Question {} of {}",
                prompt.current + 1,
                prompt.questions.len()
            ),
            theme::DIM,
        )));
    }
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" question "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn question_hints(question: &Question) -> String {
    let mut parts = vec!["↑↓ move".to_owned()];
    if question.multi_select {
        parts.push("Space toggle".to_owned());
    } else if !question.options.is_empty() {
        parts.push("Space select".to_owned());
    }
    if !question.options.is_empty() {
        let n = question.options.len().min(9);
        parts.push(format!("1-{n} pick"));
    }
    if question.allow_custom {
        parts.push("type custom".to_owned());
    }
    parts.push("Enter confirm".to_owned());
    parts.push("Esc submit partial".to_owned());
    parts.join(" · ")
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
                for line in terminal_lines(output) {
                    lines.push(Line::from(line));
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
        Line::from("@ attach files · @path:[0:12] python slice preview · type @ to search"),
        Line::from("PgUp/PgDn or ↑/↓ (empty prompt) scroll · End follow · drag to select & copy"),
        Line::from("Ctrl+Shift+C or /copy — copy transcript · Ctrl+M — toggle mouse wheel scroll"),
        Line::from(
            "Ctrl+T expand/collapse thought · /thinking off|low|medium|high · /mode code|plan · /permissions require|auto|allow-all",
        ),
        Line::from(
            "Ctrl+O expand/collapse tool result · Tab cycle tool rows · Enter/Space toggle focused row",
        ),
        Line::from(
            "Shift+Tab cycle mode: require → accept edits → plan · Enter mid-turn queues the prompt",
        ),
        Line::default(),
        Line::from(Span::styled("Modes", theme::TITLE)),
        Line::from("require — ask before mutating tools · accept edits — file edits auto-allowed"),
        Line::from(
            "plan — read-only research · allow-all — /permissions allow-all (never in the cycle)",
        ),
        Line::default(),
        Line::from(Span::styled("Backends vs providers", theme::TITLE)),
        Line::from(
            "/provider switches the LLM API inside the native loop (incl. /connect custom hosts);",
        ),
        Line::from("/agent swaps the whole backend (native loop vs external claude/copilot CLIs)."),
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

fn draw_mcp_list(frame: &mut Frame<'_>, state: &McpListState, root: Rect) {
    let area = centered(root, 72, 60);
    let filter = state.filter.to_lowercase();
    let visible: Vec<_> = state
        .items
        .iter()
        .filter(|item| {
            filter.is_empty()
                || item.name.to_lowercase().contains(&filter)
                || item.source.to_lowercase().contains(&filter)
        })
        .collect();
    let items = visible
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let style = if idx == state.selected {
                theme::SELECTED
            } else {
                Style::default()
            };
            let status = if item.enabled { "on" } else { "off" };
            ListItem::new(Line::from(vec![
                Span::styled(format!("[{status}] {}", item.name), style),
                Span::raw(" "),
                Span::styled(item.source.clone(), theme::DIM),
            ]))
        })
        .collect::<Vec<_>>();
    frame.render_widget(Clear, area);
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" MCP servers ")
                .title_bottom("Space toggle · Enter save · /mcp <name> attach"),
        ),
        area,
    );
}

fn draw_mcp_explorer(frame: &mut Frame<'_>, state: &McpExplorerState, root: Rect) {
    let area = centered(root, 78, 65);
    let title = format!(" MCP explore: {} ", state.server);
    let lines = match &state.phase {
        McpExplorerPhase::Loading => vec![Line::from("connecting and listing tools…")],
        McpExplorerPhase::Calling => vec![Line::from("calling tool…")],
        McpExplorerPhase::Failed { message } => {
            vec![Line::from(Span::styled(message, theme::ERROR))]
        }
        McpExplorerPhase::Result { output, .. } => terminal_lines(output)
            .into_iter()
            .map(Line::from)
            .collect::<Vec<_>>(),
        McpExplorerPhase::Tools { tools } => {
            if state.args_mode {
                vec![
                    Line::from(Span::styled("JSON arguments:", theme::TITLE)),
                    Line::from(state.args_input.clone()),
                    Line::default(),
                    Line::from(Span::styled("Enter call · Esc back", theme::DIM)),
                ]
            } else {
                let filter = state.filter.to_lowercase();
                tools
                    .iter()
                    .filter(|tool| {
                        filter.is_empty()
                            || tool.name.to_lowercase().contains(&filter)
                            || tool.description.to_lowercase().contains(&filter)
                    })
                    .enumerate()
                    .map(|(idx, tool)| {
                        let style = if idx == state.selected {
                            theme::SELECTED
                        } else {
                            Style::default()
                        };
                        Line::from(vec![
                            Span::styled(tool.name.clone(), style),
                            Span::raw(" — "),
                            Span::styled(tool.description.clone(), theme::DIM),
                        ])
                    })
                    .collect()
            }
        }
    };
    let scroll = if matches!(state.phase, McpExplorerPhase::Result { .. }) {
        state.scroll
    } else {
        0
    };
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: false })
            .scroll((scroll as u16, 0)),
        area,
    );
}

fn draw_mcp_install(frame: &mut Frame<'_>, state: &McpInstallState, root: Rect) {
    let area = centered(root, 74, 62);
    let mode_label = match state.mode {
        McpInstallMode::Registry => "registry",
        McpInstallMode::Npm => "npm package",
        McpInstallMode::Import => "import file",
    };
    let mut lines = vec![
        Line::from(Span::styled(
            format!("mode: {mode_label} (Tab to switch)"),
            theme::TITLE,
        )),
        Line::default(),
    ];
    match state.mode {
        McpInstallMode::Registry => {
            let entries = agentloop_cli_core::registry();
            let filter = state.filter.to_lowercase();
            for (idx, entry) in entries
                .iter()
                .filter(|entry| {
                    filter.is_empty()
                        || entry.name.contains(&filter)
                        || entry.label.to_lowercase().contains(&filter)
                })
                .enumerate()
            {
                let style = if idx == state.selected {
                    theme::SELECTED
                } else {
                    Style::default()
                };
                lines.push(Line::from(vec![
                    Span::styled(entry.label.clone(), style),
                    Span::raw(" "),
                    Span::styled(entry.npm.clone(), theme::DIM),
                ]));
                lines.push(Line::from(Span::styled(
                    entry.description.clone(),
                    theme::DIM,
                )));
            }
        }
        McpInstallMode::Npm | McpInstallMode::Import => {
            let prompt = if state.mode == McpInstallMode::Npm {
                "package name (e.g. @scope/pkg)"
            } else {
                "path to mcpServers JSON"
            };
            lines.push(Line::from(prompt));
            if state.input_mode {
                lines.push(Line::from(state.input.clone()));
            } else {
                lines.push(Line::from(Span::styled(
                    "Enter to type · Esc cancel",
                    theme::DIM,
                )));
            }
        }
    }
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" mcp-install "),
            )
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
