//! Modal overlays: pickers, permission/question dialogs, login, help.
//!
//! Exactly one modal is active at a time; permission and question requests
//! arriving while another modal is open queue in the app and drain as
//! modals resolve. Each overlay's key handler returns an [`OverlayOutcome`]
//! so the app reducer stays the single place effects are emitted from.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use agentloop_contracts::{
    Answer, PermissionDecision, PermissionDecisionKind, PermissionRequestId, Question, QuestionId,
    ToolCallId,
};

use agentloop_mcp::McpRemoteTool;

use crate::events::Effect;

/// The active modal.
#[derive(Debug, Default)]
pub enum Overlay {
    #[default]
    None,
    Picker(PickerState),
    Permission(PermissionPrompt),
    Question(QuestionPrompt),
    Login(LoginState),
    Help,
    ShellCommand(ShellCommandOverlay),
    /// One-shot confirmation (e.g. allow-all permissions).
    Confirm(ConfirmPrompt),
    /// `/mcps` — toggle installed servers.
    McpList(McpListState),
    /// `/mcp explore` — manual tool calls.
    McpExplorer(McpExplorerState),
    /// `/mcp-install` wizard.
    McpInstall(McpInstallState),
}

impl Overlay {
    /// Whether a modal is currently active.
    pub fn is_active(&self) -> bool {
        !matches!(self, Self::None)
    }
}

/// What selecting a picker item means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerAction {
    /// Set the session model to the item id (a model ref).
    SetModel,
    /// Set the session model to the named provider's default.
    SwitchProvider,
    /// Switch agent implementation to the item id.
    SwitchAgent,
    /// Set session mode (`code` / `plan`).
    SetSessionMode,
    /// Set permission security level.
    SetPermissionMode,
}

/// One selectable picker row.
#[derive(Debug, Clone)]
pub struct PickerItem {
    /// Machine id handed to the action (model ref, provider, agent id).
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
    /// When false the row is shown dimmed and cannot be selected.
    pub enabled: bool,
}

impl PickerItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            detail: None,
            enabled: true,
        }
    }
}

/// A generic filterable list picker (`/model`, `/provider`, `/agent`).
#[derive(Debug)]
pub struct PickerState {
    pub title: String,
    pub items: Vec<PickerItem>,
    pub filter: String,
    pub selected: usize,
    pub action: PickerAction,
}

impl PickerState {
    pub fn new(title: impl Into<String>, items: Vec<PickerItem>, action: PickerAction) -> Self {
        Self {
            title: title.into(),
            items,
            filter: String::new(),
            selected: 0,
            action,
        }
    }

    /// Rows matching the current filter (case-insensitive substring).
    pub fn visible(&self) -> Vec<&PickerItem> {
        let filter = self.filter.to_lowercase();
        self.items
            .iter()
            .filter(|item| {
                filter.is_empty()
                    || item.label.to_lowercase().contains(&filter)
                    || item.id.to_lowercase().contains(&filter)
            })
            .collect()
    }
}

/// A pending permission request rendered as a dialog.
#[derive(Debug, Clone)]
pub struct PermissionPrompt {
    pub id: PermissionRequestId,
    pub call_id: Option<ToolCallId>,
    pub title: String,
    pub detail: Option<String>,
    pub options: Vec<PermissionDecisionKind>,
    pub selected: usize,
}

/// A multi-page question wizard.
#[derive(Debug, Clone)]
pub struct QuestionPrompt {
    pub id: QuestionId,
    pub questions: Vec<Question>,
    /// Page index.
    pub current: usize,
    /// Selected option indices per question.
    pub picks: Vec<Vec<usize>>,
    /// Free-text answers per question (`None` = option picks).
    pub custom_texts: Vec<Option<String>>,
    /// Cursor within the current page's options.
    pub cursor: usize,
    /// In-progress custom answer for the current page.
    pub custom_input: String,
    /// Whether the user is typing a custom answer on the current page.
    pub custom_mode: bool,
}

impl QuestionPrompt {
    pub fn new(id: QuestionId, questions: Vec<Question>) -> Self {
        let len = questions.len();
        Self {
            id,
            questions,
            current: 0,
            picks: vec![Vec::new(); len],
            custom_texts: vec![None; len],
            cursor: 0,
            custom_input: String::new(),
            custom_mode: false,
        }
    }

    fn current_question(&self) -> Option<&Question> {
        self.questions.get(self.current)
    }

    fn finalize_current_page(&mut self) {
        let Some(question) = self.questions.get(self.current) else {
            return;
        };
        if self.custom_mode {
            let trimmed = self.custom_input.trim();
            if !trimmed.is_empty() {
                self.custom_texts[self.current] = Some(trimmed.to_owned());
                self.picks[self.current].clear();
            }
        } else if !question.multi_select
            && self.picks[self.current].is_empty()
            && !question.options.is_empty()
        {
            self.picks[self.current] = vec![self.cursor];
        }
    }

    fn advance_page(&mut self) {
        self.current += 1;
        self.cursor = 0;
        self.custom_input.clear();
        self.custom_mode = false;
    }

    fn answers(&self) -> Vec<Answer> {
        self.questions
            .iter()
            .enumerate()
            .map(|(idx, question)| {
                if let Some(text) = self.custom_texts[idx].as_ref() {
                    Answer {
                        question: question.question.clone(),
                        selected: vec![text.clone()],
                    }
                } else {
                    Answer {
                        question: question.question.clone(),
                        selected: self.picks[idx]
                            .iter()
                            .filter_map(|&i| question.options.get(i))
                            .map(|option| option.label.clone())
                            .collect(),
                    }
                }
            })
            .collect()
    }

    fn submit_outcome(&self) -> OverlayOutcome {
        OverlayOutcome {
            effects: vec![Effect::RespondQuestion {
                id: self.id.clone(),
                answers: self.answers(),
            }],
            close: true,
            ..OverlayOutcome::default()
        }
    }
}

/// Device-flow login progress, driven entirely by
/// [`agentloop_cli_core::LoginEvent`]s.
#[derive(Debug)]
pub enum LoginState {
    /// Requesting a device code.
    Starting,
    /// Show the code; waiting for the user to enter it on github.com.
    CodeReady {
        user_code: String,
        verification_uri: String,
        expires_in: u64,
        since: Instant,
    },
    /// Waiting for approval.
    Polling { since: Instant },
    /// Token stored; verifying Copilot access.
    Verifying,
    /// The login task failed.
    Failed { message: String },
}

/// A yes/no confirmation dialog.
#[derive(Debug, Clone)]
pub struct ConfirmPrompt {
    pub title: String,
    pub message: String,
    pub action: ConfirmAction,
}

/// What confirming the dialog does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    AllowAllPermissions,
    McpRemove { name: String },
}

/// `/command` overlay: running spinner or scrollable combined output.
#[derive(Debug)]
pub struct ShellCommandOverlay {
    pub command: String,
    pub phase: ShellCommandPhase,
    /// Scroll offset in wrapped output lines (done/failed only).
    pub scroll: usize,
}

/// Lifecycle of a `/command` invocation in the overlay.
#[derive(Debug)]
pub enum ShellCommandPhase {
    Running {
        since: Instant,
    },
    Done {
        output: String,
        exit_code: Option<i32>,
    },
    Failed {
        message: String,
    },
}

impl ShellCommandOverlay {
    pub fn running(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            phase: ShellCommandPhase::Running {
                since: Instant::now(),
            },
            scroll: 0,
        }
    }
}

/// `/mcps` picker: Space toggles enabled, Enter saves.
#[derive(Debug)]
pub struct McpListState {
    pub items: Vec<McpListItem>,
    pub filter: String,
    pub selected: usize,
    pub dirty: bool,
}

/// One row in the MCP list overlay.
#[derive(Debug, Clone)]
pub struct McpListItem {
    pub name: String,
    pub source: String,
    pub enabled: bool,
}

/// `/mcp explore` overlay lifecycle.
#[derive(Debug)]
pub struct McpExplorerState {
    pub server: String,
    pub phase: McpExplorerPhase,
    pub selected: usize,
    pub filter: String,
    pub args_input: String,
    pub args_mode: bool,
    pub scroll: usize,
}

/// Explorer phases.
#[derive(Debug)]
pub enum McpExplorerPhase {
    Loading,
    Tools { tools: Vec<McpRemoteTool> },
    Calling,
    Result { output: String, is_error: bool },
    Failed { message: String },
}

/// `/mcp-install` wizard.
#[derive(Debug)]
pub struct McpInstallState {
    pub mode: McpInstallMode,
    pub filter: String,
    pub selected: usize,
    pub input: String,
    pub input_mode: bool,
}

/// Install wizard tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpInstallMode {
    Registry,
    Npm,
    Import,
}

/// What an overlay key press produced.
#[derive(Debug, Default)]
pub struct OverlayOutcome {
    pub effects: Vec<Effect>,
    /// Close the overlay (the app then drains queued prompts).
    pub close: bool,
    /// Info line to append to the chat.
    pub info: Option<String>,
    /// A picker selection for the app to apply.
    pub choice: Option<PickerChoice>,
    /// A confirmed dialog action.
    pub confirmed: Option<ConfirmAction>,
    /// MCP list saved (Enter on `/mcps`).
    pub mcp_list_saved: bool,
    /// MCP install wizard selection.
    pub mcp_install: Option<McpInstallChoice>,
}

impl OverlayOutcome {
    fn close() -> Self {
        Self {
            close: true,
            ..Self::default()
        }
    }

    fn consumed() -> Self {
        Self::default()
    }
}

/// Handle a key while a modal is active. Returns `None` when the overlay
/// did not consume the key.
pub fn handle_key(overlay: &mut Overlay, key: KeyEvent) -> Option<OverlayOutcome> {
    match overlay {
        Overlay::None => None,
        Overlay::Picker(picker) => Some(picker_key(picker, key)),
        Overlay::Permission(prompt) => Some(permission_key(prompt, key)),
        Overlay::Question(prompt) => Some(question_key(prompt, key)),
        Overlay::Login(state) => Some(login_key(state, key)),
        Overlay::Help => Some(OverlayOutcome::close()),
        Overlay::ShellCommand(state) => Some(shell_command_key(state, key)),
        Overlay::Confirm(prompt) => Some(confirm_key(prompt, key)),
        Overlay::McpList(state) => Some(mcp_list_key(state, key)),
        Overlay::McpExplorer(state) => Some(mcp_explorer_key(state, key)),
        Overlay::McpInstall(state) => Some(mcp_install_key(state, key)),
    }
}

/// A confirmed picker selection, applied by the app reducer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerChoice {
    SetModel(String),
    SwitchProvider(String),
    SwitchAgent(String),
    SetSessionMode(String),
    SetPermissionMode(String),
}

/// What the install wizard selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpInstallChoice {
    Registry { id: String },
    Npm { package: String },
    Import { path: String },
}

fn picker_key(picker: &mut PickerState, key: KeyEvent) -> OverlayOutcome {
    let visible_len = picker.visible().len();
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => OverlayOutcome::close(),
        (KeyCode::Up, _) => {
            if visible_len > 0 {
                picker.selected = if picker.selected == 0 {
                    visible_len - 1
                } else {
                    picker.selected - 1
                };
            }
            OverlayOutcome::consumed()
        }
        (KeyCode::Down, _) => {
            if visible_len > 0 {
                picker.selected = (picker.selected + 1) % visible_len;
            }
            OverlayOutcome::consumed()
        }
        (KeyCode::Backspace, _) => {
            picker.filter.pop();
            picker.selected = 0;
            OverlayOutcome::consumed()
        }
        (KeyCode::Enter, _) => {
            let chosen = picker
                .visible()
                .get(picker.selected)
                .filter(|item| item.enabled)
                .map(|item| item.id.clone());
            match chosen {
                Some(id) => OverlayOutcome {
                    choice: Some(match picker.action {
                        PickerAction::SetModel => PickerChoice::SetModel(id),
                        PickerAction::SwitchProvider => PickerChoice::SwitchProvider(id),
                        PickerAction::SwitchAgent => PickerChoice::SwitchAgent(id),
                        PickerAction::SetSessionMode => PickerChoice::SetSessionMode(id),
                        PickerAction::SetPermissionMode => PickerChoice::SetPermissionMode(id),
                    }),
                    close: true,
                    ..OverlayOutcome::default()
                },
                None => OverlayOutcome::consumed(),
            }
        }
        (KeyCode::Char(c), m) if m.is_empty() || m == KeyModifiers::SHIFT => {
            picker.filter.push(c);
            picker.selected = 0;
            OverlayOutcome::consumed()
        }
        _ => OverlayOutcome::consumed(),
    }
}

fn permission_key(prompt: &mut PermissionPrompt, key: KeyEvent) -> OverlayOutcome {
    let deny = |prompt: &PermissionPrompt| OverlayOutcome {
        effects: vec![Effect::RespondPermission {
            id: prompt.id.clone(),
            decision: PermissionDecision::Deny { reason: None },
        }],
        close: true,
        info: None,
        ..OverlayOutcome::default()
    };
    match (key.code, key.modifiers) {
        (KeyCode::Up | KeyCode::Left, _) => {
            if !prompt.options.is_empty() {
                prompt.selected = if prompt.selected == 0 {
                    prompt.options.len() - 1
                } else {
                    prompt.selected - 1
                };
            }
            OverlayOutcome::consumed()
        }
        (KeyCode::Down | KeyCode::Right | KeyCode::Tab, _) => {
            if !prompt.options.is_empty() {
                prompt.selected = (prompt.selected + 1) % prompt.options.len();
            }
            OverlayOutcome::consumed()
        }
        (KeyCode::Enter, _) | (KeyCode::Char('y'), KeyModifiers::NONE) => {
            let Some(kind) = prompt.options.get(prompt.selected) else {
                return deny(prompt);
            };
            OverlayOutcome {
                effects: vec![Effect::RespondPermission {
                    id: prompt.id.clone(),
                    decision: decision_for(*kind),
                }],
                close: true,
                ..OverlayOutcome::default()
            }
        }
        (KeyCode::Char('a'), KeyModifiers::NONE)
            if prompt
                .options
                .contains(&PermissionDecisionKind::AllowAlways) =>
        {
            OverlayOutcome {
                effects: vec![Effect::RespondPermission {
                    id: prompt.id.clone(),
                    decision: PermissionDecision::AllowAlways,
                }],
                close: true,
                ..OverlayOutcome::default()
            }
        }
        // Esc never leaves a request dangling while the turn blocks on it.
        (KeyCode::Esc, _) | (KeyCode::Char('n'), KeyModifiers::NONE) => deny(prompt),
        _ => OverlayOutcome::consumed(),
    }
}

fn decision_for(kind: PermissionDecisionKind) -> PermissionDecision {
    match kind {
        PermissionDecisionKind::AllowOnce => PermissionDecision::AllowOnce,
        PermissionDecisionKind::AllowAlways => PermissionDecision::AllowAlways,
        _ => PermissionDecision::Deny { reason: None },
    }
}

fn question_key(prompt: &mut QuestionPrompt, key: KeyEvent) -> OverlayOutcome {
    let Some(question) = prompt.current_question().cloned() else {
        return OverlayOutcome::close();
    };
    let option_count = question.options.len();
    let multi = question.multi_select;
    let allow_custom = question.allow_custom;

    match (key.code, key.modifiers) {
        (KeyCode::Up, _) if !prompt.custom_mode => {
            if option_count > 0 {
                prompt.cursor = if prompt.cursor == 0 {
                    option_count - 1
                } else {
                    prompt.cursor - 1
                };
            }
            OverlayOutcome::consumed()
        }
        (KeyCode::Down, _) if !prompt.custom_mode => {
            if option_count > 0 {
                prompt.cursor = (prompt.cursor + 1) % option_count;
            }
            OverlayOutcome::consumed()
        }
        (KeyCode::Char(' '), _) if prompt.custom_mode => {
            prompt.custom_input.push(' ');
            OverlayOutcome::consumed()
        }
        (KeyCode::Char(' '), _) if multi => {
            toggle_pick(&mut prompt.picks[prompt.current], prompt.cursor);
            prompt.custom_mode = false;
            prompt.custom_input.clear();
            prompt.custom_texts[prompt.current] = None;
            OverlayOutcome::consumed()
        }
        (KeyCode::Char(' '), _) if option_count > 0 => {
            prompt.picks[prompt.current] = vec![prompt.cursor];
            prompt.custom_mode = false;
            prompt.custom_input.clear();
            prompt.custom_texts[prompt.current] = None;
            OverlayOutcome::consumed()
        }
        (KeyCode::Enter, _) => {
            prompt.finalize_current_page();
            if prompt.current + 1 < prompt.questions.len() {
                prompt.advance_page();
                OverlayOutcome::consumed()
            } else {
                prompt.submit_outcome()
            }
        }
        (KeyCode::Backspace, _) if prompt.custom_mode => {
            prompt.custom_input.pop();
            if prompt.custom_input.is_empty() {
                prompt.custom_mode = false;
            }
            OverlayOutcome::consumed()
        }
        (KeyCode::Char(c), m) if c.is_ascii_digit() && m.is_empty() && !prompt.custom_mode => {
            let digit = c.to_digit(10).unwrap_or(0) as usize;
            if digit == 0 || digit > option_count {
                return OverlayOutcome::consumed();
            }
            let idx = digit - 1;
            if multi {
                toggle_pick(&mut prompt.picks[prompt.current], idx);
            } else {
                prompt.picks[prompt.current] = vec![idx];
                prompt.cursor = idx;
            }
            prompt.custom_mode = false;
            prompt.custom_input.clear();
            prompt.custom_texts[prompt.current] = None;
            OverlayOutcome::consumed()
        }
        (KeyCode::Char(c), m) if allow_custom && (m.is_empty() || m == KeyModifiers::SHIFT) => {
            if c == ' ' && !prompt.custom_mode {
                return OverlayOutcome::consumed();
            }
            prompt.custom_mode = true;
            prompt.picks[prompt.current].clear();
            prompt.custom_texts[prompt.current] = None;
            prompt.custom_input.push(c);
            OverlayOutcome::consumed()
        }
        // Esc submits what was picked so far — the turn is blocked on the
        // answer and there is no cancel channel for questions.
        (KeyCode::Esc, _) => {
            prompt.finalize_current_page();
            OverlayOutcome {
                effects: vec![Effect::RespondQuestion {
                    id: prompt.id.clone(),
                    answers: prompt.answers(),
                }],
                close: true,
                info: Some("question dismissed — sent partial answers".to_owned()),
                ..OverlayOutcome::default()
            }
        }
        _ => OverlayOutcome::consumed(),
    }
}

fn toggle_pick(picks: &mut Vec<usize>, idx: usize) {
    match picks.iter().position(|&i| i == idx) {
        Some(pos) => {
            picks.remove(pos);
        }
        None => picks.push(idx),
    }
}

fn login_key(state: &mut LoginState, key: KeyEvent) -> OverlayOutcome {
    match (key.code, &state) {
        (KeyCode::Esc, LoginState::Failed { .. }) | (KeyCode::Enter, LoginState::Failed { .. }) => {
            OverlayOutcome::close()
        }
        (KeyCode::Esc, _) => OverlayOutcome {
            effects: vec![Effect::CancelLogin],
            close: true,
            info: Some("login cancelled".to_owned()),
            ..OverlayOutcome::default()
        },
        (
            KeyCode::Char('o'),
            LoginState::CodeReady {
                verification_uri, ..
            },
        ) => OverlayOutcome {
            effects: vec![Effect::OpenBrowser {
                url: verification_uri.clone(),
            }],
            ..OverlayOutcome::default()
        },
        _ => OverlayOutcome::consumed(),
    }
}

fn confirm_key(prompt: &ConfirmPrompt, key: KeyEvent) -> OverlayOutcome {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => OverlayOutcome::close(),
        (KeyCode::Enter, _) | (KeyCode::Char('y'), KeyModifiers::NONE) => OverlayOutcome {
            confirmed: Some(prompt.action.clone()),
            close: true,
            ..OverlayOutcome::default()
        },
        _ => OverlayOutcome::consumed(),
    }
}

fn mcp_list_visible(state: &McpListState) -> Vec<&McpListItem> {
    let filter = state.filter.to_lowercase();
    state
        .items
        .iter()
        .filter(|item| {
            filter.is_empty()
                || item.name.to_lowercase().contains(&filter)
                || item.source.to_lowercase().contains(&filter)
        })
        .collect()
}

fn mcp_list_key(state: &mut McpListState, key: KeyEvent) -> OverlayOutcome {
    let visible_len = mcp_list_visible(state).len();
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => OverlayOutcome::close(),
        (KeyCode::Up, _) => {
            if visible_len > 0 {
                state.selected = if state.selected == 0 {
                    visible_len - 1
                } else {
                    state.selected - 1
                };
            }
            OverlayOutcome::consumed()
        }
        (KeyCode::Down, _) => {
            if visible_len > 0 {
                state.selected = (state.selected + 1) % visible_len;
            }
            OverlayOutcome::consumed()
        }
        (KeyCode::Char(' '), _) => {
            if let Some(item) = mcp_list_visible(state).get(state.selected) {
                let name = item.name.clone();
                if let Some(row) = state.items.iter_mut().find(|row| row.name == name) {
                    row.enabled = !row.enabled;
                    state.dirty = true;
                }
            }
            OverlayOutcome::consumed()
        }
        (KeyCode::Backspace, _) => {
            state.filter.pop();
            state.selected = 0;
            OverlayOutcome::consumed()
        }
        (KeyCode::Enter, _) => OverlayOutcome {
            mcp_list_saved: true,
            close: true,
            ..OverlayOutcome::default()
        },
        (KeyCode::Char(c), m) if m.is_empty() || m == KeyModifiers::SHIFT => {
            state.filter.push(c);
            state.selected = 0;
            OverlayOutcome::consumed()
        }
        _ => OverlayOutcome::consumed(),
    }
}

fn mcp_explorer_visible_tools(state: &McpExplorerState) -> Vec<&McpRemoteTool> {
    let McpExplorerPhase::Tools { tools } = &state.phase else {
        return Vec::new();
    };
    let filter = state.filter.to_lowercase();
    tools
        .iter()
        .filter(|tool| {
            filter.is_empty()
                || tool.name.to_lowercase().contains(&filter)
                || tool.description.to_lowercase().contains(&filter)
        })
        .collect()
}

fn mcp_explorer_key(state: &mut McpExplorerState, key: KeyEvent) -> OverlayOutcome {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => {
            if state.args_mode {
                state.args_mode = false;
                state.args_input.clear();
                OverlayOutcome::consumed()
            } else {
                OverlayOutcome::close()
            }
        }
        (KeyCode::Enter, _) if state.args_mode => {
            let tool = match &state.phase {
                McpExplorerPhase::Tools { tools } => mcp_explorer_visible_tools(state)
                    .get(state.selected)
                    .map(|tool| tool.name.clone())
                    .or_else(|| tools.get(state.selected).map(|tool| tool.name.clone())),
                _ => None,
            };
            let Some(tool) = tool else {
                return OverlayOutcome::consumed();
            };
            state.phase = McpExplorerPhase::Calling;
            OverlayOutcome {
                effects: vec![Effect::McpCallTool {
                    server: state.server.clone(),
                    tool,
                    args_json: state.args_input.clone(),
                }],
                ..OverlayOutcome::default()
            }
        }
        (KeyCode::Enter, _) => match &state.phase {
            McpExplorerPhase::Tools { .. } => {
                state.args_mode = true;
                if state.args_input.is_empty() {
                    state.args_input = "{}".to_owned();
                }
                OverlayOutcome::consumed()
            }
            McpExplorerPhase::Result { .. } | McpExplorerPhase::Failed { .. } => {
                OverlayOutcome::close()
            }
            _ => OverlayOutcome::consumed(),
        },
        (KeyCode::Up, _) if matches!(state.phase, McpExplorerPhase::Result { .. }) => {
            state.scroll = state.scroll.saturating_sub(1);
            OverlayOutcome::consumed()
        }
        (KeyCode::Down, _) if matches!(state.phase, McpExplorerPhase::Result { .. }) => {
            state.scroll = state.scroll.saturating_add(1);
            OverlayOutcome::consumed()
        }
        (KeyCode::Up, _) if !state.args_mode => {
            let len = mcp_explorer_visible_tools(state).len();
            if len > 0 {
                state.selected = if state.selected == 0 {
                    len - 1
                } else {
                    state.selected - 1
                };
            }
            OverlayOutcome::consumed()
        }
        (KeyCode::Down, _) if !state.args_mode => {
            let len = mcp_explorer_visible_tools(state).len();
            if len > 0 {
                state.selected = (state.selected + 1) % len;
            }
            OverlayOutcome::consumed()
        }
        (KeyCode::Backspace, _) if state.args_mode => {
            state.args_input.pop();
            OverlayOutcome::consumed()
        }
        (KeyCode::Backspace, _) => {
            state.filter.pop();
            state.selected = 0;
            OverlayOutcome::consumed()
        }
        (KeyCode::Char(c), m) if state.args_mode && (m.is_empty() || m == KeyModifiers::SHIFT) => {
            state.args_input.push(c);
            OverlayOutcome::consumed()
        }
        (KeyCode::Char(c), m) if !state.args_mode && (m.is_empty() || m == KeyModifiers::SHIFT) => {
            state.filter.push(c);
            state.selected = 0;
            OverlayOutcome::consumed()
        }
        _ => OverlayOutcome::consumed(),
    }
}

fn mcp_install_key(state: &mut McpInstallState, key: KeyEvent) -> OverlayOutcome {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => {
            if state.input_mode {
                state.input_mode = false;
                state.input.clear();
                OverlayOutcome::consumed()
            } else {
                OverlayOutcome::close()
            }
        }
        (KeyCode::Tab, _) => {
            state.mode = match state.mode {
                McpInstallMode::Registry => McpInstallMode::Npm,
                McpInstallMode::Npm => McpInstallMode::Import,
                McpInstallMode::Import => McpInstallMode::Registry,
            };
            state.selected = 0;
            state.filter.clear();
            OverlayOutcome::consumed()
        }
        (KeyCode::Up, _) if !state.input_mode && state.mode == McpInstallMode::Registry => {
            if state.selected > 0 {
                state.selected -= 1;
            }
            OverlayOutcome::consumed()
        }
        (KeyCode::Down, _) if !state.input_mode && state.mode == McpInstallMode::Registry => {
            state.selected = state.selected.saturating_add(1);
            OverlayOutcome::consumed()
        }
        (KeyCode::Enter, _) if state.input_mode => {
            let input = state.input.trim().to_owned();
            if input.is_empty() {
                return OverlayOutcome::consumed();
            }
            let choice = match state.mode {
                McpInstallMode::Npm => McpInstallChoice::Npm { package: input },
                McpInstallMode::Import => McpInstallChoice::Import { path: input },
                McpInstallMode::Registry => return OverlayOutcome::consumed(),
            };
            OverlayOutcome {
                mcp_install: Some(choice),
                close: true,
                ..OverlayOutcome::default()
            }
        }
        (KeyCode::Enter, _) if state.mode == McpInstallMode::Registry => {
            let entries = agentloop_cli_core::registry();
            let filter = state.filter.to_lowercase();
            let visible: Vec<_> = entries
                .iter()
                .filter(|entry| {
                    filter.is_empty()
                        || entry.name.contains(&filter)
                        || entry.label.to_lowercase().contains(&filter)
                })
                .collect();
            let Some(entry) = visible.get(state.selected) else {
                return OverlayOutcome::consumed();
            };
            OverlayOutcome {
                mcp_install: Some(McpInstallChoice::Registry {
                    id: entry.name.clone(),
                }),
                close: true,
                ..OverlayOutcome::default()
            }
        }
        (KeyCode::Enter, _) => {
            state.input_mode = true;
            OverlayOutcome::consumed()
        }
        (KeyCode::Backspace, _) if state.input_mode => {
            state.input.pop();
            OverlayOutcome::consumed()
        }
        (KeyCode::Backspace, _) => {
            state.filter.pop();
            state.selected = 0;
            OverlayOutcome::consumed()
        }
        (KeyCode::Char(c), m) if state.input_mode && (m.is_empty() || m == KeyModifiers::SHIFT) => {
            state.input.push(c);
            OverlayOutcome::consumed()
        }
        (KeyCode::Char(c), m)
            if !state.input_mode
                && state.mode == McpInstallMode::Registry
                && (m.is_empty() || m == KeyModifiers::SHIFT) =>
        {
            state.filter.push(c);
            state.selected = 0;
            OverlayOutcome::consumed()
        }
        _ => OverlayOutcome::consumed(),
    }
}

fn shell_command_key(state: &mut ShellCommandOverlay, key: KeyEvent) -> OverlayOutcome {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => match &state.phase {
            ShellCommandPhase::Running { .. } => OverlayOutcome {
                effects: vec![Effect::CancelShellCommand],
                close: true,
                info: Some(format!("cancelled: {}", state.command)),
                ..OverlayOutcome::default()
            },
            _ => OverlayOutcome::close(),
        },
        (KeyCode::Up, _) => {
            state.scroll = state.scroll.saturating_sub(1);
            OverlayOutcome::consumed()
        }
        (KeyCode::Down, _) => {
            state.scroll = state.scroll.saturating_add(1);
            OverlayOutcome::consumed()
        }
        (KeyCode::PageUp, _) => {
            state.scroll = state.scroll.saturating_sub(10);
            OverlayOutcome::consumed()
        }
        (KeyCode::PageDown, _) => {
            state.scroll = state.scroll.saturating_add(10);
            OverlayOutcome::consumed()
        }
        _ => OverlayOutcome::consumed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::{Question, QuestionId, QuestionOption};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn sample_question(multi_select: bool) -> Question {
        Question {
            header: "Pick".to_owned(),
            question: "Which one?".to_owned(),
            options: vec![
                QuestionOption {
                    label: "alpha".to_owned(),
                    description: None,
                },
                QuestionOption {
                    label: "beta".to_owned(),
                    description: None,
                },
                QuestionOption {
                    label: "gamma".to_owned(),
                    description: None,
                },
            ],
            multi_select,
            allow_custom: true,
        }
    }

    fn sample_prompt(multi_select: bool) -> QuestionPrompt {
        QuestionPrompt::new(
            QuestionId::from("q-test"),
            vec![sample_question(multi_select)],
        )
    }

    #[test]
    fn multi_select_space_toggles_options() {
        let mut prompt = sample_prompt(true);
        let outcome = question_key(&mut prompt, key(KeyCode::Char(' ')));
        assert!(!outcome.close);
        assert_eq!(prompt.picks[0], vec![0]);

        let outcome = question_key(&mut prompt, key(KeyCode::Down));
        assert!(!outcome.close);
        let outcome = question_key(&mut prompt, key(KeyCode::Char(' ')));
        assert!(!outcome.close);
        assert_eq!(prompt.picks[0], vec![0, 1]);

        let outcome = question_key(&mut prompt, key(KeyCode::Char(' ')));
        assert!(!outcome.close);
        assert_eq!(prompt.picks[0], vec![0]);
    }

    #[test]
    fn multi_select_enter_submits_all_labels() {
        let mut prompt = sample_prompt(true);
        question_key(&mut prompt, key(KeyCode::Char(' ')));
        question_key(&mut prompt, key(KeyCode::Down));
        question_key(&mut prompt, key(KeyCode::Char(' ')));

        let outcome = question_key(&mut prompt, key(KeyCode::Enter));
        assert!(outcome.close);
        let effect = outcome.effects.first().expect("respond effect");
        let Effect::RespondQuestion { answers, .. } = effect else {
            panic!("expected RespondQuestion");
        };
        assert_eq!(answers[0].selected, vec!["alpha", "beta"]);
    }

    #[test]
    fn custom_answer_replaces_option_picks() {
        let mut prompt = sample_prompt(false);
        question_key(&mut prompt, key(KeyCode::Char('1')));
        assert_eq!(prompt.picks[0], vec![0]);

        question_key(&mut prompt, key(KeyCode::Char('m')));
        question_key(&mut prompt, key(KeyCode::Char('y')));
        question_key(&mut prompt, key(KeyCode::Char(' ')));
        question_key(&mut prompt, key(KeyCode::Char('a')));
        question_key(&mut prompt, key(KeyCode::Char('n')));
        question_key(&mut prompt, key(KeyCode::Char('s')));
        question_key(&mut prompt, key(KeyCode::Char('w')));
        question_key(&mut prompt, key(KeyCode::Char('e')));
        question_key(&mut prompt, key(KeyCode::Char('r')));
        assert!(prompt.custom_mode);
        assert!(prompt.picks[0].is_empty());

        let outcome = question_key(&mut prompt, key(KeyCode::Enter));
        assert!(outcome.close);
        let Effect::RespondQuestion { answers, .. } = &outcome.effects[0] else {
            panic!("expected RespondQuestion");
        };
        assert_eq!(answers[0].selected, vec!["my answer"]);
    }

    #[test]
    fn single_select_number_key_picks_option() {
        let mut prompt = sample_prompt(false);
        question_key(&mut prompt, key(KeyCode::Char('2')));
        assert_eq!(prompt.picks[0], vec![1]);
        assert_eq!(prompt.cursor, 1);

        let outcome = question_key(&mut prompt, key(KeyCode::Enter));
        assert!(outcome.close);
        let Effect::RespondQuestion { answers, .. } = &outcome.effects[0] else {
            panic!("expected RespondQuestion");
        };
        assert_eq!(answers[0].selected, vec!["beta"]);
    }

    #[test]
    fn toggle_pick_adds_and_removes() {
        let mut picks = Vec::new();
        toggle_pick(&mut picks, 2);
        assert_eq!(picks, vec![2]);
        toggle_pick(&mut picks, 2);
        assert!(picks.is_empty());
    }
}
