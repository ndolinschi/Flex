//! Chat state: the ordered item list and the delta-reconciliation rules.
//!
//! [`ChatState::apply`] maps every content-bearing [`AgentEvent`] onto the
//! item list. The core rule is *authoritative materialization*: streamed
//! deltas accumulate into a draft, and the persisted `AssistantMessage`
//! (or each `TextSnapshot`) **replaces** the accumulated state, so delta
//! loss or duplication self-heals.

use std::collections::HashMap;
use std::time::Instant;

use agentloop_contracts::{
    AgentEvent, ContentBlock, MessageId, PlanEntry, Role, SessionId, TokenUsage, ToolCall,
    ToolCallId, ToolCallStatus, Transcript, TranscriptBlock,
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
    /// An error surfaced by the engine or the CLI.
    Error {
        headline: String,
        detail: Option<String>,
    },
    /// A subagent task marker.
    Subagent {
        child: SessionId,
        task: String,
        done: bool,
    },
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

    /// Append a CLI-local info line. Skipped when it would repeat the
    /// immediately preceding info line (no `already on native` stutter).
    pub fn push_info(&mut self, text: impl Into<String>) {
        let text = text.into();
        if matches!(self.items.last(), Some(ChatItem::Info { text: last }) if *last == text) {
            return;
        }
        self.items.push(ChatItem::Info { text });
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
                ChatItem::Subagent { task, done, .. } => {
                    let marker = if *done { "done" } else { "running" };
                    out.push_str(&format!("subagent {marker}: {task}\n"));
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
            } => {
                self.items.push(ChatItem::Subagent {
                    child: child_session.clone(),
                    task: task.clone(),
                    done: false,
                });
            }
            AgentEvent::SubagentCompleted { child_session, .. } => {
                for item in self.items.iter_mut().rev() {
                    if let ChatItem::Subagent { child, done, .. } = item {
                        if child == child_session {
                            *done = true;
                            break;
                        }
                    }
                }
            }
            // Turn lifecycle, permissions, questions, gaps: app reducer.
            // SubagentEvent, ToolArgsDelta, hooks: not rendered.
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
