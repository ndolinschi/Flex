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
    /// Cursor within the current page's options.
    pub cursor: usize,
}

impl QuestionPrompt {
    pub fn new(id: QuestionId, questions: Vec<Question>) -> Self {
        let picks = vec![Vec::new(); questions.len()];
        Self {
            id,
            questions,
            current: 0,
            picks,
            cursor: 0,
        }
    }

    fn answers(&self) -> Vec<Answer> {
        self.questions
            .iter()
            .zip(&self.picks)
            .map(|(question, picks)| Answer {
                question: question.question.clone(),
                selected: picks
                    .iter()
                    .filter_map(|&i| question.options.get(i))
                    .map(|option| option.label.clone())
                    .collect(),
            })
            .collect()
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmAction {
    AllowAllPermissions,
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
    let Some(question) = prompt.questions.get(prompt.current) else {
        return OverlayOutcome::close();
    };
    let option_count = question.options.len();
    let multi = question.multi_select;
    match (key.code, key.modifiers) {
        (KeyCode::Up, _) => {
            if option_count > 0 {
                prompt.cursor = if prompt.cursor == 0 {
                    option_count - 1
                } else {
                    prompt.cursor - 1
                };
            }
            OverlayOutcome::consumed()
        }
        (KeyCode::Down, _) => {
            if option_count > 0 {
                prompt.cursor = (prompt.cursor + 1) % option_count;
            }
            OverlayOutcome::consumed()
        }
        (KeyCode::Char(' '), _) if multi => {
            let picks = &mut prompt.picks[prompt.current];
            match picks.iter().position(|&i| i == prompt.cursor) {
                Some(pos) => {
                    picks.remove(pos);
                }
                None => picks.push(prompt.cursor),
            }
            OverlayOutcome::consumed()
        }
        (KeyCode::Enter, _) => {
            if !multi {
                prompt.picks[prompt.current] = vec![prompt.cursor];
            }
            if prompt.current + 1 < prompt.questions.len() {
                prompt.current += 1;
                prompt.cursor = 0;
                OverlayOutcome::consumed()
            } else {
                OverlayOutcome {
                    effects: vec![Effect::RespondQuestion {
                        id: prompt.id.clone(),
                        answers: prompt.answers(),
                    }],
                    close: true,
                    ..OverlayOutcome::default()
                }
            }
        }
        // Esc submits what was picked so far — the turn is blocked on the
        // answer and there is no cancel channel for questions.
        (KeyCode::Esc, _) => OverlayOutcome {
            effects: vec![Effect::RespondQuestion {
                id: prompt.id.clone(),
                answers: prompt.answers(),
            }],
            close: true,
            info: Some("question dismissed — sent partial answers".to_owned()),
            ..OverlayOutcome::default()
        },
        _ => OverlayOutcome::consumed(),
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
            confirmed: Some(prompt.action),
            close: true,
            ..OverlayOutcome::default()
        },
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
