//! Pure ratatui rendering for the app state.

pub(crate) mod diff;
mod highlight;
mod markdown;
mod thinking;
mod tool_view;

pub use markdown::MarkdownCache;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, Padding, Paragraph, Wrap,
};

use agentloop_contracts::{PermissionDecisionKind, Question};

use crate::app::{App, AppRoute, TurnPhase, permission_mode_label, session_mode_label};
use crate::chat::{ChatItem, DraftBlock, SubagentOutcome};
use crate::files::{MENTION_PREVIEW_MAX_LINES, MentionPreview};
use crate::input::{
    CommandPopup, FilePopup, InputPopup, POPUP_LIST_MAX_ROWS, popup_list_scroll_offset,
};
use crate::overlay::{
    CommandPaletteState, ConfirmPrompt, ConnectGalleryRow, ConnectWizardState, ConnectWizardStep,
    LoginState, McpExplorerPhase, McpExplorerState, McpInstallMode, McpInstallState, McpListState,
    Overlay, PermissionPrompt, PickerState, QuestionPrompt, ShellCommandOverlay, ShellCommandPhase,
};
use crate::terminal_text::terminal_lines;
use crate::theme;

/// Width of the activity sidebar, and the minimum chat width below which it is
/// suppressed so narrow terminals aren't cramped.
const SIDEBAR_WIDTH: u16 = 34;
const SIDEBAR_MIN_TOTAL_WIDTH: u16 = 90;

/// Draw one full frame: chat, an optional notification line (busy pulse or
/// newest toast), the input box, and the status bar.
pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    let input_height = input_height(app);
    let notify = notification_line_visible(app);
    let queue = !app.queued_prompts.is_empty();
    let mut constraints = vec![Constraint::Min(1)];
    if notify {
        constraints.push(Constraint::Length(1));
    }
    if queue {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(input_height));
    constraints.push(Constraint::Length(1));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut idx = 0;
    let chat_area = chunks[idx];
    idx += 1;
    let notify_area = notify.then(|| {
        let area = chunks[idx];
        idx += 1;
        area
    });
    let queue_area = queue.then(|| {
        let area = chunks[idx];
        idx += 1;
        area
    });
    let input_area = chunks[idx];
    let status_area = chunks[idx + 1];
    let (chat_area, sidebar_area) = if sidebar_visible(app, chat_area) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(SIDEBAR_WIDTH)])
            .split(chat_area);
        (cols[0], Some(cols[1]))
    } else {
        (chat_area, None)
    };
    draw_chat(frame, app, chat_area);
    if let Some(sidebar_area) = sidebar_area {
        draw_sidebar(frame, app, sidebar_area);
    }
    if let Some(area) = notify_area {
        draw_notification_line(frame, app, area);
    }
    if let Some(queue_area) = queue_area {
        draw_queue_banner(frame, app, queue_area);
    }
    draw_input(frame, app, input_area);
    draw_status(frame, app, status_area);
    draw_popup(frame, app, input_area);
    draw_overlay(frame, app, area, input_area);
}

/// One-line banner for prompts queued while a turn runs.
fn draw_queue_banner(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let count = app.queued_prompts.len();
    let noun = if count == 1 { "message" } else { "messages" };
    let line = Line::from(vec![
        Span::styled(format!("{count} {noun} queued"), theme::warn()),
        Span::styled(" · esc to clear queue", theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

/// The notification line stays reserved while a turn runs or a toast is
/// alive, so the layout doesn't jitter between busy and toast states.
fn notification_line_visible(app: &App) -> bool {
    app.session.turn.is_running() || !app.status.toasts.is_empty()
}

/// Busy line (priority) or the newest toast. The busy pulse/timer is
/// suppressed while the agent is blocked on a decision (permission/question) —
/// it isn't working, it's waiting on the user, so a ticking timer would lie.
fn draw_notification_line(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let awaiting_decision = matches!(app.overlay, Overlay::Permission(_) | Overlay::Question(_));
    let line = match app.session.turn {
        TurnPhase::Running { started } if !awaiting_decision => {
            let glyph = theme::pulse_frame(app.status.spinner);
            let verb = theme::spinner_verb(app.status.turn_verb_idx);
            let tokens = app.status.turn_output_chars / 4;
            Line::from(vec![
                Span::styled(format!("{glyph} {verb}… "), theme::warn()),
                Span::styled(
                    format!(
                        "({}s · ↑ {} tokens · esc to interrupt)",
                        started.elapsed().as_secs(),
                        fmt_k(tokens)
                    ),
                    theme::dim(),
                ),
            ])
        }
        _ => match app.status.toasts.back() {
            Some(toast) => Line::from(Span::styled(toast.text.clone(), theme::dim())),
            None => return,
        },
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
    (lines as u16 + 3).clamp(4, 9)
}

fn draw_chat(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    if app.is_home_route() {
        draw_home_centered(frame, app, area);
        return;
    }
    let sidebar_shown = sidebar_visible(app, area);
    let lines = chat_lines(app, area.width, sidebar_shown);
    if app.chat.scroll.follow {
        app.chat.scroll.offset_from_bottom = 0;
    }

    let (_, _, max_offset) = chat_viewport_metrics(&lines, area);
    app.chat.scroll.clamp_offset(max_offset);
    let scroll_top = max_offset.saturating_sub(app.chat.scroll.offset_from_bottom);

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll_top as u16, 0));
    frame.render_widget(paragraph, area);
}

/// Empty home: centered brand block, no transcript scroll area.
fn draw_home_centered(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let sidebar_shown = sidebar_visible(app, area);
    // #region agent log
    let splash_rotate = app.status.spinner;
    let hint_idx = (splash_rotate / crate::input::ROTATE_HINT_TICKS) % 3;
    crate::debug_agent::log_splash_hint_if_changed(splash_rotate, hint_idx);
    // #endregion
    let lines = splash_lines(
        &app.engine_name,
        &app.engine_version,
        &app.workdir.display().to_string(),
        true,
        splash_rotate,
        app.show_getting_started() && sidebar_shown,
    );
    let block = Block::default();
    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

/// Scroll budget for the chat pane: `(total_wrapped, viewport, max_offset)`.
/// The pane has no border, so it uses the full area.
fn chat_viewport_metrics(lines: &[Line<'_>], area: Rect) -> (usize, usize, usize) {
    let total_lines = wrapped_line_count(lines, area.width);
    let viewport_lines = area.height as usize;
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

fn chat_lines(app: &mut App, viewport_width: u16, sidebar_shown: bool) -> Vec<Line<'static>> {
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
                            Span::styled("> ", theme::dim()),
                            Span::styled(line.to_owned(), theme::user_text()),
                        ]));
                    } else {
                        lines.push(Line::from(Span::styled(
                            format!("  {line}"),
                            theme::user_text(),
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
                    lines.push(Line::from(Span::styled("  ▌", theme::dim())));
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
                lines.push(Line::from(Span::styled("plan", theme::dim())));
                for entry in entries {
                    lines.push(Line::from(Span::styled(
                        format!("  {} {}", plan_marker(entry.status), entry.content),
                        theme::dim(),
                    )));
                }
            }
            ChatItem::Info { text } => {
                // The interrupt marker reads as a soft error, not plain info.
                let style = if text == crate::app::INTERRUPT_NOTE {
                    theme::error().add_modifier(Modifier::DIM)
                } else {
                    theme::dim()
                };
                lines.push(Line::from(Span::styled(text.clone(), style)));
            }
            ChatItem::Splash { name, version, cwd } => {
                let home = app.home_screen && app.chat.is_home_screen();
                lines.extend(splash_lines(
                    name,
                    version,
                    cwd,
                    home,
                    app.status.spinner,
                    app.show_getting_started() && sidebar_shown,
                ));
                if home && app.show_getting_started() && !sidebar_shown {
                    lines.extend(home_getting_started_card(viewport_width));
                }
            }
            ChatItem::Error { headline, detail } => {
                lines.push(Line::from(Span::styled(
                    format!("✗ {headline}"),
                    theme::error(),
                )));
                if let Some(detail) = detail {
                    lines.push(Line::from(Span::styled(
                        detail.clone(),
                        theme::error().add_modifier(Modifier::DIM),
                    )));
                }
            }
            ChatItem::Subagent {
                role,
                model,
                tool_count,
                tokens,
                last_activity,
                duration_ms,
                outcome,
                ..
            } => {
                let badge = role.as_deref().unwrap_or("subagent");
                let mut segments: Vec<String> = Vec::new();
                match outcome {
                    SubagentOutcome::Running => {
                        if let Some(model) = model {
                            segments.push(model.clone());
                        }
                        segments.push("running".to_owned());
                    }
                    SubagentOutcome::Done => segments.push(match duration_ms {
                        Some(ms) => format!("done in {}s", ms / 1000),
                        None => "done".to_owned(),
                    }),
                    SubagentOutcome::Failed => segments.push(match duration_ms {
                        Some(ms) => format!("failed in {}s", ms / 1000),
                        None => "failed".to_owned(),
                    }),
                    SubagentOutcome::Cancelled => segments.push("cancelled".to_owned()),
                }
                if *tool_count > 0 {
                    let noun = if *tool_count == 1 { "tool" } else { "tools" };
                    segments.push(format!("{tool_count} {noun}"));
                }
                if *tokens > 0 {
                    segments.push(format!("{} tok", format_tokens(*tokens)));
                }
                if *outcome == SubagentOutcome::Running {
                    if let Some(activity) = last_activity {
                        segments.push(activity.clone());
                    }
                }
                let style = match outcome {
                    SubagentOutcome::Failed => theme::error(),
                    _ => theme::dim(),
                };
                lines.push(Line::from(Span::styled(
                    format!("  ⎿ [{badge}] {}", segments.join(" · ")),
                    style,
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
            theme::dim(),
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

/// The welcome splash: accent brand mark, version, cwd, and key hints.
fn splash_lines(
    name: &str,
    version: &str,
    cwd: &str,
    home: bool,
    rotate: usize,
    sidebar_getting_started: bool,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  ✻ ", theme::accent()),
            Span::styled(name.to_owned(), theme::title()),
            Span::styled(format!("  v{version}"), theme::dim()),
        ]),
        Line::from(Span::styled(
            "     agentic coding in your terminal".to_owned(),
            theme::dim(),
        )),
        Line::from(""),
        Line::from(Span::styled(format!("  {cwd}"), theme::dim())),
    ];
    if home && !sidebar_getting_started {
        let hints = [
            "Try: \"explain this repo\" or \"fix the failing test\"",
            "Ctrl+P opens the command palette · /sessions resumes past work",
            "/model picks a model · shift+tab cycles code/plan modes",
        ];
        let idx = (rotate / crate::input::ROTATE_HINT_TICKS) % hints.len();
        lines.push(Line::from(Span::styled(
            format!("  {}", hints[idx]),
            theme::accent(),
        )));
        lines.push(Line::from(Span::styled(
            "  /help · Ctrl+P · /sessions · @ files".to_owned(),
            theme::dim(),
        )));
    } else if home {
        lines.push(Line::from(Span::styled(
            "  /help · Ctrl+P · /sessions · @ files".to_owned(),
            theme::dim(),
        )));
    }
    lines
}

/// Compact getting-started card for narrow terminals (sidebar hidden).
fn home_getting_started_card(width: u16) -> Vec<Line<'static>> {
    let inner = width.saturating_sub(6) as usize;
    let border = "─".repeat(inner.min(40));
    vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  ┌", theme::border()),
            Span::styled(border.clone(), theme::border()),
            Span::styled("┐", theme::border()),
        ]),
        Line::from(vec![
            Span::styled("  │ ", theme::border()),
            Span::styled("Connect a provider to get started", theme::title()),
        ]),
        Line::from(vec![
            Span::styled("  │ ", theme::border()),
            Span::styled("Run ", theme::dim()),
            Span::styled("/connect", theme::accent()),
            Span::styled(" to pick a model", theme::dim()),
        ]),
        Line::from(vec![
            Span::styled("  └", theme::border()),
            Span::styled(border, theme::border()),
            Span::styled("┘", theme::border()),
        ]),
    ]
}

/// Whether to show the context sidebar: wide terminals always get it.
fn sidebar_visible(_app: &App, chat_area: Rect) -> bool {
    chat_area.width >= SIDEBAR_MIN_TOTAL_WIDTH
}

/// Tool calls currently executing or awaiting permission, newest last.
fn running_tools(app: &App) -> Vec<&agentloop_contracts::ToolCall> {
    app.chat
        .items
        .iter()
        .filter_map(|item| match item {
            ChatItem::Tool { call, .. }
                if matches!(
                    call.status,
                    agentloop_contracts::ToolCallStatus::Running
                        | agentloop_contracts::ToolCallStatus::AwaitingPermission { .. }
                ) =>
            {
                Some(call.as_ref())
            }
            _ => None,
        })
        .collect()
}

/// Subagents still running.
fn active_subagents(app: &App) -> Vec<&ChatItem> {
    app.chat
        .items
        .iter()
        .filter(|item| {
            matches!(
                item,
                ChatItem::Subagent {
                    outcome: SubagentOutcome::Running,
                    ..
                }
            )
        })
        .collect()
}

/// The most recent plan entries, if any.
fn latest_plan(app: &App) -> Option<&[agentloop_contracts::PlanEntry]> {
    app.chat.items.iter().rev().find_map(|item| match item {
        ChatItem::Plan { entries, .. } => Some(entries.as_slice()),
        _ => None,
    })
}

/// The right-hand context panel: usage, MCP, optional activity, getting started,
/// and cwd/branch/version footer.
fn draw_sidebar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let inner_width = area.width.saturating_sub(4) as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();

    if app.route == AppRoute::Session {
        if let Some(label) = app.session_label.as_deref() {
            lines.push(Line::from(Span::styled(
                truncate_cells(label, inner_width),
                theme::title(),
            )));
            lines.push(Line::from(""));
        }
    }

    // ── context ──
    lines.push(section_header("context"));
    let usage = app.status.total_usage;
    let total_tokens = usage.input + usage.output;
    let mut ctx_line = format!("  {} tokens", format_tokens(total_tokens));
    if let Some((pct, _)) = context_percent(app) {
        ctx_line.push_str(&format!(" · {pct}%"));
    }
    lines.push(Line::from(Span::styled(ctx_line, theme::dim())));
    if let Some(cost) = app.status.last_cost_usd {
        lines.push(Line::from(Span::styled(
            format!("  ${cost:.4} session cost"),
            theme::dim(),
        )));
    }
    let model = app
        .session
        .model
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "no model".to_owned());
    lines.push(Line::from(Span::styled(
        truncate_cells(&format!("  {} · {}", app.kind, model), inner_width),
        theme::dim(),
    )));

    // ── mcp ──
    lines.push(Line::from(""));
    lines.push(section_header("mcp"));
    let mcp_total = app.mcp_store.servers.len();
    let mcp_enabled = app.mcp_enabled;
    if mcp_total == 0 {
        lines.push(Line::from(Span::styled(
            "  none installed".to_owned(),
            theme::dim(),
        )));
        lines.push(Line::from(Span::styled(
            "  /mcp-install".to_owned(),
            theme::accent(),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            format!("  {mcp_enabled}/{mcp_total} connected"),
            theme::dim(),
        )));
    }

    // ── activity (when busy) ──
    let tools = running_tools(app);
    let subagents = active_subagents(app);
    let plan = latest_plan(app);
    let has_activity = !tools.is_empty()
        || !subagents.is_empty()
        || plan.is_some_and(|entries| !entries.is_empty());
    if has_activity {
        lines.push(Line::from(""));
        lines.push(section_header("activity"));
        if tools.is_empty() {
            lines.push(Line::from(Span::styled("  idle".to_owned(), theme::dim())));
        } else {
            for call in &tools {
                let summary = crate::tool_output::tool_summary(&call.tool_name, &call.input);
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{} ", theme::spinner_frame(app.status.spinner)),
                        theme::tool_running(),
                    ),
                    Span::styled(
                        truncate_cells(&summary, inner_width.saturating_sub(2)),
                        theme::assistant(),
                    ),
                ]));
            }
        }
        if !subagents.is_empty() {
            for item in &subagents {
                if let ChatItem::Subagent {
                    role,
                    tool_count,
                    last_activity,
                    ..
                } = item
                {
                    let badge = role.as_deref().unwrap_or("subagent");
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{} ", theme::spinner_frame(app.status.spinner)),
                            theme::tool(),
                        ),
                        Span::styled(badge.to_owned(), theme::tool()),
                        Span::styled(format!(" · {tool_count} tools"), theme::dim()),
                    ]));
                    if let Some(activity) = last_activity {
                        lines.push(Line::from(Span::styled(
                            format!(
                                "    {}",
                                truncate_cells(activity, inner_width.saturating_sub(4))
                            ),
                            theme::dim(),
                        )));
                    }
                }
            }
        }
        if let Some(entries) = plan.filter(|entries| !entries.is_empty()) {
            for entry in entries {
                lines.push(Line::from(Span::styled(
                    format!(
                        "  {} {}",
                        plan_marker(entry.status),
                        truncate_cells(&entry.content, inner_width.saturating_sub(4))
                    ),
                    theme::dim(),
                )));
            }
        }
    }

    // ── getting started ──
    if app.show_getting_started() && (app.is_home_route() || app.route == AppRoute::Session) {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "┌ getting started ─",
            theme::border(),
        )));
        lines.push(Line::from(Span::styled(
            "│ Connect a provider".to_owned(),
            theme::title(),
        )));
        lines.push(Line::from(vec![
            Span::styled("│ ", theme::border()),
            Span::styled("/connect", theme::accent()),
        ]));
        lines.push(Line::from(Span::styled(
            "└──────────────────",
            theme::border(),
        )));
    }

    // ── footer: path · branch · version ──
    lines.push(Line::from(""));
    let cwd = abbreviate_home(&app.workdir.display().to_string());
    let branch = app.git_branch.as_deref().unwrap_or("no branch");
    lines.push(Line::from(Span::styled(
        truncate_cells(&format!("{cwd}:{branch}"), inner_width),
        theme::dim(),
    )));
    lines.push(Line::from(Span::styled(
        format!("{} v{}", app.engine_name, app.engine_version),
        theme::dim(),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border())
        .padding(Padding::horizontal(1))
        .title(Span::styled(" context ", theme::title()));
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn abbreviate_home(path: &str) -> String {
    if let Ok(home) = std::env::var("HOME") {
        let home = home.trim_end_matches('/');
        if path == home {
            return "~".to_owned();
        }
        if let Some(rest) = path.strip_prefix(&format!("{home}/")) {
            return format!("~/{rest}");
        }
    }
    path.to_owned()
}

/// A dim, uppercase-ish section label for the sidebar.
fn section_header(label: &str) -> Line<'static> {
    Line::from(Span::styled(
        label.to_owned(),
        theme::accent().add_modifier(Modifier::BOLD),
    ))
}

/// Truncate `text` to at most `max` display columns, adding an ellipsis.
fn truncate_cells(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let keep = max.saturating_sub(1);
    let mut out: String = text.chars().take(keep).collect();
    out.push('…');
    out
}

/// Consecutive Info lines, consecutive Tool rows, and Tool/Subagent runs
/// (a parallel Task batch with nested subagent rows) group tightly: no
/// blank line between them.
fn tight_group(items: &[ChatItem], idx: usize) -> bool {
    let Some(next) = items.get(idx + 1) else {
        return false;
    };
    matches!(
        (&items[idx], next),
        (ChatItem::Info { .. }, ChatItem::Info { .. })
            | (ChatItem::Tool { .. }, ChatItem::Tool { .. })
            | (ChatItem::Tool { .. }, ChatItem::Subagent { .. })
            | (ChatItem::Subagent { .. }, ChatItem::Tool { .. })
            | (ChatItem::Subagent { .. }, ChatItem::Subagent { .. })
    )
}

/// Compact token count: `12_400` → `"12.4k"`, values under 1000 verbatim.
fn format_tokens(n: u64) -> String {
    if n < 1000 {
        return n.to_string();
    }
    let k = n as f64 / 1000.0;
    if k < 100.0 {
        format!("{k:.1}k")
    } else {
        format!("{k:.0}k")
    }
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
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border())
        .title(Span::styled(" prompt ", theme::title()));
    frame.render_widget(block, area);
    let inner = area.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    let chrome_h = 1u16;
    let input_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(chrome_h)])
        .split(inner);
    frame.render_widget(&app.input.textarea, input_chunks[0]);
    draw_prompt_chrome(frame, app, input_chunks[1]);
}

fn draw_prompt_chrome(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let mode = session_mode_label(app.session.session_mode);
    let model = app
        .session
        .model
        .as_ref()
        .map(|m| m.0.as_str())
        .unwrap_or("default");
    let provider = app
        .session
        .model
        .as_ref()
        .and_then(|m| m.0.split_once('/').map(|(p, _)| p))
        .or_else(|| app.providers.first().map(String::as_str))
        .unwrap_or("no provider");
    let line = Line::from(vec![
        Span::styled(format!("{mode} · {model} · {provider}"), theme::dim()),
        Span::styled(" · tab mode · ctrl+p commands", theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

/// One line: `native · code · auto · <model> · 47% context · ↑12.3k ↓4.1k`
/// plus scrolled-up and cost suffixes. Busy state lives on the notification
/// line, errors in the transcript — neither renders here.
fn draw_status(frame: &mut Frame<'_>, app: &App, area: Rect) {
    if app.is_home_route() {
        let mut spans = vec![Span::styled(
            format!("{} v{}", app.engine_name, app.engine_version),
            theme::dim(),
        )];
        if area.width >= 100 {
            spans.push(Span::styled(" · ", theme::dim()));
            spans.push(Span::styled("ctrl+p commands", theme::dim()));
        } else if app.providers.is_empty() {
            let phase = (app.status.spinner / 50) % 3;
            if phase < 2 {
                spans.push(Span::styled(" · ", theme::dim()));
                spans.push(Span::styled("connect /connect", theme::dim()));
            }
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
        return;
    }
    let model = app
        .session
        .model
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "default model".to_owned());
    let usage = app.status.total_usage;
    let session_mode = session_mode_label(app.session.session_mode);
    let permission = permission_mode_label(app.session.effective_permission_mode());

    let mut segments = vec![app.kind.to_string(), session_mode.to_owned()];
    // In plan mode the effective permission is forced to "plan" as well, so
    // showing both would read as a redundant "plan · plan". Only surface the
    // permission when it adds information beyond the session mode.
    if permission != session_mode {
        segments.push(permission.to_owned());
    }
    if let Some(budget) = app.thinking_budget.filter(|_| app.caps.reasoning_visible) {
        segments.push(format!("think:{}", fmt_thinking_budget_k(budget)));
    }

    // The model is the status bar's focal point — give it the accent color.
    let mut spans = vec![
        Span::styled(segments.join(" · "), theme::status()),
        Span::styled(" · ", theme::status()),
        Span::styled(model, theme::accent()),
    ];
    // DeepSeek "auto" orchestration is active whenever a DeepSeek model is the
    // session model: research subagents run v4-flash, implementation v4-pro
    // (the model picker's "auto" entry lands here). Surface it so the smart
    // split is visible rather than looking like a plain single-model session.
    // Labelled `ds-auto`, not `auto`, so it can't be confused with the
    // accept-edits permission mode (which already renders as `auto`).
    if app
        .session
        .model
        .as_ref()
        .is_some_and(|model| model.0.starts_with("deepseek/"))
    {
        spans.push(Span::styled(" · ", theme::status()));
        spans.push(Span::styled("ds-auto", theme::accent()));
    }
    if let Some((pct, style)) = context_percent(app) {
        spans.push(Span::styled(" · ", theme::status()));
        spans.push(Span::styled(context_gauge(pct), style));
        spans.push(Span::styled(format!(" {pct}%"), style));
    }
    if app.mcp_enabled > 0 {
        spans.push(Span::styled(
            format!(" · mcp:{}", app.mcp_enabled),
            theme::status(),
        ));
    }
    spans.push(Span::styled(
        format!(" · ↑{} ↓{}", fmt_k(usage.input), fmt_k(usage.output)),
        theme::status(),
    ));
    if !app.chat.scroll.follow {
        spans.push(Span::styled(
            " · scrolled up (End to follow)",
            theme::status(),
        ));
    }
    if let Some(cost) = app.status.last_cost_usd {
        spans.push(Span::styled(format!(" · ${cost:.4}"), theme::status()));
    }
    if area.width >= 100 {
        spans.push(Span::styled(" · ", theme::status()));
        spans.push(Span::styled("ctrl+p commands", theme::dim()));
    } else if app.providers.is_empty() {
        // 5s on / 10s off at 100ms ticks → 50 on, 100 off (15s cycle).
        let phase = (app.status.spinner / 50) % 3;
        if phase < 2 {
            spans.push(Span::styled(" · ", theme::status()));
            spans.push(Span::styled("connect /connect", theme::dim()));
        }
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// A compact block gauge for a `0..=100` percentage, e.g. `▕████░░░░▏`.
fn context_gauge(pct: u8) -> String {
    const WIDTH: usize = 8;
    let filled = ((pct as usize * WIDTH) + 50) / 100;
    let filled = filled.min(WIDTH);
    format!("▕{}{}▏", "█".repeat(filled), "░".repeat(WIDTH - filled))
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
        theme::success()
    } else if pct < 80 {
        theme::warn()
    } else {
        theme::error()
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
    let filter_empty = popup.filter.is_empty();
    let cmd_width = popup
        .matches
        .iter()
        .map(|e| e.name.len() + 1)
        .max()
        .unwrap_or(8)
        .max(8);
    let row_width = area.width.saturating_sub(2) as usize;
    let scroll_offset = popup_list_scroll_offset(popup.selected, visible_rows, popup.matches.len());
    let mut items = Vec::new();
    let mut last_category: Option<&str> = None;
    for (idx, entry) in popup
        .matches
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_rows)
    {
        if filter_empty && last_category != Some(entry.category) {
            items.push(ListItem::new(Line::from(Span::styled(
                command_category_label(entry.category),
                theme::accent().add_modifier(Modifier::BOLD),
            ))));
            last_category = Some(entry.category);
        }
        let cmd = format!("/{:<width$}", entry.name, width = cmd_width);
        let mut spans = vec![
            Span::raw(cmd),
            Span::raw(" "),
            Span::styled(entry.description.clone(), theme::dim()),
        ];
        if filter_empty {
            spans.push(Span::styled(format!(" [{}]", entry.source), theme::dim()));
        }
        let line = Line::from(spans);
        if idx == popup.selected {
            items.push(ListItem::new(pad_line_to_width(line, row_width)).style(theme::selected()));
        } else {
            items.push(ListItem::new(line));
        }
    }
    frame.render_widget(Clear, area);
    let position = format!(" {}/{} ", popup.selected + 1, popup.matches.len());
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(format!(" commands{position}")),
        ),
        area,
    );
}

fn command_category_label(category: &str) -> String {
    match category {
        "session" => "Session".to_owned(),
        "provider" => "Provider".to_owned(),
        "mcp" => "MCP".to_owned(),
        "workspace" => "Workspace".to_owned(),
        "ui" => "UI".to_owned(),
        _ => "Other".to_owned(),
    }
}

fn pad_line_to_width(mut line: Line<'static>, width: usize) -> Line<'static> {
    let mut used = 0usize;
    for span in &line.spans {
        used += span.content.chars().count();
    }
    if used < width {
        line.spans.push(Span::raw(" ".repeat(width - used)));
    }
    line
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
                theme::selected()
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled("@", theme::dim()),
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
                .border_type(BorderType::Rounded)
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
        lines.push(Line::from(Span::styled(err.clone(), theme::warn())));
    } else if preview.lines.is_empty() {
        lines.push(Line::from(Span::styled("(empty range)", theme::dim())));
    } else {
        for (num, line) in &preview.lines {
            lines.push(Line::from(vec![
                Span::styled(format!("{num:>4} "), theme::dim()),
                Span::raw(line.clone()),
            ]));
        }
        if preview.truncated {
            let hidden = preview.total_lines.saturating_sub(preview.lines.len());
            lines.push(Line::from(Span::styled(
                format!("… {hidden} more lines"),
                theme::dim(),
            )));
        }
    }
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(theme::border())
                    .title(title),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
    y
}

fn draw_overlay(frame: &mut Frame<'_>, app: &App, root: Rect, input_area: Rect) {
    match &app.overlay {
        Overlay::None => {}
        Overlay::Picker(picker) => draw_picker(frame, picker, root),
        // Decisions render inline, anchored just above the input (Claude
        // Code-style), so they sit in the conversation flow rather than
        // floating in the middle of the screen.
        Overlay::Permission(prompt) => draw_permission(frame, prompt, root, input_area),
        Overlay::Question(prompt) => draw_question(frame, prompt, root),
        Overlay::Login(state) => draw_login(frame, state, root),
        Overlay::Help => draw_help(frame, app, root),
        Overlay::ShellCommand(state) => draw_shell_command(frame, state, app, root),
        Overlay::Confirm(prompt) => draw_confirm(frame, prompt, root, input_area),
        Overlay::McpList(state) => draw_mcp_list(frame, state, root),
        Overlay::McpExplorer(state) => draw_mcp_explorer(frame, state, root),
        Overlay::McpInstall(state) => draw_mcp_install(frame, state, root),
        Overlay::CommandPalette(state) => draw_command_palette(frame, state, root),
        Overlay::WhichKey => draw_which_key(frame, app, root, input_area),
        Overlay::ConnectWizard(state) => draw_connect_wizard(frame, state, app, root),
    }
}

/// A box anchored directly above the input, aligned to its width — for inline
/// decisions (permission, confirm). Grows upward from the input by the content
/// height (+2 for borders), clamped to the room above.
fn bottom_anchored(root: Rect, input_area: Rect, content_lines: usize) -> Rect {
    let desired = (content_lines as u16).saturating_add(2);
    let available = input_area.y.saturating_sub(root.y);
    let height = desired.min(available).max(3);
    Rect {
        x: input_area.x,
        y: input_area.y.saturating_sub(height),
        width: input_area.width,
        height,
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
                theme::dim()
            } else if idx == picker.selected {
                theme::selected()
            } else {
                Style::default()
            };
            let mut spans = vec![Span::styled(item.label.clone(), style)];
            if let Some(detail) = &item.detail {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(detail.clone(), theme::dim()));
            }
            ListItem::new(Line::from(spans))
        })
        .collect::<Vec<_>>();
    frame.render_widget(Clear, area);
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme::border())
                .title(format!(" {}  filter: {} ", picker.title, picker.filter)),
        ),
        area,
    );
}

fn draw_permission(frame: &mut Frame<'_>, prompt: &PermissionPrompt, root: Rect, input_area: Rect) {
    let mut title_spans = Vec::new();
    if let Some(role) = &prompt.role {
        title_spans.push(Span::styled(format!("[{role}] "), theme::warn()));
    }
    title_spans.push(Span::styled(prompt.title.clone(), theme::title()));
    let mut lines = vec![Line::from(title_spans)];
    if let Some(preview) = &prompt.diff {
        let total = preview.lines.len();
        let shown = if prompt.diff_expanded {
            total
        } else {
            preview.preview_len()
        };
        for line in &preview.lines[..shown] {
            lines.push(diff_permission_line(line));
        }
        if !prompt.diff_expanded && total > shown {
            lines.push(Line::from(Span::styled(
                format!("… +{} lines (d to expand)", total - shown),
                theme::dim(),
            )));
        } else if prompt.diff_expanded && total > preview.preview_len() {
            lines.push(Line::from(Span::styled(
                "(d to collapse)".to_owned(),
                theme::dim(),
            )));
        }
    } else if let Some(detail) = &prompt.detail {
        lines.push(Line::default());
        lines.push(Line::from(detail.clone()));
    }
    lines.push(Line::default());
    for (idx, option) in prompt.options.iter().enumerate() {
        let style = if idx == prompt.selected {
            theme::selected()
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(
            permission_label(*option, idx + 1),
            style,
        )));
    }
    lines.push(Line::default());
    let mut footer = "enter confirm · 1-3 select · y allow · a always · esc/n deny".to_owned();
    if prompt.diff.is_some() {
        footer.push_str(" · d diff");
    }
    lines.push(Line::from(Span::styled(footer, theme::dim())));
    let area = bottom_anchored(root, input_area, lines.len());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(theme::border())
                    .padding(Padding::horizontal(1))
                    .title(Span::styled(" permission ", theme::title())),
            )
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

fn diff_permission_line(line: &crate::ui::diff::DiffLine) -> Line<'static> {
    use crate::ui::diff::DiffKind;
    let no = line
        .line_no
        .map(|n| format!("{n:>3}"))
        .unwrap_or_else(|| "   ".to_owned());
    let span = match line.kind {
        DiffKind::Del => Span::styled(format!("{no} - {}", line.text), theme::diff_del()),
        DiffKind::Add => Span::styled(format!("{no} + {}", line.text), theme::diff_add()),
        DiffKind::Ctx => Span::styled(format!("{no}   {}", line.text), theme::dim()),
    };
    Line::from(span)
}

fn draw_command_palette(frame: &mut Frame<'_>, state: &CommandPaletteState, root: Rect) {
    let area = centered(root, 72, 60);
    let visible = state.visible();
    let row_width = area.width.saturating_sub(2) as usize;
    let title_width = visible
        .iter()
        .map(|e| e.title.len())
        .max()
        .unwrap_or(8)
        .max(8);
    let items = visible
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            if let Some(header) = &entry.section_header {
                return ListItem::new(Line::from(Span::styled(
                    header.clone(),
                    theme::accent().add_modifier(Modifier::BOLD),
                )));
            }
            let title = format!("{:<width$}", entry.title, width = title_width);
            let mut spans = vec![
                Span::raw(title),
                Span::raw("  "),
                Span::styled(entry.description.clone(), theme::dim()),
            ];
            if let Some(hint) = &entry.key_hint {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(hint.clone(), theme::dim()));
            }
            let line = Line::from(spans);
            if idx == state.selected && entry.section_header.is_none() {
                ListItem::new(pad_line_to_width(line, row_width)).style(theme::selected())
            } else {
                ListItem::new(line)
            }
        })
        .collect::<Vec<_>>();
    frame.render_widget(Clear, area);
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme::border())
                .title(format!(" commands  filter: {} ", state.filter)),
        ),
        area,
    );
}

fn draw_which_key(frame: &mut Frame<'_>, app: &App, root: Rect, input_area: Rect) {
    let lines = if app.input.popup.is_some() {
        vec![
            Line::from(Span::styled("Slash / file popup", theme::title())),
            Line::from("↑↓ move · enter insert · esc dismiss · type to filter"),
        ]
    } else if matches!(app.overlay, Overlay::CommandPalette(_)) {
        vec![
            Line::from(Span::styled("Command palette", theme::title())),
            Line::from("↑↓ move · enter run · esc close · type to filter"),
        ]
    } else if app.session.turn.is_running() {
        vec![
            Line::from(Span::styled("Turn running", theme::title())),
            Line::from("esc interrupt · end scroll bottom · ctrl+p palette"),
        ]
    } else if app.overlay.is_active() {
        vec![
            Line::from(Span::styled("Overlay", theme::title())),
            Line::from("↑↓ move · enter select · esc cancel"),
            Line::from("y/n/a on permission · d diff"),
        ]
    } else {
        vec![
            Line::from(Span::styled("Chat", theme::title())),
            Line::from("enter send · esc interrupt/clear queue · end scroll bottom"),
            Line::from("ctrl+p palette · /sessions · ? which-key · ctrl+o expand tool"),
            Line::from("shift+tab cycle mode · ctrl+shift+c copy transcript"),
            Line::from(Span::styled("Pickers", theme::title())),
            Line::from("↑↓ move · enter select · esc cancel · type to filter"),
            Line::from(Span::styled("Permission", theme::title())),
            Line::from("y/n/a · 1-3 · d diff · esc deny"),
        ]
    };
    let area = bottom_anchored(root, input_area, lines.len());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme::border())
                .title(Span::styled(" keys ", theme::title())),
        ),
        area,
    );
}

fn draw_connect_wizard(frame: &mut Frame<'_>, state: &ConnectWizardState, app: &App, root: Rect) {
    let area = centered(root, 70, 50);
    match state.step {
        ConnectWizardStep::PickProvider => {
            draw_connect_gallery(frame, state, app, area);
            return;
        }
        ConnectWizardStep::PickAuthMethod => {
            draw_connect_auth_method(frame, state, area);
            return;
        }
        ConnectWizardStep::OAuthWaiting => {
            draw_connect_oauth_waiting(frame, state, app, area);
            return;
        }
        _ => {}
    }
    let (step_label, prompt) = match state.step {
        ConnectWizardStep::CustomProviderId => ("custom id", "Unique id (e.g. myllm, glm):"),
        ConnectWizardStep::BaseUrl => ("base URL", "OpenAI-compatible base URL:"),
        ConnectWizardStep::ApiKey => ("API key", "API key (leave empty for local endpoints):"),
        ConnectWizardStep::Model => ("default model", "Default model id:"),
        ConnectWizardStep::PickProvider
        | ConnectWizardStep::PickAuthMethod
        | ConnectWizardStep::OAuthWaiting => unreachable!(),
    };
    let mut lines = vec![
        Line::from(Span::styled("Connect provider", theme::title())),
        Line::from(Span::styled(step_label, theme::dim())),
        Line::default(),
        Line::from(prompt),
        Line::from(Span::styled(
            if state.input.is_empty() {
                "type value…".to_owned()
            } else {
                state.input.clone()
            },
            theme::selected(),
        )),
    ];
    if !state.id.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("id: {}", state.id),
            theme::dim(),
        )));
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "enter next · esc cancel",
        theme::dim(),
    )));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme::border())
                .title(" connect "),
        ),
        area,
    );
}

fn draw_connect_gallery(frame: &mut Frame<'_>, state: &ConnectWizardState, app: &App, area: Rect) {
    use agentloop_cli_core::{CliPrefs, provider_template, template_is_connected};
    let prefs = CliPrefs::load();
    let rows = state.gallery_rows();
    let row_width = area.width.saturating_sub(2) as usize;
    let items = rows
        .iter()
        .enumerate()
        .map(|(idx, row)| match row {
            ConnectGalleryRow::Header(label) => ListItem::new(Line::from(Span::styled(
                label.to_string(),
                theme::accent().add_modifier(Modifier::BOLD),
            ))),
            ConnectGalleryRow::Template(id) => {
                let Some(template) = provider_template(id) else {
                    return ListItem::new(Line::from(""));
                };
                let connected = template_is_connected(template, &app.providers, &prefs);
                let gutter = if connected { "✓ " } else { "  " };
                let label = format!("{gutter}{}", template.label);
                let mut spans = vec![
                    Span::raw(label),
                    Span::raw("  "),
                    Span::styled(template.description.to_owned(), theme::dim()),
                ];
                if let agentloop_cli_core::ProviderAuth::EnvOnly { env_var } = template.auth {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(format!("({env_var})"), theme::dim()));
                }
                let line = Line::from(spans);
                if idx == state.selected {
                    ListItem::new(pad_line_to_width(line, row_width)).style(theme::selected())
                } else {
                    ListItem::new(line)
                }
            }
            ConnectGalleryRow::Custom => {
                let line = Line::from(vec![
                    Span::raw("  Custom provider…"),
                    Span::raw("  "),
                    Span::styled("enter your own base URL", theme::dim()),
                ]);
                if idx == state.selected {
                    ListItem::new(pad_line_to_width(line, row_width)).style(theme::selected())
                } else {
                    ListItem::new(line)
                }
            }
        })
        .collect::<Vec<_>>();
    frame.render_widget(Clear, area);
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme::border())
                .title(Span::styled(" Connect a provider ", theme::title()))
                .title_bottom(Span::styled(" esc ", theme::dim())),
        ),
        area,
    );
    if !state.filter.is_empty() {
        let filter_line = Line::from(Span::styled(
            format!("filter: {}", state.filter),
            theme::dim(),
        ));
        let filter_area = Rect {
            x: area.x + 2,
            y: area.y + 1,
            width: area.width.saturating_sub(4),
            height: 1,
        };
        frame.render_widget(Paragraph::new(filter_line), filter_area);
    }
}

fn draw_connect_auth_method(frame: &mut Frame<'_>, state: &ConnectWizardState, area: Rect) {
    let items = state
        .auth_methods
        .iter()
        .enumerate()
        .map(|(idx, spec)| {
            let marker = if idx == state.selected { "› " } else { "  " };
            let line = Line::from(format!("{marker}{}", spec.label));
            if idx == state.selected {
                ListItem::new(line).style(theme::selected())
            } else {
                ListItem::new(line)
            }
        })
        .collect::<Vec<_>>();
    frame.render_widget(Clear, area);
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme::border())
                .title(Span::styled(
                    state
                        .template_id
                        .as_deref()
                        .and_then(agentloop_cli_core::provider_template)
                        .map(|t| t.label)
                        .unwrap_or("Connect"),
                    theme::title(),
                ))
                .title_bottom(Span::styled(" esc ", theme::dim())),
        ),
        area,
    );
}

fn draw_connect_oauth_waiting(
    frame: &mut Frame<'_>,
    state: &ConnectWizardState,
    app: &App,
    area: Rect,
) {
    let title = state
        .auth_method_label
        .as_deref()
        .unwrap_or("ChatGPT sign-in");
    let url = state.oauth_url.as_deref().unwrap_or("…");
    let instructions = state
        .oauth_instructions
        .as_deref()
        .unwrap_or("Waiting to start…");
    let pulse = if state.oauth_waiting {
        theme::spinner_frame(app.status.spinner)
    } else {
        "…"
    };
    let lines = vec![
        Line::from(Span::styled(title.to_owned(), theme::title())),
        Line::default(),
        Line::from(Span::styled(
            truncate_cells(url, area.width.saturating_sub(4) as usize),
            theme::accent(),
        )),
        Line::from(Span::styled(instructions.to_owned(), theme::dim())),
        Line::default(),
        Line::from(vec![
            Span::styled(format!("{pulse} "), theme::accent()),
            Span::styled("Waiting for authorization…", theme::dim()),
        ]),
        Line::default(),
        Line::from(Span::styled("c copy url · esc cancel", theme::dim())),
    ];
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme::border())
                .title(Span::styled(" connect ", theme::title())),
        ),
        area,
    );
}

fn draw_question(frame: &mut Frame<'_>, prompt: &QuestionPrompt, root: Rect) {
    let area = centered(root, 70, 60);
    let Some(question) = prompt.questions.get(prompt.current) else {
        return;
    };
    let multi = question.multi_select;
    let mut header_spans = Vec::new();
    if let Some(role) = &prompt.role {
        header_spans.push(Span::styled(format!("[{role}] "), theme::warn()));
    }
    header_spans.push(Span::styled(question.header.clone(), theme::title()));
    let mut lines = vec![
        Line::from(header_spans),
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
            theme::selected()
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{number}. {marker} {}", option.label), style),
            option
                .description
                .as_ref()
                .map(|description| Span::styled(format!(" — {description}"), theme::dim()))
                .unwrap_or_else(|| Span::raw("")),
        ]));
    }
    if question.allow_custom {
        lines.push(Line::default());
        let custom_style = if prompt.custom_mode {
            theme::selected()
        } else {
            theme::dim()
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
        theme::dim(),
    )));
    if prompt.questions.len() > 1 {
        lines.push(Line::from(Span::styled(
            format!(
                "Question {} of {}",
                prompt.current + 1,
                prompt.questions.len()
            ),
            theme::dim(),
        )));
    }
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(theme::border())
                    .title(" question "),
            )
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
            Line::from(Span::styled(verification_uri.clone(), theme::title())),
            Line::from(Span::styled(
                user_code.clone(),
                theme::title().add_modifier(Modifier::BOLD),
            )),
            Line::default(),
            Line::from(Span::styled(
                format!(
                    "Waiting {}s / expires in {expires_in}s. Press o to open, Esc to cancel.",
                    since.elapsed().as_secs()
                ),
                theme::dim(),
            )),
        ],
        LoginState::Polling { since } => vec![Line::from(format!(
            "Waiting for GitHub confirmation... {}s",
            since.elapsed().as_secs()
        ))],
        LoginState::Verifying => vec![Line::from("Verifying Copilot access...")],
        LoginState::Failed { message } => vec![
            Line::from(Span::styled("Login failed", theme::error())),
            Line::default(),
            Line::from(message.clone()),
            Line::default(),
            Line::from(Span::styled(
                "Esc or Enter closes this dialog.",
                theme::dim(),
            )),
        ],
    };
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" GitHub Copilot login "),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_shell_command(frame: &mut Frame<'_>, state: &ShellCommandOverlay, app: &App, root: Rect) {
    let area = centered(root, 80, 70);
    let mut lines = vec![Line::from(vec![
        Span::styled("$ ", theme::user()),
        Span::styled(state.command.clone(), theme::assistant()),
    ])];
    lines.push(Line::default());

    match &state.phase {
        ShellCommandPhase::Running { since } => {
            let spinner = theme::spinner_frame(app.status.spinner);
            lines.push(Line::from(vec![
                Span::styled(format!("{spinner} "), theme::warn()),
                Span::styled(
                    format!("running… {}s", since.elapsed().as_secs()),
                    theme::warn(),
                ),
            ]));
            lines.push(Line::default());
            lines.push(Line::from(Span::styled("Esc cancels", theme::dim())));
        }
        ShellCommandPhase::Done { output, exit_code } => {
            if let Some(code) = exit_code.filter(|code| *code != 0) {
                lines.push(Line::from(Span::styled(
                    format!("exit code {code}"),
                    theme::error(),
                )));
                lines.push(Line::default());
            }
            if output.is_empty() {
                lines.push(Line::from(Span::styled("(no output)", theme::dim())));
            } else {
                for line in terminal_lines(output) {
                    lines.push(Line::from(line));
                }
            }
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                "↑/↓ scroll · Esc close",
                theme::dim(),
            )));
        }
        ShellCommandPhase::Failed { message } => {
            lines.push(Line::from(Span::styled(message.clone(), theme::error())));
            lines.push(Line::default());
            lines.push(Line::from(Span::styled("Esc close", theme::dim())));
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
                    .border_type(BorderType::Rounded)
                    .border_style(theme::border())
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
        Line::from(Span::styled("Keys", theme::title())),
        Line::from("Enter submit · Alt+Enter/Ctrl+J newline · Esc cancel · Ctrl+C quit"),
        Line::from("@ attach files · @path:[0:12] python slice preview · type @ to search"),
        Line::from(
            "Wheel / PgUp·PgDn / ↑↓ (empty prompt) scroll the transcript · End follows the tail",
        ),
        Line::from("Select text: drag with ⌥ (iTerm2) · Fn (Terminal.app) · Shift (Linux)"),
        Line::from(
            "Ctrl+Shift+C or /copy — copy transcript · Ctrl+M — wheel-scroll ⇄ drag-select (saved)",
        ),
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
        Line::from(Span::styled("Modes", theme::title())),
        Line::from("require — ask before mutating tools · accept edits — file edits auto-allowed"),
        Line::from(
            "plan — read-only research · allow-all — /permissions allow-all (never in the cycle)",
        ),
        Line::default(),
        Line::from(Span::styled("Backends vs providers", theme::title())),
        Line::from(
            "/provider switches the LLM API inside the native loop (incl. /connect custom hosts);",
        ),
        Line::from("/agent swaps the whole backend (native loop vs external claude/copilot CLIs)."),
        Line::default(),
        Line::from(Span::styled("Commands", theme::title())),
    ];
    for entry in app.commands.entries() {
        let hint = entry
            .args_hint
            .as_ref()
            .map(|hint| format!(" {hint}"))
            .unwrap_or_default();
        lines.push(Line::from(vec![
            Span::styled(format!("/{}{}", entry.name, hint), theme::assistant()),
            Span::raw(" — "),
            Span::raw(entry.description.clone()),
            Span::styled(format!(" [{}]", entry.source), theme::dim()),
        ]));
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "CLI commands win name collisions; engine commands are sent through to the loop.",
        theme::dim(),
    )));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(theme::border())
                    .title(" help "),
            )
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
                theme::selected()
            } else {
                Style::default()
            };
            let status = if item.enabled { "on" } else { "off" };
            ListItem::new(Line::from(vec![
                Span::styled(format!("[{status}] {}", item.name), style),
                Span::raw(" "),
                Span::styled(item.source.clone(), theme::dim()),
            ]))
        })
        .collect::<Vec<_>>();
    frame.render_widget(Clear, area);
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
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
            vec![Line::from(Span::styled(message, theme::error()))]
        }
        McpExplorerPhase::Result { output, .. } => terminal_lines(output)
            .into_iter()
            .map(Line::from)
            .collect::<Vec<_>>(),
        McpExplorerPhase::Tools { tools } => {
            if state.args_mode {
                vec![
                    Line::from(Span::styled("JSON arguments:", theme::title())),
                    Line::from(state.args_input.clone()),
                    Line::default(),
                    Line::from(Span::styled("Enter call · Esc back", theme::dim())),
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
                            theme::selected()
                        } else {
                            Style::default()
                        };
                        Line::from(vec![
                            Span::styled(tool.name.clone(), style),
                            Span::raw(" — "),
                            Span::styled(tool.description.clone(), theme::dim()),
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
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(theme::border())
                    .title(title),
            )
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
            theme::title(),
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
                    theme::selected()
                } else {
                    Style::default()
                };
                lines.push(Line::from(vec![
                    Span::styled(entry.label.clone(), style),
                    Span::raw(" "),
                    Span::styled(entry.npm.clone(), theme::dim()),
                ]));
                lines.push(Line::from(Span::styled(
                    entry.description.clone(),
                    theme::dim(),
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
                    theme::dim(),
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
                    .border_type(BorderType::Rounded)
                    .title(" mcp-install "),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_confirm(frame: &mut Frame<'_>, prompt: &ConfirmPrompt, root: Rect, input_area: Rect) {
    let lines = vec![
        Line::from(Span::styled(prompt.title.clone(), theme::title())),
        Line::default(),
        Line::from(prompt.message.clone()),
        Line::default(),
        Line::from(Span::styled("Enter/y confirm · Esc cancel", theme::dim())),
    ];
    let area = bottom_anchored(root, input_area, lines.len());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(theme::border())
                    .padding(Padding::horizontal(1))
                    .title(Span::styled(" confirm ", theme::title())),
            )
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
        assert_eq!(viewport, 20);
        assert_eq!(max_offset, 0);
    }

    #[test]
    fn viewport_metrics_short_content_fits() {
        let area = Rect::new(0, 0, 80, 10);
        let lines = vec![Line::from("hello"), Line::from("world")];
        let (total, viewport, max_offset) = chat_viewport_metrics(&lines, area);
        assert_eq!(total, 2);
        assert_eq!(viewport, 10);
        assert_eq!(max_offset, 0);
    }

    #[test]
    fn viewport_metrics_long_content_scrolls() {
        let area = Rect::new(0, 0, 40, 8);
        let lines: Vec<_> = (0..20).map(|i| Line::from(format!("row {i}"))).collect();
        let (total, viewport, max_offset) = chat_viewport_metrics(&lines, area);
        assert_eq!(total, 20);
        assert_eq!(viewport, 8);
        assert_eq!(max_offset, 12);
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
        use ratatui::widgets::{Paragraph, Wrap};

        let marker = "ZZZZ_LAST_LINE";
        let mut lines: Vec<_> = (0..30)
            .map(|i| Line::from(format!("filler line {i}")))
            .collect();
        lines.push(Line::from(marker));

        let area = Rect::new(0, 0, 60, 12);
        let (_, _, max_offset) = chat_viewport_metrics(&lines, area);

        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| {
                frame.render_widget(
                    Paragraph::new(lines)
                        .wrap(Wrap { trim: false })
                        .scroll((max_offset as u16, 0)),
                    area,
                );
            })
            .expect("draw");

        let buf = terminal.backend().buffer();
        let mut visible = String::new();
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
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
