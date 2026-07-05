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
    SessionId, ToolCallId,
};

use agentloop_mcp::McpRemoteTool;

use crate::events::Effect;
use crate::ui::diff::DiffPreview;

/// Lightweight subsequence fuzzy score: higher is better. Empty filter matches
/// everything with score 0.
pub(crate) fn fuzzy_score(filter: &str, text: &str) -> Option<i32> {
    let filter = filter.trim();
    if filter.is_empty() {
        return Some(0);
    }
    let needle: Vec<char> = filter.to_lowercase().chars().collect();
    let haystack: Vec<char> = text.to_lowercase().chars().collect();
    if needle.is_empty() {
        return Some(0);
    }
    if haystack
        .windows(needle.len())
        .any(|window| window == needle.as_slice())
    {
        return Some(10_000);
    }
    let mut score = 0i32;
    let mut last_match: Option<usize> = None;
    let mut needle_idx = 0usize;
    for (idx, ch) in haystack.iter().enumerate() {
        if needle_idx < needle.len() && *ch == needle[needle_idx] {
            let gap = last_match.map(|prev| idx.saturating_sub(prev).saturating_sub(1));
            score += 100;
            if let Some(gap) = gap {
                score -= (gap as i32).saturating_mul(5);
            }
            last_match = Some(idx);
            needle_idx += 1;
        }
    }
    (needle_idx == needle.len()).then_some(score)
}

/// Rank searchable rows by fuzzy score (desc), then label/id (asc).
pub(crate) fn fuzzy_rank<'a, I, F>(filter: &str, items: I, searchable: F) -> Vec<usize>
where
    I: IntoIterator<Item = (usize, &'a str)>,
    F: Fn(&str) -> Vec<&'a str>,
{
    let filter = filter.trim();
    let mut ranked: Vec<(i32, usize, String)> = items
        .into_iter()
        .filter_map(|(idx, primary)| {
            let mut best = fuzzy_score(filter, primary)?;
            for field in searchable(primary) {
                if let Some(score) = fuzzy_score(filter, field) {
                    best = best.max(score);
                }
            }
            Some((best, idx, primary.to_lowercase()))
        })
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(&b.2)));
    ranked.into_iter().map(|(_, idx, _)| idx).collect()
}

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
    /// Ctrl+P — fuzzy search over commands and actions.
    CommandPalette(CommandPaletteState),
    /// `?` / Ctrl+? — context-sensitive key hints.
    WhichKey,
    /// `/connect` step-through wizard (no args).
    ConnectWizard(ConnectWizardState),
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
    /// Set the color theme to the item id.
    SetTheme,
    /// Set the effort level to the item id.
    SetEffort,
    /// Resume a past session by id (`/sessions`).
    ResumeSession,
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

    /// Rows matching the current filter (fuzzy subsequence rank).
    pub fn visible(&self) -> Vec<&PickerItem> {
        let filter = self.filter.trim();
        if filter.is_empty() {
            return self.items.iter().collect();
        }
        let ranked = fuzzy_rank(
            filter,
            self.items
                .iter()
                .enumerate()
                .map(|(idx, item)| (idx, item.label.as_str())),
            |primary| {
                self.items
                    .iter()
                    .find(|item| item.label == primary)
                    .map(|item| {
                        let mut fields = vec![item.id.as_str()];
                        if let Some(detail) = item.detail.as_deref() {
                            fields.push(detail);
                        }
                        fields
                    })
                    .unwrap_or_default()
            },
        );
        ranked
            .into_iter()
            .filter_map(|idx| self.items.get(idx))
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
    /// Child session for relayed subagent prompts; `None` = current.
    pub session: Option<SessionId>,
    /// Role badge for relayed subagent prompts (`[worker] Allow ...`).
    pub role: Option<String>,
    /// Inline diff preview for Edit/Write permission prompts.
    pub(crate) diff: Option<DiffPreview>,
    /// Whether the diff preview is expanded (`d` toggles).
    pub diff_expanded: bool,
}

/// A multi-page question wizard.
#[derive(Debug, Clone)]
pub struct QuestionPrompt {
    pub id: QuestionId,
    pub questions: Vec<Question>,
    /// Child session for relayed subagent prompts; `None` = current.
    pub session: Option<SessionId>,
    /// Role badge for relayed subagent prompts.
    pub role: Option<String>,
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
            session: None,
            role: None,
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
                session: self.session.clone(),
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
    McpRemove {
        name: String,
    },
    /// Adopt these project-declared MCP servers into the global config.
    McpImport {
        servers: Vec<agentloop_cli_core::InstalledMcpServer>,
    },
    /// Save a provider after validation failed.
    SaveProviderAnyway {
        id: String,
        config: agentloop_cli_core::ProviderConfig,
    },
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

/// One row in the command palette.
#[derive(Debug, Clone)]
pub struct CommandPaletteEntry {
    pub title: String,
    pub description: String,
    pub category: &'static str,
    /// When true, render a dim section header row above this entry.
    pub section_header: Option<String>,
    pub key_hint: Option<String>,
    pub action: CommandPaletteAction,
}

/// What selecting a palette row does (handled by the app reducer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandPaletteAction {
    Local(crate::commands::LocalCommand),
    EngineCommand(String),
    OpenModelPicker,
    OpenProviderPicker,
    OpenAgentPicker,
    OpenSessionsPicker,
    ScrollToBottom,
    ToggleThinking,
    CopyTranscript,
    CyclePermissionMode,
    ToggleMouseCapture,
}

/// Ctrl+P fuzzy command palette.
#[derive(Debug)]
pub struct CommandPaletteState {
    pub entries: Vec<CommandPaletteEntry>,
    pub filter: String,
    pub selected: usize,
}

impl CommandPaletteState {
    pub fn visible(&self) -> Vec<&CommandPaletteEntry> {
        let filter = self.filter.trim();
        if filter.is_empty() {
            return self.entries.iter().collect();
        }
        let ranked = fuzzy_rank(
            filter,
            self.entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.section_header.is_none())
                .map(|(idx, entry)| (idx, entry.title.as_str())),
            |primary| {
                self.entries
                    .iter()
                    .find(|entry| entry.title == primary)
                    .map(|entry| {
                        let mut fields = vec![entry.category, entry.description.as_str()];
                        if let Some(hint) = entry.key_hint.as_deref() {
                            fields.push(hint);
                        }
                        fields
                    })
                    .unwrap_or_default()
            },
        );
        ranked
            .into_iter()
            .filter_map(|idx| self.entries.get(idx))
            .collect()
    }
}

/// `/connect` wizard step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectWizardStep {
    /// Categorized provider gallery.
    PickProvider,
    /// Choose browser / headless / API key (OpenAI).
    PickAuthMethod,
    /// Free-text id for a custom OpenAI-compatible endpoint.
    CustomProviderId,
    BaseUrl,
    ApiKey,
    /// Browser or headless OAuth in progress.
    OAuthWaiting,
    Model,
}

/// Step-through custom provider setup.
#[derive(Debug)]
pub struct ConnectWizardState {
    pub step: ConnectWizardStep,
    pub filter: String,
    pub selected: usize,
    pub id: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub input: String,
    /// Gallery template id when defaults are pre-filled.
    pub template_id: Option<String>,
    /// Auth methods for the current template.
    pub auth_methods: Vec<agentloop_cli_core::AuthMethodSpec>,
    /// Label shown on the OAuth waiting step.
    pub auth_method_label: Option<String>,
    pub oauth_url: Option<String>,
    pub oauth_instructions: Option<String>,
    pub oauth_waiting: bool,
    pending_oauth_method: Option<agentloop_cli_core::OpenAiOAuthMethod>,
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
    /// Theme id to preview live (highlight moved in the theme picker); the app
    /// applies it without persisting.
    pub preview_theme: Option<String>,
    /// Restore the theme snapshot taken when the theme picker opened (the user
    /// cancelled the picker).
    pub revert_theme: bool,
    /// A command-palette selection for the app to apply.
    pub palette_action: Option<CommandPaletteAction>,
    /// Connect wizard finished — validate the assembled provider.
    pub connect_wizard: Option<(String, agentloop_cli_core::ProviderConfig)>,
    /// Gallery picked Copilot — start device-flow sign-in.
    pub start_copilot_login: bool,
    /// OpenAI OAuth method to start when the wizard enters OAuthWaiting.
    pub start_openai_oauth: Option<agentloop_cli_core::OpenAiOAuthMethod>,
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
/// Strip line breaks from a bracketed paste destined for a single-line field.
fn single_line_paste(text: &str) -> String {
    text.chars().filter(|c| *c != '\n' && *c != '\r').collect()
}

/// Handle a bracketed paste while a modal is active. Returns `true` when the
/// paste was consumed (including modals with no text field).
pub fn handle_paste(overlay: &mut Overlay, text: &str) -> bool {
    let pasted = single_line_paste(text);
    match overlay {
        Overlay::None => false,
        Overlay::Picker(picker) => {
            picker.filter.push_str(&pasted);
            picker.selected = 0;
            true
        }
        Overlay::Permission(_)
        | Overlay::Help
        | Overlay::ShellCommand(_)
        | Overlay::WhichKey
        | Overlay::Login(_)
        | Overlay::Confirm(_) => true,
        Overlay::Question(prompt) => {
            if prompt
                .current_question()
                .is_some_and(|question| question.allow_custom)
            {
                prompt.custom_mode = true;
                prompt.picks[prompt.current].clear();
                prompt.custom_texts[prompt.current] = None;
                prompt.custom_input.push_str(&pasted);
            }
            true
        }
        Overlay::McpList(state) => {
            state.filter.push_str(&pasted);
            state.selected = 0;
            true
        }
        Overlay::McpExplorer(state) => {
            if state.args_mode {
                state.args_input.push_str(&pasted);
            } else {
                state.filter.push_str(&pasted);
                state.selected = 0;
            }
            true
        }
        Overlay::McpInstall(state) => {
            if state.input_mode {
                state.input.push_str(&pasted);
            } else {
                state.filter.push_str(&pasted);
                state.selected = 0;
            }
            true
        }
        Overlay::CommandPalette(state) => {
            state.filter.push_str(&pasted);
            state.selected = 0;
            true
        }
        Overlay::ConnectWizard(state) => {
            match state.step {
                ConnectWizardStep::PickProvider => {
                    state.filter.push_str(&pasted);
                    state.selected = 0;
                }
                ConnectWizardStep::CustomProviderId
                | ConnectWizardStep::BaseUrl
                | ConnectWizardStep::ApiKey
                | ConnectWizardStep::Model => {
                    state.input.push_str(&pasted);
                }
                ConnectWizardStep::PickAuthMethod | ConnectWizardStep::OAuthWaiting => {}
            }
            true
        }
    }
}

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
        Overlay::CommandPalette(state) => Some(command_palette_key(state, key)),
        Overlay::WhichKey => Some(which_key_key(key)),
        Overlay::ConnectWizard(state) => Some(connect_wizard_key(state, key)),
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
    SetTheme(String),
    SetEffort(String),
    ResumeSession(String),
}

/// What the install wizard selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpInstallChoice {
    Registry { id: String },
    Npm { package: String },
    Import { path: String },
}

/// The id under the picker cursor, for live theme preview. `None` unless the
/// picker is a theme picker with a selectable row highlighted.
fn picker_theme_preview(picker: &PickerState) -> Option<String> {
    if picker.action != PickerAction::SetTheme {
        return None;
    }
    picker
        .visible()
        .get(picker.selected)
        .filter(|item| item.enabled)
        .map(|item| item.id.clone())
}

/// A navigation outcome that also carries a live theme preview when relevant.
fn picker_moved(picker: &PickerState) -> OverlayOutcome {
    OverlayOutcome {
        preview_theme: picker_theme_preview(picker),
        ..OverlayOutcome::default()
    }
}

fn advance_picker_selection(picker: &mut PickerState, delta: isize) {
    let visible = picker.visible();
    let selectable: Vec<usize> = visible
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| item.enabled.then_some(idx))
        .collect();
    if selectable.is_empty() {
        picker.selected = 0;
        return;
    }
    let current_pos = selectable
        .iter()
        .position(|&idx| idx == picker.selected)
        .unwrap_or(0);
    let next_pos = if delta < 0 {
        if current_pos == 0 {
            selectable.len() - 1
        } else {
            current_pos - 1
        }
    } else {
        (current_pos + 1) % selectable.len()
    };
    picker.selected = selectable[next_pos];
}

fn picker_key(picker: &mut PickerState, key: KeyEvent) -> OverlayOutcome {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => OverlayOutcome {
            close: true,
            revert_theme: picker.action == PickerAction::SetTheme,
            ..OverlayOutcome::default()
        },
        (KeyCode::Up, _) => {
            advance_picker_selection(picker, -1);
            picker_moved(picker)
        }
        (KeyCode::Down, _) => {
            advance_picker_selection(picker, 1);
            picker_moved(picker)
        }
        (KeyCode::Backspace, _) => {
            picker.filter.pop();
            picker.selected = 0;
            picker_moved(picker)
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
                        PickerAction::SetTheme => PickerChoice::SetTheme(id),
                        PickerAction::SetEffort => PickerChoice::SetEffort(id),
                        PickerAction::ResumeSession => PickerChoice::ResumeSession(id),
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
            picker_moved(picker)
        }
        _ => OverlayOutcome::consumed(),
    }
}

fn permission_key(prompt: &mut PermissionPrompt, key: KeyEvent) -> OverlayOutcome {
    let deny = |prompt: &PermissionPrompt| OverlayOutcome {
        effects: vec![Effect::RespondPermission {
            id: prompt.id.clone(),
            decision: PermissionDecision::Deny { reason: None },
            session: prompt.session.clone(),
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
                    session: prompt.session.clone(),
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
                    session: prompt.session.clone(),
                }],
                close: true,
                ..OverlayOutcome::default()
            }
        }
        // Digits select-and-confirm the numbered option (1-based).
        (KeyCode::Char(digit @ '1'..='9'), KeyModifiers::NONE) => {
            let index = digit as usize - '1' as usize;
            let Some(kind) = prompt.options.get(index) else {
                return OverlayOutcome::consumed();
            };
            OverlayOutcome {
                effects: vec![Effect::RespondPermission {
                    id: prompt.id.clone(),
                    decision: decision_for(*kind),
                    session: prompt.session.clone(),
                }],
                close: true,
                ..OverlayOutcome::default()
            }
        }
        // Esc never leaves a request dangling while the turn blocks on it.
        (KeyCode::Esc, _) | (KeyCode::Char('n'), KeyModifiers::NONE) => deny(prompt),
        (KeyCode::Char('d'), KeyModifiers::NONE) if prompt.diff.is_some() => {
            prompt.diff_expanded = !prompt.diff_expanded;
            OverlayOutcome::consumed()
        }
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
                    session: prompt.session.clone(),
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
    let filter = state.filter.trim();
    if filter.is_empty() {
        return state.items.iter().collect();
    }
    let ranked = fuzzy_rank(
        filter,
        state
            .items
            .iter()
            .enumerate()
            .map(|(idx, item)| (idx, item.name.as_str())),
        |primary| {
            state
                .items
                .iter()
                .find(|item| item.name == primary)
                .map(|item| vec![item.source.as_str()])
                .unwrap_or_default()
        },
    );
    ranked
        .into_iter()
        .filter_map(|idx| state.items.get(idx))
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
    let filter = state.filter.trim();
    if filter.is_empty() {
        return tools.iter().collect();
    }
    let ranked = fuzzy_rank(
        filter,
        tools
            .iter()
            .enumerate()
            .map(|(idx, tool)| (idx, tool.name.as_str())),
        |primary| {
            tools
                .iter()
                .find(|tool| tool.name == primary)
                .map(|tool| vec![tool.description.as_str()])
                .unwrap_or_default()
        },
    );
    ranked
        .into_iter()
        .filter_map(|idx| tools.get(idx))
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

fn advance_palette_selection(state: &mut CommandPaletteState, delta: isize) {
    let visible = state.visible();
    if visible.is_empty() {
        state.selected = 0;
        return;
    }
    let mut idx = state.selected;
    for _ in 0..visible.len() {
        idx = if delta < 0 {
            if idx == 0 { visible.len() - 1 } else { idx - 1 }
        } else {
            (idx + 1) % visible.len()
        };
        if visible[idx].section_header.is_none() {
            state.selected = idx;
            return;
        }
    }
}

fn command_palette_key(state: &mut CommandPaletteState, key: KeyEvent) -> OverlayOutcome {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => OverlayOutcome::close(),
        (KeyCode::Up, _) => {
            advance_palette_selection(state, -1);
            OverlayOutcome::consumed()
        }
        (KeyCode::Down, _) => {
            advance_palette_selection(state, 1);
            OverlayOutcome::consumed()
        }
        (KeyCode::Backspace, _) => {
            state.filter.pop();
            state.selected = 0;
            OverlayOutcome::consumed()
        }
        (KeyCode::Enter, _) => {
            let action = state
                .visible()
                .get(state.selected)
                .filter(|entry| entry.section_header.is_none())
                .map(|entry| entry.action.clone());
            match action {
                Some(action) => OverlayOutcome {
                    palette_action: Some(action),
                    close: true,
                    ..OverlayOutcome::default()
                },
                None => OverlayOutcome::consumed(),
            }
        }
        (KeyCode::Char(c), m) if m.is_empty() || m == KeyModifiers::SHIFT => {
            state.filter.push(c);
            state.selected = 0;
            OverlayOutcome::consumed()
        }
        _ => OverlayOutcome::consumed(),
    }
}

fn which_key_key(key: KeyEvent) -> OverlayOutcome {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) | (KeyCode::Char('?'), _) => OverlayOutcome::close(),
        _ => OverlayOutcome::consumed(),
    }
}

fn connect_wizard_key(state: &mut ConnectWizardState, key: KeyEvent) -> OverlayOutcome {
    match state.step {
        ConnectWizardStep::PickProvider => connect_gallery_key(state, key),
        ConnectWizardStep::PickAuthMethod => connect_auth_method_key(state, key),
        ConnectWizardStep::OAuthWaiting => connect_oauth_waiting_key(state, key),
        _ => connect_form_key(state, key),
    }
}

fn connect_gallery_key(state: &mut ConnectWizardState, key: KeyEvent) -> OverlayOutcome {
    let rows = connect_gallery_rows(state);
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => OverlayOutcome::close(),
        (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
            advance_gallery_selection(state, &rows, -1);
            OverlayOutcome::consumed()
        }
        (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::CONTROL) => {
            advance_gallery_selection(state, &rows, 1);
            OverlayOutcome::consumed()
        }
        (KeyCode::Enter, _) => {
            let Some(row) = rows.get(state.selected) else {
                return OverlayOutcome::consumed();
            };
            match row {
                ConnectGalleryRow::Header(_) => OverlayOutcome::consumed(),
                ConnectGalleryRow::Template(id) => {
                    apply_connect_template(state, id);
                    connect_after_template_pick(state)
                }
                ConnectGalleryRow::Custom => {
                    state.step = ConnectWizardStep::CustomProviderId;
                    state.id.clear();
                    state.input.clear();
                    state.template_id = None;
                    OverlayOutcome::consumed()
                }
            }
        }
        (KeyCode::Backspace, _) => {
            state.filter.pop();
            state.selected = 0;
            OverlayOutcome::consumed()
        }
        (KeyCode::Char(c), m) if m.is_empty() || m == KeyModifiers::SHIFT => {
            state.filter.push(c);
            state.selected = 0;
            OverlayOutcome::consumed()
        }
        _ => OverlayOutcome::consumed(),
    }
}

fn connect_after_template_pick(state: &mut ConnectWizardState) -> OverlayOutcome {
    let Some(template) = state
        .template_id
        .as_deref()
        .and_then(agentloop_cli_core::provider_template)
    else {
        return OverlayOutcome::consumed();
    };
    let methods = agentloop_cli_core::auth_methods(template);
    state.auth_methods = methods.to_vec();
    if methods.len() > 1 {
        state.step = ConnectWizardStep::PickAuthMethod;
        state.selected = 0;
        return OverlayOutcome::consumed();
    }
    if let Some(spec) = methods.first() {
        return connect_after_auth_method_pick(state, spec.kind);
    }
    match template.auth {
        agentloop_cli_core::ProviderAuth::DeviceFlow => OverlayOutcome {
            close: true,
            start_copilot_login: true,
            ..OverlayOutcome::default()
        },
        agentloop_cli_core::ProviderAuth::EnvOnly { env_var } => OverlayOutcome {
            close: true,
            info: Some(format!("set {env_var} then run /provider {}", template.id)),
            ..OverlayOutcome::default()
        },
        agentloop_cli_core::ProviderAuth::ApiKey { env_var } => {
            if let Some(var) = env_var.filter(|v| agentloop_cli_core::env_var_configured(v)) {
                state.api_key = format!("{{env:{var}}}");
                state.step = ConnectWizardStep::Model;
                state.input = state.model.clone();
            } else {
                state.step = ConnectWizardStep::ApiKey;
                state.input.clear();
            }
            OverlayOutcome::consumed()
        }
        agentloop_cli_core::ProviderAuth::MultiMethod => OverlayOutcome::consumed(),
    }
}

fn connect_after_auth_method_pick(
    state: &mut ConnectWizardState,
    kind: agentloop_cli_core::AuthMethodKind,
) -> OverlayOutcome {
    use agentloop_cli_core::AuthMethodKind;
    use agentloop_cli_core::OpenAiOAuthMethod;
    match kind {
        AuthMethodKind::DeviceFlow => OverlayOutcome {
            close: true,
            start_copilot_login: true,
            ..OverlayOutcome::default()
        },
        AuthMethodKind::OAuthBrowser => {
            state.auth_method_label = state
                .auth_methods
                .iter()
                .find(|m| m.kind == AuthMethodKind::OAuthBrowser)
                .map(|m| m.label.to_owned());
            state.step = ConnectWizardStep::OAuthWaiting;
            state.oauth_waiting = false;
            state.oauth_url = None;
            state.oauth_instructions = None;
            state.pending_oauth_method = Some(OpenAiOAuthMethod::Browser);
            OverlayOutcome {
                start_openai_oauth: Some(OpenAiOAuthMethod::Browser),
                ..OverlayOutcome::default()
            }
        }
        AuthMethodKind::OAuthHeadless => {
            state.auth_method_label = state
                .auth_methods
                .iter()
                .find(|m| m.kind == AuthMethodKind::OAuthHeadless)
                .map(|m| m.label.to_owned());
            state.step = ConnectWizardStep::OAuthWaiting;
            state.oauth_waiting = false;
            state.oauth_url = None;
            state.oauth_instructions = None;
            state.pending_oauth_method = Some(OpenAiOAuthMethod::Headless);
            OverlayOutcome {
                start_openai_oauth: Some(OpenAiOAuthMethod::Headless),
                ..OverlayOutcome::default()
            }
        }
        AuthMethodKind::ApiKey => {
            if let Some(var) = state.template_id.as_deref().and_then(|id| {
                agentloop_cli_core::provider_template(id).and_then(|t| match t.auth {
                    agentloop_cli_core::ProviderAuth::ApiKey { env_var } => env_var,
                    agentloop_cli_core::ProviderAuth::MultiMethod if id == "openai" => {
                        Some("OPENAI_API_KEY")
                    }
                    _ => None,
                })
            }) && agentloop_cli_core::env_var_configured(var)
            {
                state.api_key = format!("{{env:{var}}}");
                state.step = ConnectWizardStep::Model;
                state.input = state.model.clone();
            } else {
                state.step = ConnectWizardStep::ApiKey;
                state.input.clear();
            }
            OverlayOutcome::consumed()
        }
    }
}

fn connect_auth_method_key(state: &mut ConnectWizardState, key: KeyEvent) -> OverlayOutcome {
    let count = state.auth_methods.len();
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => OverlayOutcome::close(),
        (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
            if state.selected > 0 {
                state.selected -= 1;
            } else if count > 0 {
                state.selected = count - 1;
            }
            OverlayOutcome::consumed()
        }
        (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::CONTROL) => {
            if count > 0 {
                state.selected = (state.selected + 1) % count;
            }
            OverlayOutcome::consumed()
        }
        (KeyCode::Enter, _) => {
            let Some(spec) = state.auth_methods.get(state.selected) else {
                return OverlayOutcome::consumed();
            };
            connect_after_auth_method_pick(state, spec.kind)
        }
        _ => OverlayOutcome::consumed(),
    }
}

fn connect_oauth_waiting_key(state: &mut ConnectWizardState, key: KeyEvent) -> OverlayOutcome {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => OverlayOutcome {
            close: true,
            effects: vec![Effect::CancelOpenAiOAuth],
            ..OverlayOutcome::default()
        },
        (KeyCode::Char('c'), _) => {
            if let Some(url) = state.oauth_url.clone() {
                OverlayOutcome {
                    effects: vec![Effect::CopyToClipboard { text: url }],
                    ..OverlayOutcome::default()
                }
            } else {
                OverlayOutcome::consumed()
            }
        }
        _ => OverlayOutcome::consumed(),
    }
}

fn apply_connect_template(state: &mut ConnectWizardState, id: &str) {
    state.id = id.to_owned();
    state.template_id = Some(id.to_owned());
    if let Some(template) = agentloop_cli_core::provider_template(id) {
        state.base_url = template.base_url.unwrap_or_default().to_owned();
        state.model = template.default_model.unwrap_or_default().to_owned();
    }
}

fn connect_form_key(state: &mut ConnectWizardState, key: KeyEvent) -> OverlayOutcome {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => OverlayOutcome::close(),
        (KeyCode::Backspace, _) => {
            state.input.pop();
            OverlayOutcome::consumed()
        }
        (KeyCode::Enter, _) => {
            let value = state.input.trim().to_owned();
            if value.is_empty() {
                return OverlayOutcome::consumed();
            }
            match state.step {
                ConnectWizardStep::CustomProviderId => {
                    state.id = value.to_lowercase();
                    state.template_id = None;
                    if let Some((base_url, default_model, _)) =
                        agentloop_cli_core::known_provider_defaults(&state.id)
                    {
                        state.base_url = base_url.to_owned();
                        state.model = default_model.to_owned();
                        state.template_id = Some(state.id.clone());
                        state.step = ConnectWizardStep::ApiKey;
                    } else {
                        state.step = ConnectWizardStep::BaseUrl;
                    }
                    state.input.clear();
                    OverlayOutcome::consumed()
                }
                ConnectWizardStep::BaseUrl => {
                    state.base_url = value;
                    state.step = ConnectWizardStep::ApiKey;
                    state.input.clear();
                    OverlayOutcome::consumed()
                }
                ConnectWizardStep::ApiKey => {
                    state.api_key = value;
                    state.step = ConnectWizardStep::Model;
                    state.input = state.model.clone();
                    OverlayOutcome::consumed()
                }
                ConnectWizardStep::Model => {
                    state.model = value;
                    let config = agentloop_cli_core::ProviderConfig {
                        name: None,
                        base_url: state.base_url.clone(),
                        api_key: state.api_key.clone(),
                        models: Vec::new(),
                        default_model: Some(state.model.clone()),
                        thinking: state
                            .template_id
                            .as_deref()
                            .and_then(agentloop_cli_core::known_provider_defaults)
                            .map(|(_, _, thinking)| thinking)
                            .unwrap_or(false),
                    };
                    OverlayOutcome {
                        connect_wizard: Some((state.id.clone(), config)),
                        close: true,
                        ..OverlayOutcome::default()
                    }
                }
                ConnectWizardStep::PickProvider
                | ConnectWizardStep::PickAuthMethod
                | ConnectWizardStep::OAuthWaiting => OverlayOutcome::consumed(),
            }
        }
        (KeyCode::Char(c), m) if m.is_empty() || m == KeyModifiers::SHIFT => {
            state.input.push(c);
            OverlayOutcome::consumed()
        }
        _ => OverlayOutcome::consumed(),
    }
}

fn advance_gallery_selection(
    state: &mut ConnectWizardState,
    rows: &[ConnectGalleryRow],
    delta: isize,
) {
    if rows.is_empty() {
        state.selected = 0;
        return;
    }
    let mut idx = state.selected;
    for _ in 0..rows.len() {
        idx = if delta < 0 {
            if idx == 0 { rows.len() - 1 } else { idx - 1 }
        } else {
            (idx + 1) % rows.len()
        };
        if !matches!(rows[idx], ConnectGalleryRow::Header(_)) {
            state.selected = idx;
            return;
        }
    }
}

pub(crate) enum ConnectGalleryRow {
    Header(&'static str),
    Template(&'static str),
    Custom,
}

fn connect_gallery_rows(state: &ConnectWizardState) -> Vec<ConnectGalleryRow> {
    let filter = state.filter.trim();
    let mut rows = Vec::new();
    let mut last_category: Option<agentloop_cli_core::ProviderCategory> = None;
    for template in agentloop_cli_core::provider_templates() {
        if !filter.is_empty() {
            let haystack = format!(
                "{} {} {}",
                template.id, template.label, template.description
            );
            if fuzzy_score(filter, &haystack).is_none() {
                continue;
            }
        }
        if last_category != Some(template.category) {
            rows.push(ConnectGalleryRow::Header(template.category.label()));
            last_category = Some(template.category);
        }
        rows.push(ConnectGalleryRow::Template(template.id));
    }
    if filter.is_empty() || fuzzy_score(filter, "custom provider").is_some() {
        rows.push(ConnectGalleryRow::Header(
            agentloop_cli_core::ProviderCategory::Custom.label(),
        ));
        rows.push(ConnectGalleryRow::Custom);
    }
    rows
}

impl ConnectWizardState {
    /// Empty gallery wizard.
    pub fn new_gallery() -> Self {
        Self {
            step: ConnectWizardStep::PickProvider,
            filter: String::new(),
            selected: 0,
            id: String::new(),
            base_url: String::new(),
            api_key: String::new(),
            model: String::new(),
            input: String::new(),
            template_id: None,
            auth_methods: Vec::new(),
            auth_method_label: None,
            oauth_url: None,
            oauth_instructions: None,
            oauth_waiting: false,
            pending_oauth_method: None,
        }
    }

    /// Gallery rows for rendering and keyboard navigation.
    pub(crate) fn gallery_rows(&self) -> Vec<ConnectGalleryRow> {
        connect_gallery_rows(self)
    }

    /// Clamp selection after the visible row list changes.
    pub fn clamp_gallery_selection(&mut self) {
        let len = connect_gallery_rows(self).len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
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
    fn permission_answer_carries_prompt_session() {
        use agentloop_contracts::{PermissionDecisionKind, PermissionRequestId};
        let mut prompt = PermissionPrompt {
            id: PermissionRequestId::from("perm-1"),
            call_id: None,
            title: "Allow `Bash`?".to_owned(),
            detail: None,
            options: vec![PermissionDecisionKind::AllowOnce],
            selected: 0,
            session: Some(SessionId::from("child-1")),
            role: Some("worker".to_owned()),
            diff: None,
            diff_expanded: false,
        };
        let outcome = permission_key(&mut prompt, key(KeyCode::Enter));
        assert!(outcome.close);
        let Effect::RespondPermission { session, .. } = &outcome.effects[0] else {
            panic!("expected RespondPermission");
        };
        assert_eq!(session.as_ref().map(SessionId::as_str), Some("child-1"));
    }

    #[test]
    fn question_answer_carries_prompt_session() {
        let mut prompt = sample_prompt(false);
        prompt.session = Some(SessionId::from("child-2"));
        question_key(&mut prompt, key(KeyCode::Char('1')));
        let outcome = question_key(&mut prompt, key(KeyCode::Enter));
        let Effect::RespondQuestion { session, .. } = &outcome.effects[0] else {
            panic!("expected RespondQuestion");
        };
        assert_eq!(session.as_ref().map(SessionId::as_str), Some("child-2"));
    }

    #[test]
    fn toggle_pick_adds_and_removes() {
        let mut picks = Vec::new();
        toggle_pick(&mut picks, 2);
        assert_eq!(picks, vec![2]);
        toggle_pick(&mut picks, 2);
        assert!(picks.is_empty());
    }

    #[test]
    fn fuzzy_score_prefers_substring_over_subsequence() {
        let sub = fuzzy_score("anth", "anthropic/claude").expect("match");
        let seq = fuzzy_score("anth", "a nice theory helps").expect("match");
        assert!(sub > seq);
    }

    #[test]
    fn fuzzy_score_ranks_closer_matches_higher() {
        let anth = fuzzy_score("anth", "anthropic").expect("match");
        let ant = fuzzy_score("ant", "anthropic").expect("match");
        assert!(anth >= ant);
    }

    #[test]
    fn fuzzy_score_rejects_unrelated_strings() {
        assert!(fuzzy_score("xyz", "anthropic").is_none());
    }

    #[test]
    fn connect_wizard_api_key_paste_appends_to_wizard_input() {
        let mut overlay = Overlay::ConnectWizard(ConnectWizardState::new_gallery());
        if let Overlay::ConnectWizard(state) = &mut overlay {
            state.step = ConnectWizardStep::ApiKey;
            state.id = "openai".to_owned();
            state.base_url = "https://api.openai.com/v1".to_owned();
            state.template_id = Some("openai".to_owned());
        }
        assert!(handle_paste(&mut overlay, "sk-test-key\n"));
        let Overlay::ConnectWizard(state) = overlay else {
            panic!("expected connect wizard");
        };
        assert_eq!(state.input, "sk-test-key");
    }

    #[test]
    fn connect_wizard_gallery_paste_appends_to_filter() {
        let mut overlay = Overlay::ConnectWizard(ConnectWizardState::new_gallery());
        assert!(handle_paste(&mut overlay, "deep"));
        let Overlay::ConnectWizard(state) = overlay else {
            panic!("expected connect wizard");
        };
        assert_eq!(state.filter, "deep");
    }

    #[test]
    fn login_overlay_paste_is_consumed_without_text_field() {
        let mut overlay = Overlay::Login(LoginState::Starting);
        assert!(handle_paste(&mut overlay, "should-not-appear"));
        assert!(matches!(overlay, Overlay::Login(LoginState::Starting)));
    }
}
