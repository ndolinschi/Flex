//! Chat state: the ordered item list and the delta-reconciliation rules.
//!
//! [`ChatState::apply`] maps every content-bearing [`AgentEvent`] onto the
//! item list. The core rule is *authoritative materialization*: streamed
//! deltas accumulate into a draft, and the persisted `AssistantMessage`
//! (or each `TextSnapshot`) **replaces** the accumulated state, so delta
//! loss or duplication self-heals.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use agentloop_contracts::{
    AgentEvent, ContentBlock, MessageId, PlanEntry, Role, SessionId, TokenUsage, ToolCall,
    ToolCallId, ToolCallStatus, Transcript, TranscriptBlock, TurnStopReason,
};

/// One block of an assistant item (draft or complete).
#[derive(Debug, Clone, PartialEq)]
pub enum DraftBlock {
    /// Markdown body text.
    Markdown(String),
    /// Reasoning text; collapsed to one line once the message completes.
    Thinking {
        text: String,
        collapsed: bool,
        /// When the first delta arrived (drives the real duration).
        started: Option<Instant>,
        /// Total thinking time, fixed once the block completes.
        duration_ms: Option<u64>,
    },
}

/// Displayed thinking duration in whole seconds: real when measured, a
/// rough line-count estimate otherwise (resumed transcripts).
pub(crate) fn thinking_seconds(duration_ms: Option<u64>, text: &str) -> u64 {
    match duration_ms {
        Some(ms) => ms / 1000,
        None => {
            let lines = text.lines().count().max(1);
            (lines as u64).saturating_add(1)
        }
    }
}

/// One rendered row group in the chat viewport.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatItem {
    /// The user's prompt, echoed by the engine's `UserMessage`.
    User { text: String },
    /// An assistant message: streaming draft or materialized final.
    Assistant {
        blocks: Vec<DraftBlock>,
        model: Option<String>,
        usage: Option<TokenUsage>,
        complete: bool,
        /// Bumped on every mutation; render caches key on it.
        rev: u64,
    },
    /// One tool invocation, upserted across its lifecycle.
    Tool {
        call: Box<ToolCall>,
        /// Ephemeral progress note shown while `Running`.
        progress: Option<String>,
        /// When true, show the full tool result instead of a truncated preview.
        expanded: bool,
        rev: u64,
    },
    /// The agent's working plan (latest state).
    Plan { entries: Vec<PlanEntry>, rev: u64 },
    /// CLI-local informational line.
    Info { text: String },
    /// The welcome splash shown at the top of a fresh transcript.
    Splash {
        name: String,
        version: String,
        cwd: String,
    },
    /// An error surfaced by the engine or the CLI.
    Error {
        headline: String,
        detail: Option<String>,
    },
    /// A subagent row, nested under the Task tool row that spawned it and
    /// updated live from relayed child events.
    Subagent {
        child: SessionId,
        task: String,
        role: Option<String>,
        /// Learned from the first relayed `AssistantMessage`/`ModelFallback`
        /// (`SubagentStarted` carries no model on the wire).
        model: Option<String>,
        tool_count: usize,
        tokens: u64,
        /// Latest relayed tool call, as `Name(args-summary)`.
        last_activity: Option<String>,
        /// Live-elapsed anchor; only meaningful while running.
        started: Option<Instant>,
        /// From `SubagentCompleted.summary` — authoritative once done.
        duration_ms: Option<u64>,
        outcome: SubagentOutcome,
        /// Distinct child tool-call ids seen via relay (drives `tool_count`).
        seen_calls: HashSet<ToolCallId>,
    },
}

/// Terminal state of a subagent row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentOutcome {
    Running,
    Done,
    Failed,
    Cancelled,
}

/// Viewport scroll position.
#[derive(Debug, Clone, Copy)]
pub struct ScrollState {
    /// Wrapped lines between the bottom of content and the viewport bottom.
    pub offset_from_bottom: usize,
    /// When true the view pins to the newest content.
    pub follow: bool,
}

impl Default for ScrollState {
    fn default() -> Self {
        Self {
            offset_from_bottom: 0,
            follow: true,
        }
    }
}

impl ScrollState {
    /// Scroll up by `step` wrapped lines; leaves follow mode.
    pub fn page_up(&mut self, step: usize) {
        self.follow = false;
        self.offset_from_bottom += step;
    }

    /// Scroll down by `step` wrapped lines; re-enables follow at the bottom.
    pub fn page_down(&mut self, step: usize) {
        self.offset_from_bottom = self.offset_from_bottom.saturating_sub(step);
        if self.offset_from_bottom == 0 {
            self.follow = true;
        }
    }

    /// Pin the viewport to the newest content.
    pub fn scroll_to_bottom(&mut self) {
        self.offset_from_bottom = 0;
        self.follow = true;
    }

    /// Clamp `offset_from_bottom` to the scrollable range.
    pub fn clamp_offset(&mut self, max_offset: usize) {
        if self.offset_from_bottom > max_offset {
            self.offset_from_bottom = max_offset;
        }
    }
}

#[cfg(test)]
mod scroll_tests {
    use super::ScrollState;

    #[test]
    fn page_down_re_enables_follow_at_bottom() {
        let mut scroll = ScrollState::default();
        scroll.page_up(5);
        assert!(!scroll.follow);
        scroll.page_down(5);
        assert!(scroll.follow);
        assert_eq!(scroll.offset_from_bottom, 0);
    }

    #[test]
    fn clamp_offset_caps_scrollback() {
        let mut scroll = ScrollState::default();
        scroll.page_up(100);
        scroll.clamp_offset(10);
        assert_eq!(scroll.offset_from_bottom, 10);
    }

    #[test]
    fn scroll_to_bottom_resets_follow() {
        let mut scroll = ScrollState::default();
        scroll.page_up(5);
        scroll.scroll_to_bottom();
        assert!(scroll.follow);
        assert_eq!(scroll.offset_from_bottom, 0);
    }
}

/// The chat transcript as displayed, plus the indices that make streamed
/// upserts O(1).
#[derive(Default)]
pub struct ChatState {
    /// Items in display order.
    pub items: Vec<ChatItem>,
    /// Open (draft) assistant messages by id.
    open: HashMap<MessageId, usize>,
    /// Tool items by call id.
    tools: HashMap<ToolCallId, usize>,
    /// Subagent items by child session id.
    subagents: HashMap<SessionId, usize>,
    /// The single trailing plan item, if any.
    plan: Option<usize>,
    /// Scroll position of the viewport.
    pub scroll: ScrollState,
    /// Index into [`Self::items`] for keyboard expand/collapse focus.
    pub focused_tool: Option<usize>,
    rev: u64,
}

impl ChatState {
    fn next_rev(&mut self) -> u64 {
        self.rev += 1;
        self.rev
    }

    /// Insert at `at`, shifting every stored index >= `at` so the O(1)
    /// upsert maps stay correct.
    fn insert_item_at(&mut self, at: usize, item: ChatItem) {
        self.items.insert(at, item);
        for idx in self.open.values_mut() {
            if *idx >= at {
                *idx += 1;
            }
        }
        for idx in self.tools.values_mut() {
            if *idx >= at {
                *idx += 1;
            }
        }
        for idx in self.subagents.values_mut() {
            if *idx >= at {
                *idx += 1;
            }
        }
        if let Some(plan) = &mut self.plan {
            if *plan >= at {
                *plan += 1;
            }
        }
        if let Some(focused) = &mut self.focused_tool {
            if *focused >= at {
                *focused += 1;
            }
        }
    }

    /// Role of a live subagent row (used by the app reducer to badge
    /// relayed permission prompts).
    pub fn subagent_role(&self, child: &SessionId) -> Option<String> {
        self.subagents
            .get(child)
            .and_then(|idx| self.items.get(*idx))
            .and_then(|item| match item {
                ChatItem::Subagent { role, .. } => role.clone(),
                _ => None,
            })
    }

    /// Project one relayed child event onto its subagent row. Relays are
    /// ephemeral (never persisted in the parent), so this only decorates a
    /// live row: after a Gap/resync only Started/Completed replay, and a
    /// completed row renders fully from its TurnSummary fields.
    fn project_subagent_event(&mut self, idx: usize, event: &AgentEvent) {
        if let AgentEvent::ModelFallback {
            from,
            to: Some(to),
            reason,
        } = event
        {
            let badge = match self.items.get_mut(idx) {
                Some(ChatItem::Subagent { model, role, .. }) => {
                    *model = Some(to.0.clone());
                    role.clone().unwrap_or_else(|| "subagent".to_owned())
                }
                _ => return,
            };
            self.push_info(format!(
                "\u{21bb} [{badge}] model fallback: {from} \u{2192} {to} ({})",
                reason.message
            ));
            return;
        }
        let Some(ChatItem::Subagent {
            model,
            tool_count,
            tokens,
            last_activity,
            seen_calls,
            ..
        }) = self.items.get_mut(idx)
        else {
            return;
        };
        match event {
            AgentEvent::ToolCallUpdated { call } => {
                if seen_calls.insert(call.id.clone()) {
                    *tool_count += 1;
                }
                *last_activity = Some(crate::tool_output::tool_summary(
                    &call.tool_name,
                    &call.input,
                ));
            }
            AgentEvent::AssistantMessage {
                model: message_model,
                usage,
                ..
            } => {
                if message_model.is_some() {
                    *model = message_model.clone();
                }
                *tokens += usage.map(|usage| usage.output).unwrap_or(0);
            }
            AgentEvent::PermissionRequested { title, .. } => {
                *last_activity = Some(format!("waiting: {title}"));
            }
            _ => {}
        }
    }

    /// Append a CLI-local info line. Skipped when it would repeat the
    /// immediately preceding info line (no `already on native` stutter).
    pub fn push_info(&mut self, text: impl Into<String>) {
        let text = text.into();
        if matches!(self.items.last(), Some(ChatItem::Info { text: last }) if *last == text) {
            return;
        }
        self.items.push(ChatItem::Info { text });
    }

    /// Append the welcome splash (logo, version, cwd, hints).
    pub fn push_splash(&mut self, name: String, version: String, cwd: String) {
        self.items.push(ChatItem::Splash { name, version, cwd });
    }

    /// True when only splash/info lines are present (empty session home).
    pub fn is_home_screen(&self) -> bool {
        self.items
            .iter()
            .all(|item| matches!(item, ChatItem::Splash { .. } | ChatItem::Info { .. }))
    }

    /// Append an error line (headline only).
    pub fn push_error(&mut self, message: impl Into<String>) {
        self.push_human_error(message.into(), None);
    }

    /// Append a humanized engine error with optional dim detail.
    pub fn push_human_error(&mut self, headline: String, detail: Option<String>) {
        self.items.push(ChatItem::Error { headline, detail });
    }

    /// Plain-text export of the visible transcript (for clipboard copy).
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for item in &self.items {
            match item {
                ChatItem::User { text } => {
                    out.push_str("> ");
                    out.push_str(text);
                    out.push('\n');
                }
                ChatItem::Assistant { blocks, model, .. } => {
                    out.push_str("assistant");
                    if let Some(model) = model {
                        out.push_str(" [");
                        out.push_str(model);
                        out.push(']');
                    }
                    out.push_str(":\n");
                    for block in blocks {
                        match block {
                            DraftBlock::Markdown(text) => {
                                out.push_str(text);
                                if !text.ends_with('\n') {
                                    out.push('\n');
                                }
                            }
                            DraftBlock::Thinking {
                                text,
                                collapsed,
                                duration_ms,
                                ..
                            } => {
                                if *collapsed {
                                    out.push_str(&format!(
                                        "[thought for {}s]\n",
                                        thinking_seconds(*duration_ms, text)
                                    ));
                                } else {
                                    out.push_str("[thinking]\n");
                                    out.push_str(text);
                                    if !text.ends_with('\n') {
                                        out.push('\n');
                                    }
                                }
                            }
                        }
                    }
                }
                ChatItem::Tool { call, progress, .. } => {
                    out.push_str(&crate::tool_output::tool_summary(
                        &call.tool_name,
                        &call.input,
                    ));
                    out.push_str(&format!(" [{:?}]", call.status));
                    out.push('\n');
                    if let Some(progress) = progress {
                        out.push_str("  ");
                        out.push_str(progress);
                        out.push('\n');
                    }
                    if let Some(result) = &call.result {
                        out.push_str(&result.render_text());
                        if !out.ends_with('\n') {
                            out.push('\n');
                        }
                    }
                }
                ChatItem::Plan { entries, .. } => {
                    out.push_str("plan:\n");
                    for entry in entries {
                        out.push_str("  ");
                        out.push_str(&format!("{:?}: {}", entry.status, entry.content));
                        out.push('\n');
                    }
                }
                ChatItem::Info { text } => {
                    out.push_str(text);
                    out.push('\n');
                }
                ChatItem::Splash { .. } => {}
                ChatItem::Error { headline, detail } => {
                    out.push_str("error: ");
                    out.push_str(headline);
                    if let Some(detail) = detail {
                        out.push_str(" (");
                        out.push_str(detail);
                        out.push(')');
                    }
                    out.push('\n');
                }
                ChatItem::Subagent {
                    task,
                    role,
                    outcome,
                    tool_count,
                    ..
                } => {
                    let marker = match outcome {
                        SubagentOutcome::Running => "running",
                        SubagentOutcome::Done => "done",
                        SubagentOutcome::Failed => "failed",
                        SubagentOutcome::Cancelled => "cancelled",
                    };
                    let badge = role.as_deref().unwrap_or("subagent");
                    out.push_str(&format!(
                        "subagent [{badge}] {marker}: {task} \u{b7} {tool_count} tools\n"
                    ));
                }
            }
            out.push('\n');
        }
        out
    }

    /// Apply one engine event to the item list. Control-plane events
    /// (permissions, questions, turn lifecycle, gaps) are handled by the
    /// app reducer, not here.
    pub fn apply(&mut self, event: &AgentEvent) {
        match event {
            AgentEvent::MessageStarted { message_id, role } => {
                if *role == Role::Assistant && !self.open.contains_key(message_id) {
                    self.start_draft(message_id.clone());
                }
            }
            AgentEvent::MarkdownDelta { message_id, text } => {
                let rev = self.next_rev();
                let idx = self.draft_index(message_id);
                if let Some(ChatItem::Assistant { blocks, rev: r, .. }) = self.items.get_mut(idx) {
                    match blocks.last_mut() {
                        Some(DraftBlock::Markdown(body)) => body.push_str(text),
                        _ => blocks.push(DraftBlock::Markdown(text.clone())),
                    }
                    *r = rev;
                }
            }
            AgentEvent::ThinkingDelta { message_id, text } => {
                let rev = self.next_rev();
                let idx = self.draft_index(message_id);
                if let Some(ChatItem::Assistant { blocks, rev: r, .. }) = self.items.get_mut(idx) {
                    match blocks.last_mut() {
                        Some(DraftBlock::Thinking { text: body, .. }) => body.push_str(text),
                        _ => blocks.push(DraftBlock::Thinking {
                            text: text.clone(),
                            collapsed: false,
                            started: Some(Instant::now()),
                            duration_ms: None,
                        }),
                    }
                    *r = rev;
                }
            }
            AgentEvent::TextSnapshot { message_id, text } => {
                let rev = self.next_rev();
                let idx = self.draft_index(message_id);
                if let Some(ChatItem::Assistant { blocks, rev: r, .. }) = self.items.get_mut(idx) {
                    *blocks = vec![DraftBlock::Markdown(text.clone())];
                    *r = rev;
                }
            }
            AgentEvent::AssistantMessage {
                message_id,
                content,
                model,
                usage,
            } => {
                let mut final_blocks = materialize_blocks(content);
                let rev = self.next_rev();
                match self.open.remove(message_id) {
                    Some(idx) => {
                        if let Some(ChatItem::Assistant {
                            blocks,
                            model: m,
                            usage: u,
                            complete,
                            rev: r,
                        }) = self.items.get_mut(idx)
                        {
                            carry_thinking_durations(blocks, &mut final_blocks);
                            *blocks = final_blocks;
                            *m = model.clone();
                            *u = *usage;
                            *complete = true;
                            *r = rev;
                        }
                    }
                    None => {
                        // MessageLevel agents: no draft ever existed. A
                        // message of only tool-use blocks renders nothing.
                        if !final_blocks.is_empty() {
                            self.items.push(ChatItem::Assistant {
                                blocks: final_blocks,
                                model: model.clone(),
                                usage: *usage,
                                complete: true,
                                rev,
                            });
                        }
                    }
                }
            }
            AgentEvent::UserMessage { content, .. } => {
                // Tool results are carried by ToolCall records; a message of
                // only results joins to empty text and is skipped.
                let text = joined_markdown(content);
                if !text.is_empty() {
                    self.items.push(ChatItem::User { text });
                }
            }
            AgentEvent::ToolCallUpdated { call } => {
                let rev = self.next_rev();
                match self.tools.get(&call.id).copied() {
                    Some(idx) => {
                        if let Some(ChatItem::Tool {
                            call: c,
                            progress,
                            expanded,
                            rev: r,
                        }) = self.items.get_mut(idx)
                        {
                            **c = call.clone();
                            if c.status.is_terminal() {
                                *progress = None;
                            }
                            *r = rev;
                            let _ = expanded;
                        }
                    }
                    None => {
                        self.tools.insert(call.id.clone(), self.items.len());
                        self.items.push(ChatItem::Tool {
                            call: Box::new(call.clone()),
                            progress: None,
                            expanded: false,
                            rev,
                        });
                    }
                }
            }
            AgentEvent::ToolProgress { call_id, note } => {
                let rev = self.next_rev();
                if let Some(idx) = self.tools.get(call_id).copied() {
                    if let Some(ChatItem::Tool {
                        call,
                        progress,
                        rev: r,
                        ..
                    }) = self.items.get_mut(idx)
                    {
                        if call.status == ToolCallStatus::Running {
                            *progress = Some(crate::terminal_text::normalize_terminal_text(note));
                            *r = rev;
                        }
                    }
                }
            }
            AgentEvent::PlanUpdated { entries } => {
                let rev = self.next_rev();
                match self.plan {
                    Some(idx) => {
                        if let Some(ChatItem::Plan {
                            entries: e, rev: r, ..
                        }) = self.items.get_mut(idx)
                        {
                            *e = entries.clone();
                            *r = rev;
                        }
                    }
                    None => {
                        self.plan = Some(self.items.len());
                        self.items.push(ChatItem::Plan {
                            entries: entries.clone(),
                            rev,
                        });
                    }
                }
            }
            AgentEvent::SessionError { error } => {
                if let Some(human) = crate::error_fmt::humanize_engine_error(error) {
                    self.push_human_error(human.headline, human.detail);
                }
            }
            AgentEvent::CommandExpanded { name, args } => {
                let text = if args.is_empty() {
                    format!("/{name}")
                } else {
                    format!("/{name} {args}")
                };
                self.push_info(text);
            }
            AgentEvent::ModelFallback { from, to, reason } => match to {
                Some(to) => self.push_info(format!(
                    "\u{21bb} model fallback: {from} \u{2192} {to} ({})",
                    reason.message
                )),
                None => self.push_human_error(
                    format!("all fallback models exhausted after {from}"),
                    Some(reason.message.clone()),
                ),
            },
            AgentEvent::CompactionBoundary { summary } => {
                let savings = match (summary.tokens_before, summary.tokens_after) {
                    (Some(before), Some(after)) if before > after => {
                        format!(" (~{before} → ~{after} tokens)")
                    }
                    _ => String::new(),
                };
                let is_auto = summary.strategy.starts_with("auto_");
                let message = if is_auto {
                    format!("Auto-compacted context{savings} — approaching limit")
                } else {
                    format!("Conversation compacted{savings} — earlier messages summarized")
                };
                self.push_info(message);
            }
            AgentEvent::SubagentStarted {
                child_session,
                task,
                call_id,
                role,
            } => {
                let item = ChatItem::Subagent {
                    child: child_session.clone(),
                    task: task.clone(),
                    role: role.clone(),
                    model: None,
                    tool_count: 0,
                    tokens: 0,
                    last_activity: None,
                    started: Some(Instant::now()),
                    duration_ms: None,
                    outcome: SubagentOutcome::Running,
                    seen_calls: HashSet::new(),
                };
                // Nest under the spawning Task tool row when we can find it;
                // older logs without call_id append at the end.
                let at = call_id
                    .as_ref()
                    .and_then(|id| self.tools.get(id).copied())
                    .map(|tool_idx| tool_idx + 1)
                    .unwrap_or(self.items.len());
                self.insert_item_at(at, item);
                self.subagents.insert(child_session.clone(), at);
            }
            AgentEvent::SubagentEvent {
                child_session,
                event,
            } => {
                if let Some(idx) = self.subagents.get(child_session).copied() {
                    self.project_subagent_event(idx, event);
                }
            }
            AgentEvent::SubagentCompleted {
                child_session,
                summary,
            } => {
                if let Some(ChatItem::Subagent {
                    outcome,
                    duration_ms,
                    tool_count,
                    tokens,
                    last_activity,
                    ..
                }) = self
                    .subagents
                    .get(child_session)
                    .copied()
                    .and_then(|idx| self.items.get_mut(idx))
                {
                    *outcome = match summary.stop_reason {
                        TurnStopReason::Cancelled => SubagentOutcome::Cancelled,
                        TurnStopReason::Error | TurnStopReason::MaxIterations => {
                            SubagentOutcome::Failed
                        }
                        _ => SubagentOutcome::Done,
                    };
                    // The summary is authoritative — relayed projections are
                    // best-effort and absent entirely after a resync.
                    *duration_ms = Some(summary.duration_ms);
                    *tool_count = summary.num_tool_calls as usize;
                    *tokens = summary.usage.output;
                    *last_activity = None;
                }
            }
            // Turn lifecycle, permissions, questions, gaps: app reducer.
            // ToolArgsDelta, hooks: not rendered.
            _ => {}
        }
    }

    /// Mark all open drafts complete and collapse their thinking blocks.
    /// Called at turn end so a cancelled stream never leaves a live draft.
    pub fn finalize_drafts(&mut self) {
        let rev = self.next_rev();
        for idx in self.open.values().copied().collect::<Vec<_>>() {
            if let Some(ChatItem::Assistant {
                blocks,
                complete,
                rev: r,
                ..
            }) = self.items.get_mut(idx)
            {
                *complete = true;
                for block in blocks.iter_mut() {
                    if let DraftBlock::Thinking {
                        collapsed,
                        started,
                        duration_ms,
                        ..
                    } = block
                    {
                        *collapsed = true;
                        if duration_ms.is_none() {
                            *duration_ms = started.map(|at| at.elapsed().as_millis() as u64);
                        }
                    }
                }
                *r = rev;
            }
        }
        self.open.clear();
    }

    /// Toggle the most recent thinking block's collapsed state (Ctrl+T).
    pub fn toggle_last_thinking(&mut self) -> bool {
        let rev = self.next_rev();
        for item in self.items.iter_mut().rev() {
            if let ChatItem::Assistant { blocks, rev: r, .. } = item {
                for block in blocks.iter_mut().rev() {
                    if let DraftBlock::Thinking { collapsed, .. } = block {
                        *collapsed = !*collapsed;
                        *r = rev;
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Indices of tool items in display order.
    pub fn tool_item_indices(&self) -> Vec<usize> {
        self.items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| matches!(item, ChatItem::Tool { .. }).then_some(idx))
            .collect()
    }

    /// Move keyboard focus to the next or previous expandable tool row.
    pub fn cycle_tool_focus(&mut self, backward: bool) -> bool {
        let expandable: Vec<usize> = self
            .tool_item_indices()
            .into_iter()
            .filter(|idx| self.tool_can_expand(*idx))
            .collect();
        if expandable.is_empty() {
            self.focused_tool = None;
            return false;
        }
        let next = match self.focused_tool {
            None => {
                if backward {
                    expandable[expandable.len() - 1]
                } else {
                    expandable[0]
                }
            }
            Some(current) => {
                let pos = expandable
                    .iter()
                    .position(|idx| *idx == current)
                    .unwrap_or(if backward { expandable.len() } else { 0 });
                if backward {
                    let prev = pos.checked_sub(1).unwrap_or(expandable.len() - 1);
                    expandable[prev]
                } else {
                    expandable[(pos + 1) % expandable.len()]
                }
            }
        };
        self.focused_tool = Some(next);
        true
    }

    /// Toggle expand/collapse on the focused tool, or the last expandable tool.
    pub fn toggle_focused_tool_expand(&mut self) -> bool {
        let idx = match self.focused_tool {
            Some(idx) if self.tool_can_expand(idx) => idx,
            _ => match self.last_expandable_tool_index() {
                Some(idx) => idx,
                None => return false,
            },
        };
        self.focused_tool = Some(idx);
        self.toggle_tool_expand(idx)
    }

    /// Toggle expand/collapse on one tool item.
    pub fn toggle_tool_expand(&mut self, idx: usize) -> bool {
        let expandable = match self.items.get(idx) {
            Some(ChatItem::Tool { call, .. }) => crate::tool_output::call_is_expandable(call),
            _ => false,
        };
        if !expandable {
            return false;
        }
        let next_rev = self.next_rev();
        let Some(ChatItem::Tool { expanded, rev, .. }) = self.items.get_mut(idx) else {
            return false;
        };
        *expanded = !*expanded;
        *rev = next_rev;
        true
    }

    fn tool_can_expand(&self, idx: usize) -> bool {
        let ChatItem::Tool { call, .. } = &self.items[idx] else {
            return false;
        };
        crate::tool_output::call_is_expandable(call)
    }

    fn last_expandable_tool_index(&self) -> Option<usize> {
        self.tool_item_indices()
            .into_iter()
            .rev()
            .find(|idx| self.tool_can_expand(*idx))
    }

    /// Whether any assistant draft is currently streaming.
    pub fn has_open_draft(&self) -> bool {
        !self.open.is_empty()
    }

    /// Replace all items with the materialized transcript (Gap re-sync,
    /// resumed sessions). Drafts are dropped; the transcript is ground truth.
    pub fn rebuild_from_transcript(&mut self, transcript: &Transcript) {
        self.items.clear();
        self.open.clear();
        self.tools.clear();
        // Subagent rows are not reconstructed from the reduced transcript
        // (relayed child events are ephemeral by design); clear the index
        // map so no stale entry points into the rebuilt item list.
        self.subagents.clear();
        self.plan = None;
        self.focused_tool = None;
        for item in &transcript.items {
            match item.role {
                Role::User => {
                    let text = joined_transcript_markdown(&item.blocks);
                    if !text.is_empty() {
                        self.items.push(ChatItem::User { text });
                    }
                }
                Role::Assistant => {
                    let mut blocks = Vec::new();
                    let mut calls: Vec<&ToolCall> = Vec::new();
                    for block in &item.blocks {
                        match block {
                            TranscriptBlock::Markdown { text } => {
                                blocks.push(DraftBlock::Markdown(text.clone()));
                            }
                            TranscriptBlock::Thinking { text, .. } => {
                                blocks.push(DraftBlock::Thinking {
                                    text: text.clone(),
                                    collapsed: true,
                                    started: None,
                                    duration_ms: None,
                                });
                            }
                            TranscriptBlock::ToolCall(call) => calls.push(call),
                            _ => {}
                        }
                    }
                    if !blocks.is_empty() {
                        let rev = self.next_rev();
                        self.items.push(ChatItem::Assistant {
                            blocks,
                            model: item.model.clone(),
                            usage: item.usage,
                            complete: true,
                            rev,
                        });
                    }
                    for call in calls {
                        let rev = self.next_rev();
                        self.tools.insert(call.id.clone(), self.items.len());
                        self.items.push(ChatItem::Tool {
                            call: Box::new(call.clone()),
                            progress: None,
                            expanded: false,
                            rev,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    fn start_draft(&mut self, id: MessageId) -> usize {
        let rev = self.next_rev();
        let idx = self.items.len();
        self.items.push(ChatItem::Assistant {
            blocks: Vec::new(),
            model: None,
            usage: None,
            complete: false,
            rev,
        });
        self.open.insert(id, idx);
        idx
    }

    /// The draft index for a message, creating the draft if `MessageStarted`
    /// was missed (deltas are ephemeral; arrival order is best-effort).
    fn draft_index(&mut self, id: &MessageId) -> usize {
        match self.open.get(id) {
            Some(idx) => *idx,
            None => self.start_draft(id.clone()),
        }
    }
}

/// Map materialized content blocks to display blocks. Tool use is skipped —
/// tool items render separately from `ToolCallUpdated` records.
fn materialize_blocks(content: &[ContentBlock]) -> Vec<DraftBlock> {
    let mut blocks = Vec::new();
    for block in content {
        match block {
            ContentBlock::Markdown { text } => blocks.push(DraftBlock::Markdown(text.clone())),
            ContentBlock::Thinking { text, .. } => blocks.push(DraftBlock::Thinking {
                text: text.clone(),
                collapsed: true,
                started: None,
                duration_ms: None,
            }),
            _ => {}
        }
    }
    blocks
}

/// Carry measured thinking durations from a streamed draft onto the
/// authoritative materialized blocks (matched in order).
fn carry_thinking_durations(draft: &[DraftBlock], materialized: &mut [DraftBlock]) {
    let mut measured = draft.iter().filter_map(|block| match block {
        DraftBlock::Thinking {
            started,
            duration_ms,
            ..
        } => Some(duration_ms.or_else(|| started.map(|at| at.elapsed().as_millis() as u64))),
        _ => None,
    });
    for block in materialized.iter_mut() {
        if let DraftBlock::Thinking { duration_ms, .. } = block {
            match measured.next() {
                Some(duration) => *duration_ms = duration,
                None => break,
            }
        }
    }
}

fn joined_markdown(content: &[ContentBlock]) -> String {
    let mut out = String::new();
    for block in content {
        if let ContentBlock::Markdown { text } = block {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(text);
        }
    }
    out
}

fn joined_transcript_markdown(blocks: &[TranscriptBlock]) -> String {
    let mut out = String::new();
    for block in blocks {
        if let TranscriptBlock::Markdown { text } = block {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(text);
        }
    }
    out
}

#[cfg(test)]
mod tool_expand_tests {
    use super::{ChatItem, ChatState};
    use agentloop_contracts::{
        MessageId, SessionId, ToolCall, ToolCallId, ToolCallOrigin, ToolCallStatus, ToolCallTiming,
        ToolOutput, TurnId,
    };

    fn sample_tool_call(result: ToolOutput) -> ToolCall {
        ToolCall {
            id: ToolCallId::from("call-expand"),
            session_id: SessionId::from("s1"),
            turn_id: TurnId::from("t1"),
            message_id: MessageId::from("m1"),
            tool_name: "Read".to_owned(),
            input: serde_json::json!({"file_path": "src/lib.rs"}),
            read_only: true,
            origin: ToolCallOrigin::Model,
            status: ToolCallStatus::Completed,
            timing: ToolCallTiming::default(),
            result: Some(result),
        }
    }

    #[test]
    fn toggle_tool_expand_flips_state() {
        let mut chat = ChatState::default();
        chat.apply(&agentloop_contracts::AgentEvent::ToolCallUpdated {
            call: sample_tool_call(ToolOutput::text(
                "1|line one\n2|line two\n3|line three\n4|line four",
            )),
        });
        assert!(matches!(
            chat.items[0],
            ChatItem::Tool {
                expanded: false,
                ..
            }
        ));
        assert!(chat.toggle_focused_tool_expand());
        assert!(matches!(
            chat.items[0],
            ChatItem::Tool { expanded: true, .. }
        ));
        assert_eq!(chat.focused_tool, Some(0));
        assert!(chat.toggle_focused_tool_expand());
        assert!(matches!(
            chat.items[0],
            ChatItem::Tool {
                expanded: false,
                ..
            }
        ));
    }

    #[test]
    fn cycle_tool_focus_visits_expandable_rows() {
        let mut chat = ChatState::default();
        let short = ToolCall {
            id: ToolCallId::from("call-short"),
            ..sample_tool_call(ToolOutput::text("short"))
        };
        let long_a = ToolCall {
            id: ToolCallId::from("call-long-a"),
            ..sample_tool_call(ToolOutput::text("1\n2\n3\n4"))
        };
        let long_b = ToolCall {
            id: ToolCallId::from("call-long-b"),
            ..sample_tool_call(ToolOutput::text("a\nb\nc\nd"))
        };
        chat.apply(&agentloop_contracts::AgentEvent::ToolCallUpdated { call: short });
        chat.apply(&agentloop_contracts::AgentEvent::ToolCallUpdated { call: long_a });
        chat.apply(&agentloop_contracts::AgentEvent::ToolCallUpdated { call: long_b });
        assert!(chat.cycle_tool_focus(false));
        assert_eq!(chat.focused_tool, Some(1));
        assert!(chat.cycle_tool_focus(false));
        assert_eq!(chat.focused_tool, Some(2));
        assert!(chat.cycle_tool_focus(true));
        assert_eq!(chat.focused_tool, Some(1));
    }
}

#[cfg(test)]
mod apply_tests {
    use super::{ChatItem, ChatState, DraftBlock};
    use agentloop_contracts::{AgentEvent, MessageId, Role};

    #[test]
    fn thinking_delta_appends_to_open_draft() {
        let mut chat = ChatState::default();
        let id = MessageId::from("msg-think");
        chat.apply(&AgentEvent::MessageStarted {
            message_id: id.clone(),
            role: Role::Assistant,
        });
        chat.apply(&AgentEvent::ThinkingDelta {
            message_id: id.clone(),
            text: "step one".to_owned(),
        });
        chat.apply(&AgentEvent::ThinkingDelta {
            message_id: id,
            text: " and two".to_owned(),
        });

        let ChatItem::Assistant {
            blocks, complete, ..
        } = &chat.items[0]
        else {
            panic!("expected assistant draft");
        };
        assert!(!complete);
        assert_eq!(blocks.len(), 1);
        assert!(matches!(
            &blocks[0],
            DraftBlock::Thinking {
                text,
                collapsed: false,
                started: Some(_),
                duration_ms: None,
            } if text == "step one and two"
        ));
    }

    #[test]
    fn thinking_delta_starts_new_block_after_markdown() {
        let mut chat = ChatState::default();
        let id = MessageId::from("msg-mixed");
        chat.apply(&AgentEvent::MessageStarted {
            message_id: id.clone(),
            role: Role::Assistant,
        });
        chat.apply(&AgentEvent::MarkdownDelta {
            message_id: id.clone(),
            text: "answer".to_owned(),
        });
        chat.apply(&AgentEvent::ThinkingDelta {
            message_id: id,
            text: "late thought".to_owned(),
        });

        let ChatItem::Assistant { blocks, .. } = &chat.items[0] else {
            panic!("expected assistant draft");
        };
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], DraftBlock::Markdown("answer".to_owned()));
        assert!(matches!(
            &blocks[1],
            DraftBlock::Thinking {
                text,
                collapsed: false,
                ..
            } if text == "late thought"
        ));
    }
}

#[cfg(test)]
mod subagent_tests {
    use super::{ChatItem, ChatState, SubagentOutcome};
    use agentloop_contracts::{
        AgentEvent, MessageId, SessionId, TokenUsage, ToolCall, ToolCallId, ToolCallOrigin,
        ToolCallStatus, ToolCallTiming, TurnId, TurnStopReason, TurnSummary,
    };

    fn task_call(id: &str) -> ToolCall {
        ToolCall {
            id: ToolCallId::from(id),
            session_id: SessionId::from("parent"),
            turn_id: TurnId::from("t1"),
            message_id: MessageId::from("m1"),
            tool_name: "Task".to_owned(),
            input: serde_json::json!({"role": "searcher", "description": "map events"}),
            read_only: true,
            origin: ToolCallOrigin::Model,
            status: ToolCallStatus::Running,
            timing: ToolCallTiming::default(),
            result: None,
        }
    }

    fn started(child: &str, call_id: Option<&str>) -> AgentEvent {
        AgentEvent::SubagentStarted {
            child_session: SessionId::from(child),
            task: "map events".to_owned(),
            call_id: call_id.map(ToolCallId::from),
            role: Some("searcher".to_owned()),
        }
    }

    fn summary(stop_reason: TurnStopReason) -> TurnSummary {
        TurnSummary {
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
        }
    }

    #[test]
    fn subagent_started_nests_under_task_row_and_fixes_indices() {
        let mut chat = ChatState::default();
        chat.apply(&AgentEvent::ToolCallUpdated {
            call: task_call("call-1"),
        });
        chat.apply(&AgentEvent::ToolCallUpdated {
            call: task_call("call-2"),
        });
        chat.apply(&started("child-1", Some("call-1")));

        assert!(
            matches!(&chat.items[1], ChatItem::Subagent { child, .. } if child == &SessionId::from("child-1"))
        );
        // The shifted call-2 row must still be reachable through the index
        // map: a completion update lands on the right item.
        let mut done = task_call("call-2");
        done.status = ToolCallStatus::Completed;
        chat.apply(&AgentEvent::ToolCallUpdated { call: done });
        assert!(matches!(
            &chat.items[2],
            ChatItem::Tool { call, .. }
                if call.id == ToolCallId::from("call-2")
                    && call.status == ToolCallStatus::Completed
        ));
    }

    #[test]
    fn subagent_started_without_call_id_appends() {
        let mut chat = ChatState::default();
        chat.apply(&AgentEvent::ToolCallUpdated {
            call: task_call("call-1"),
        });
        chat.apply(&started("child-1", None));
        assert!(matches!(&chat.items[1], ChatItem::Subagent { .. }));
    }

    #[test]
    fn subagent_event_projects_counts_model_and_activity() {
        let mut chat = ChatState::default();
        chat.apply(&AgentEvent::ToolCallUpdated {
            call: task_call("call-1"),
        });
        chat.apply(&started("child-1", Some("call-1")));

        let child = SessionId::from("child-1");
        let inner_call = ToolCall {
            id: ToolCallId::from("child-call-1"),
            tool_name: "Grep".to_owned(),
            input: serde_json::json!({"pattern": "emit_persistent"}),
            ..task_call("child-call-1")
        };
        // The same child call relayed twice (Running then Completed) counts once.
        for _ in 0..2 {
            chat.apply(&AgentEvent::SubagentEvent {
                child_session: child.clone(),
                event: Box::new(AgentEvent::ToolCallUpdated {
                    call: inner_call.clone(),
                }),
            });
        }
        chat.apply(&AgentEvent::SubagentEvent {
            child_session: child.clone(),
            event: Box::new(AgentEvent::ToolCallUpdated {
                call: ToolCall {
                    id: ToolCallId::from("child-call-2"),
                    ..inner_call.clone()
                },
            }),
        });
        chat.apply(&AgentEvent::SubagentEvent {
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
        });

        let ChatItem::Subagent {
            tool_count,
            tokens,
            model,
            last_activity,
            outcome,
            ..
        } = &chat.items[1]
        else {
            panic!("expected subagent row");
        };
        assert_eq!(*tool_count, 2);
        assert_eq!(*tokens, 12_400);
        assert_eq!(model.as_deref(), Some("deepseek/deepseek-chat"));
        assert!(last_activity.as_deref().is_some_and(|a| a.contains("Grep")));
        assert_eq!(*outcome, SubagentOutcome::Running);
        assert_eq!(chat.subagent_role(&child).as_deref(), Some("searcher"));
    }

    #[test]
    fn subagent_completed_uses_summary_and_outcome() {
        for (stop, expected) in [
            (TurnStopReason::EndTurn, SubagentOutcome::Done),
            (TurnStopReason::Error, SubagentOutcome::Failed),
            (TurnStopReason::MaxIterations, SubagentOutcome::Failed),
            (TurnStopReason::Cancelled, SubagentOutcome::Cancelled),
        ] {
            let mut chat = ChatState::default();
            chat.apply(&started("child-1", None));
            chat.apply(&AgentEvent::SubagentCompleted {
                child_session: SessionId::from("child-1"),
                summary: summary(stop),
            });
            let ChatItem::Subagent {
                outcome,
                duration_ms,
                tool_count,
                tokens,
                ..
            } = &chat.items[0]
            else {
                panic!("expected subagent row");
            };
            assert_eq!(*outcome, expected, "stop reason {stop:?}");
            assert_eq!(*duration_ms, Some(42_000));
            assert_eq!(*tool_count, 9);
            assert_eq!(*tokens, 31_200);
        }
    }

    #[test]
    fn rebuild_from_transcript_clears_subagent_index_map() {
        use agentloop_contracts::Transcript;
        let mut chat = ChatState::default();
        chat.apply(&AgentEvent::ToolCallUpdated {
            call: task_call("call-1"),
        });
        chat.apply(&started("child-1", Some("call-1")));
        assert!(!chat.subagents.is_empty());
        // A Gap/resync rebuilds the item list; the subagent index map must
        // reset so no stale index points into the rebuilt list.
        chat.rebuild_from_transcript(&Transcript::default());
        assert!(
            chat.subagents.is_empty(),
            "stale subagent indices survived resync"
        );
        assert!(chat.items.is_empty());
        // A relayed event for the pre-resync child is now a safe no-op.
        chat.apply(&AgentEvent::SubagentEvent {
            child_session: SessionId::from("child-1"),
            event: Box::new(AgentEvent::ToolCallUpdated {
                call: task_call("child-call-1"),
            }),
        });
        assert!(chat.items.is_empty());
    }

    #[test]
    fn subagent_tree_rebuilds_from_persisted_events_only() {
        // Relays are ephemeral: a resync replays only Started + Completed.
        let mut chat = ChatState::default();
        chat.apply(&AgentEvent::ToolCallUpdated {
            call: task_call("call-1"),
        });
        chat.apply(&started("child-1", Some("call-1")));
        chat.apply(&AgentEvent::SubagentCompleted {
            child_session: SessionId::from("child-1"),
            summary: summary(TurnStopReason::EndTurn),
        });
        let ChatItem::Subagent {
            outcome,
            duration_ms,
            tool_count,
            ..
        } = &chat.items[1]
        else {
            panic!("expected subagent row");
        };
        assert_eq!(*outcome, SubagentOutcome::Done);
        assert_eq!(*duration_ms, Some(42_000));
        assert_eq!(*tool_count, 9);
    }
}

#[cfg(test)]
mod compaction_ui_tests {
    use super::{ChatItem, ChatState};
    use agentloop_contracts::{AgentEvent, CompactionSummary};

    #[test]
    fn manual_compaction_info_message() {
        let mut chat = ChatState::default();
        chat.apply(&AgentEvent::CompactionBoundary {
            summary: CompactionSummary {
                summary_markdown: "summary".to_owned(),
                strategy: "summarize_oldest".to_owned(),
                tokens_before: Some(1_000),
                tokens_after: Some(200),
            },
        });
        assert!(matches!(
            &chat.items[0],
            ChatItem::Info { text }
                if text.contains("Conversation compacted")
                    && text.contains("~1000 → ~200")
        ));
    }

    #[test]
    fn auto_compaction_info_message() {
        let mut chat = ChatState::default();
        chat.apply(&AgentEvent::CompactionBoundary {
            summary: CompactionSummary {
                summary_markdown: "summary".to_owned(),
                strategy: "auto_summarize_oldest".to_owned(),
                tokens_before: Some(120_000),
                tokens_after: Some(8_000),
            },
        });
        assert!(matches!(
            &chat.items[0],
            ChatItem::Info { text }
                if text.contains("Auto-compacted context")
                    && text.contains("approaching limit")
        ));
    }
}

#[cfg(test)]
mod push_info_tests {
    use super::{ChatItem, ChatState};

    #[test]
    fn push_info_dedupes_identical_last_line() {
        let mut chat = ChatState::default();
        chat.push_info("already on native");
        chat.push_info("already on native");
        chat.push_info("already on native");
        assert_eq!(chat.items.len(), 1);
        chat.push_info("something else");
        chat.push_info("already on native");
        assert_eq!(chat.items.len(), 3);
    }

    #[test]
    fn push_info_dedupe_ignores_non_info_last_item() {
        let mut chat = ChatState::default();
        chat.push_info("note");
        chat.push_error("boom");
        chat.push_info("note");
        assert_eq!(chat.items.len(), 3);
        assert!(matches!(&chat.items[2], ChatItem::Info { text } if text == "note"));
    }
}

#[cfg(test)]
mod plain_text_tests {
    use super::ChatState;
    use agentloop_contracts::{AgentEvent, ContentBlock, MessageId};

    #[test]
    fn plain_text_exports_user_and_assistant() {
        let mut chat = ChatState::default();
        chat.push_info("welcome");
        chat.apply(&AgentEvent::UserMessage {
            message_id: MessageId::from("u1"),
            content: vec![ContentBlock::markdown("hello")],
        });
        chat.apply(&AgentEvent::AssistantMessage {
            message_id: MessageId::from("a1"),
            content: vec![ContentBlock::markdown("hi there")],
            model: None,
            usage: None,
        });
        let text = chat.plain_text();
        assert!(text.contains("> hello"));
        assert!(text.contains("hi there"));
    }
}
