//! The application state and its Elm-style reducer.
//!
//! [`App::update`] is pure with respect to I/O: it mutates state and returns
//! [`Effect`]s for the runtime to execute. Key routing precedence:
//! Ctrl+C/Ctrl+D → active overlay → input popup → global chords → editor.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use agentloop_cli_core::{AgentKind, CatalogEntry, LoginEvent, has_copilot_credentials};
use agentloop_contracts::{
    AgentCaps, AgentEvent, ModelDiscovery, ModelInfo, ModelRef, PermissionMode, PromptInput,
    SessionEvent, SessionId, TokenUsage, TurnOptions, TurnStopReason,
};

use crate::chat::ChatState;
use crate::commands::{CommandIndex, LocalCommand, Route};
use crate::events::{AppEvent, Effect, SessionBootstrap, ShellCommandOutcome, TaskResult};
use crate::files::FileIndex;
use crate::input::{InputOutcome, InputState};
use crate::overlay::{
    self, ConfirmAction, ConfirmPrompt, LoginState, Overlay, OverlayOutcome, PermissionPrompt,
    PickerAction, PickerChoice, PickerItem, PickerState, QuestionPrompt, ShellCommandOverlay,
    ShellCommandPhase,
};
use crate::ui::MarkdownCache;

/// Second Ctrl+C within this window quits.
const QUIT_WINDOW: Duration = Duration::from_millis(1500);
/// PageUp/PageDown scroll step in wrapped lines.
const SCROLL_STEP: usize = 10;
/// Arrow-key scroll step when the prompt is empty.
const ARROW_SCROLL_STEP: usize = 3;

/// Whether a turn is in flight.
#[derive(Debug, Clone, Copy)]
pub enum TurnPhase {
    Idle,
    Running { started: Instant },
}

impl TurnPhase {
    /// Whether a turn is currently running.
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }
}

/// Code runs tools normally; Plan forces read-only research mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionMode {
    #[default]
    Code,
    Plan,
}

/// The live session's identity and per-turn preferences.
#[derive(Debug)]
pub struct SessionState {
    pub id: SessionId,
    /// Current model selection; sent as `TurnOptions.model` on every prompt.
    pub model: Option<ModelRef>,
    pub turn: TurnPhase,
    /// Highest event sequence seen (resync anchor).
    pub last_seq: u64,
    /// Security level when `session_mode` is Code.
    pub permission_mode: PermissionMode,
    /// Session mode: code (normal) or plan (research-only).
    pub session_mode: SessionMode,
}

impl SessionState {
    /// Effective permission mode sent on each turn.
    pub fn effective_permission_mode(&self) -> PermissionMode {
        if self.session_mode == SessionMode::Plan {
            PermissionMode::Plan
        } else {
            self.permission_mode
        }
    }
}

/// Short label for the status bar.
pub fn session_mode_label(mode: SessionMode) -> &'static str {
    match mode {
        SessionMode::Code => "code",
        SessionMode::Plan => "plan",
    }
}

/// Short label for the status bar.
pub fn permission_mode_label(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::Default => "req",
        PermissionMode::AcceptEdits => "auto",
        PermissionMode::BypassPermissions => "all",
        PermissionMode::Plan => "plan",
        PermissionMode::DontAsk => "dont-ask",
        _ => "unknown",
    }
}

/// Status-bar state.
#[derive(Debug, Default)]
pub struct StatusState {
    /// Accumulated from `TurnCompleted` summaries.
    pub total_usage: TokenUsage,
    pub last_cost_usd: Option<f64>,
    /// One-line error, cleared on the next keypress.
    pub last_error: Option<String>,
    /// Transient notice ("press ctrl+c again to exit").
    pub notice: Option<String>,
    /// Spinner animation counter, advanced by ticks while busy.
    pub spinner: usize,
}

/// What to resume after a contextual Copilot sign-in completes.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingCopilotAuth {
    SwitchAgent(AgentKind),
    ApplyProvider(String),
    SetModel(ModelRef),
}

/// All application state; one instance per process.
pub struct App {
    pub kind: AgentKind,
    pub caps: AgentCaps,
    /// Engine identity from the handshake, for the welcome line.
    pub engine_name: String,
    pub engine_version: String,
    pub session: SessionState,
    pub chat: ChatState,
    pub input: InputState,
    pub commands: CommandIndex,
    pub overlay: Overlay,
    pub pending_permissions: VecDeque<PermissionPrompt>,
    pub pending_questions: VecDeque<QuestionPrompt>,
    /// Registered provider ids (empty for delegated agents).
    pub providers: Vec<String>,
    /// Cached model catalog for pickers and provider defaults.
    pub catalog: Vec<CatalogEntry>,
    /// A provider whose default model is applied once the catalog arrives.
    pending_provider: Option<String>,
    /// Open the model picker when the catalog arrives.
    awaiting_model_picker: bool,
    /// Copilot selection deferred until device-flow sign-in finishes.
    pending_copilot_auth: Option<PendingCopilotAuth>,
    /// Model to apply after rebuilding native with fresh provider registry.
    pending_model: Option<ModelRef>,
    pub status: StatusState,
    /// Parsed-markdown cache for assistant blocks (keyed on `ChatItem::rev`).
    pub markdown_cache: MarkdownCache,
    /// When true, the TUI captures the mouse (wheel scroll); when false, the
    /// terminal can select text with click-drag.
    pub mouse_capture: bool,
    /// User preference: show reasoning blocks when the agent exposes them.
    pub show_thinking: bool,
    pub should_quit: bool,
    /// Session working directory (`--workdir`); scopes `@` file search.
    pub workdir: PathBuf,
    /// Indexed files under [`Self::workdir`] for `@` mention autocomplete.
    pub file_index: FileIndex,
    dirty: bool,
    quit_armed_at: Option<Instant>,
}

impl App {
    /// Build the app around the initial session.
    pub fn new(bootstrap: SessionBootstrap, workdir: PathBuf, file_index: FileIndex) -> Self {
        let mut app = Self {
            kind: bootstrap.kind,
            caps: AgentCaps::default(),
            engine_name: bootstrap.hello.engine.name.clone(),
            engine_version: bootstrap.hello.engine.version.clone(),
            session: SessionState {
                id: SessionId(String::new()),
                model: None,
                turn: TurnPhase::Idle,
                last_seq: 0,
                permission_mode: PermissionMode::AcceptEdits,
                session_mode: SessionMode::Code,
            },
            chat: ChatState::default(),
            input: InputState::default(),
            commands: CommandIndex::default(),
            overlay: Overlay::None,
            pending_permissions: VecDeque::new(),
            pending_questions: VecDeque::new(),
            providers: Vec::new(),
            catalog: Vec::new(),
            pending_provider: None,
            awaiting_model_picker: false,
            pending_copilot_auth: None,
            pending_model: None,
            status: StatusState::default(),
            markdown_cache: MarkdownCache::default(),
            mouse_capture: false,
            show_thinking: bootstrap.hello.capabilities.reasoning_visible,
            should_quit: false,
            workdir,
            file_index,
            dirty: true,
            quit_armed_at: None,
        };
        let welcome = format!(
            "{} {} — /help for keys and commands",
            app.engine_name, app.engine_version
        );
        app.chat.push_info(welcome);
        app.install_bootstrap(bootstrap, false);
        app
    }

    /// Whether a redraw is needed. Cleared by [`Self::clear_dirty`] after a
    /// draw so a deferred frame is never lost.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the current frame as drawn.
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Whether reasoning/thinking blocks should render in the chat transcript.
    pub fn thinking_visible(&self) -> bool {
        self.caps.reasoning_visible && self.show_thinking
    }

    /// The single reducer entry point.
    pub fn update(&mut self, event: AppEvent) -> Vec<Effect> {
        match event {
            AppEvent::Term(term) => {
                self.dirty = true;
                self.on_term(term)
            }
            AppEvent::Engine(event) => {
                self.dirty = true;
                self.on_engine(*event)
            }
            AppEvent::Task(result) => {
                self.dirty = true;
                self.on_task(result)
            }
            AppEvent::Login(event) => {
                self.dirty = true;
                self.on_login(event);
                Vec::new()
            }
            AppEvent::Tick => {
                self.on_tick();
                Vec::new()
            }
            AppEvent::Interrupt => {
                self.dirty = true;
                self.on_ctrl_c()
            }
        }
    }

    // ── terminal input ──────────────────────────────────────────────────────

    fn on_term(&mut self, event: TermEvent) -> Vec<Effect> {
        use crossterm::event::MouseEventKind;
        match event {
            TermEvent::Key(key) if key.kind != KeyEventKind::Release => self.on_key(key),
            TermEvent::Mouse(mouse) if self.mouse_capture => match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.chat.scroll.page_up(ARROW_SCROLL_STEP);
                    Vec::new()
                }
                MouseEventKind::ScrollDown => {
                    self.chat.scroll.page_down(ARROW_SCROLL_STEP);
                    Vec::new()
                }
                _ => Vec::new(),
            },
            TermEvent::Paste(text) => {
                self.input.paste(&text);
                self.input.refresh_popup(&self.commands, &self.file_index);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn on_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        self.status.last_error = None;

        // Copy transcript before generic Ctrl/Cmd+C handling.
        if matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && key.modifiers.contains(KeyModifiers::SHIFT)
        {
            return self.copy_chat();
        }

        // Expand/collapse the latest thinking block (Shift+Ctrl+T).
        if matches!(key.code, KeyCode::Char('t') | KeyCode::Char('T'))
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && key.modifiers.contains(KeyModifiers::SHIFT)
        {
            if self.thinking_visible() {
                self.chat.toggle_last_thinking();
            }
            return Vec::new();
        }

        // Toggle mouse capture (wheel scroll vs native text selection).
        if matches!(key.code, KeyCode::Char('m') | KeyCode::Char('M'))
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::SHIFT)
        {
            return self.toggle_mouse_capture();
        }

        // Global: Ctrl/Cmd+C (cancel / quit), Ctrl+D (quit on empty input).
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::META)
        {
            match key.code {
                KeyCode::Char('c') | KeyCode::Char('C') => return self.on_ctrl_c(),
                KeyCode::Char('d') | KeyCode::Char('D')
                    if key.modifiers.contains(KeyModifiers::CONTROL) && self.input.is_empty() =>
                {
                    self.should_quit = true;
                    return vec![Effect::Quit];
                }
                _ => {}
            }
        }

        // Active modal consumes everything else.
        if self.overlay.is_active() {
            if let Some(outcome) = overlay::handle_key(&mut self.overlay, key) {
                return self.apply_overlay_outcome(outcome);
            }
        }

        let popup_open = self.input.popup.is_some();

        // Global chords that don't collide with the popup.
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) if !popup_open => {
                if self.session.turn.is_running() {
                    self.status.notice = Some("interrupting…".to_owned());
                    return vec![Effect::CancelTurn];
                }
                return Vec::new();
            }
            (KeyCode::PageUp, _) => {
                self.chat.scroll.page_up(SCROLL_STEP);
                return Vec::new();
            }
            (KeyCode::PageDown, _) => {
                self.chat.scroll.page_down(SCROLL_STEP);
                return Vec::new();
            }
            (KeyCode::Up, KeyModifiers::NONE) if self.input.is_empty() => {
                self.chat.scroll.page_up(ARROW_SCROLL_STEP);
                return Vec::new();
            }
            (KeyCode::Down, KeyModifiers::NONE) if self.input.is_empty() => {
                self.chat.scroll.page_down(ARROW_SCROLL_STEP);
                return Vec::new();
            }
            (KeyCode::End, _) if self.input.is_empty() => {
                self.chat.scroll.scroll_to_bottom();
                return Vec::new();
            }
            (KeyCode::Char('t'), KeyModifiers::CONTROL) => {
                return self.toggle_thinking();
            }
            (KeyCode::Tab, modifiers) if self.input.is_empty() => {
                if self
                    .chat
                    .cycle_tool_focus(modifiers.contains(KeyModifiers::SHIFT))
                {
                    return Vec::new();
                }
            }
            (KeyCode::Enter, KeyModifiers::NONE) | (KeyCode::Char(' '), KeyModifiers::NONE)
                if self.input.is_empty() && self.chat.toggle_focused_tool_expand() =>
            {
                return Vec::new();
            }
            _ => {}
        }

        // Editor (handles popup navigation internally).
        match self.input.handle_key(key, &self.commands, &self.file_index) {
            InputOutcome::Submitted(line) => self.on_submit(&line),
            InputOutcome::Consumed | InputOutcome::Ignored => Vec::new(),
        }
    }

    fn on_ctrl_c(&mut self) -> Vec<Effect> {
        if self.session.turn.is_running() {
            self.status.notice = Some("interrupting…".to_owned());
            return vec![Effect::CancelTurn];
        }
        if matches!(self.overlay, Overlay::Login(_)) {
            self.overlay = Overlay::None;
            self.pending_copilot_auth = None;
            self.chat.push_info("login cancelled");
            self.drain_pending();
            return vec![Effect::CancelLogin];
        }
        if !self.input.is_empty() {
            self.input.set_text("");
            self.quit_armed_at = Some(Instant::now());
            self.status.notice = Some("input cleared — ctrl+c again to exit".to_owned());
            return Vec::new();
        }
        self.should_quit = true;
        vec![Effect::Quit]
    }

    fn apply_overlay_outcome(&mut self, outcome: OverlayOutcome) -> Vec<Effect> {
        let mut effects = outcome.effects;
        if effects
            .iter()
            .any(|effect| matches!(effect, Effect::CancelLogin))
        {
            self.pending_copilot_auth = None;
        }
        if let Some(info) = outcome.info {
            self.chat.push_info(info);
        }
        if let Some(action) = outcome.confirmed {
            self.apply_confirm_action(action);
        }
        if outcome.close {
            self.overlay = Overlay::None;
            self.drain_pending();
        }
        if let Some(choice) = outcome.choice {
            effects.extend(self.apply_picker_choice(choice));
        }
        effects
    }

    fn apply_confirm_action(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::AllowAllPermissions => {
                self.set_permission_mode(PermissionMode::BypassPermissions);
            }
        }
    }

    fn apply_picker_choice(&mut self, choice: PickerChoice) -> Vec<Effect> {
        match choice {
            PickerChoice::SetModel(id) => self.set_model(ModelRef(id)),
            PickerChoice::SwitchProvider(name) => self.apply_provider(&name),
            PickerChoice::SwitchAgent(id) => self.switch_agent(&id),
            PickerChoice::SetSessionMode(id) => {
                self.apply_session_mode_arg(&id);
                Vec::new()
            }
            PickerChoice::SetPermissionMode(id) => {
                self.apply_permission_picker_id(&id);
                Vec::new()
            }
        }
    }

    // ── submission and slash commands ───────────────────────────────────────

    fn on_submit(&mut self, line: &str) -> Vec<Effect> {
        if line.trim().eq_ignore_ascii_case("/login") {
            self.chat.push_info(
                "Copilot sign-in starts automatically when you select copilot via /agent, \
                 /provider, or /model.",
            );
            return Vec::new();
        }
        match self.commands.route(line) {
            Route::Plain | Route::Engine => self.submit_prompt(line),
            Route::Local(command) => self.run_local(command),
        }
    }

    fn submit_prompt(&mut self, line: &str) -> Vec<Effect> {
        if self.session.turn.is_running() {
            self.status.notice = Some("turn in progress — esc to cancel".to_owned());
            return Vec::new();
        }
        self.session.turn = TurnPhase::Running {
            started: Instant::now(),
        };
        self.status.notice = None;
        vec![Effect::SubmitPrompt {
            input: PromptInput::text(line),
            opts: TurnOptions {
                model: self.session.model.clone(),
                permission_mode: Some(self.session.effective_permission_mode()),
                ..TurnOptions::default()
            },
        }]
    }

    fn run_local(&mut self, command: LocalCommand) -> Vec<Effect> {
        match command {
            LocalCommand::Model { arg: Some(model) } => self.set_model(ModelRef(model)),
            LocalCommand::Model { arg: None } => self.open_model_picker(),
            LocalCommand::Provider { arg: Some(name) } => self.apply_provider(&name),
            LocalCommand::Provider { arg: None } => self.open_provider_picker(),
            LocalCommand::Agent { arg: Some(id) } => self.switch_agent(&id),
            LocalCommand::Agent { arg: None } => {
                self.open_agent_picker();
                Vec::new()
            }
            LocalCommand::New => {
                self.status.notice = Some("starting new session…".to_owned());
                vec![Effect::NewSession]
            }
            LocalCommand::Help => {
                self.overlay = Overlay::Help;
                Vec::new()
            }
            LocalCommand::Copy => self.copy_chat(),
            LocalCommand::Command { shell } => self.run_shell_command(&shell),
            LocalCommand::Quit => {
                self.should_quit = true;
                vec![Effect::Quit]
            }
            LocalCommand::Mode { arg } => self.run_mode_command(arg),
            LocalCommand::Permissions { arg } => self.run_permissions_command(arg),
            LocalCommand::Thinking { arg } => self.run_thinking_command(arg),
            LocalCommand::Compact => self.run_compact(),
        }
    }

    fn run_compact(&mut self) -> Vec<Effect> {
        if self.session.turn.is_running() {
            self.status.notice = Some("turn in progress — esc to cancel first".to_owned());
            return Vec::new();
        }
        self.session.turn = TurnPhase::Running {
            started: Instant::now(),
        };
        self.status.notice = Some("compacting session…".to_owned());
        vec![Effect::CompactSession {
            opts: TurnOptions {
                model: self.session.model.clone(),
                permission_mode: Some(self.session.effective_permission_mode()),
                ..TurnOptions::default()
            },
        }]
    }

    fn toggle_thinking(&mut self) -> Vec<Effect> {
        if !self.caps.reasoning_visible {
            self.status.notice = Some("this agent does not expose reasoning".to_owned());
            return Vec::new();
        }
        self.show_thinking = !self.show_thinking;
        if self.show_thinking {
            self.status.notice = Some("thinking visible (ctrl+t to hide)".to_owned());
        } else {
            self.status.notice = Some("thinking hidden (ctrl+t to show)".to_owned());
        }
        Vec::new()
    }

    fn run_thinking_command(&mut self, arg: Option<String>) -> Vec<Effect> {
        if !self.caps.reasoning_visible {
            self.chat
                .push_error("this agent does not expose reasoning output");
            return Vec::new();
        }
        match arg.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            None => self.toggle_thinking(),
            Some("on" | "show") => {
                self.show_thinking = true;
                self.chat.push_info("thinking visible");
                Vec::new()
            }
            Some("off" | "hide") => {
                self.show_thinking = false;
                self.chat.push_info("thinking hidden");
                Vec::new()
            }
            Some(text) => {
                self.chat
                    .push_error(format!("unknown /thinking value `{text}` (use on or off)"));
                Vec::new()
            }
        }
    }

    fn run_mode_command(&mut self, arg: Option<String>) -> Vec<Effect> {
        match arg {
            Some(text) => {
                self.apply_session_mode_arg(&text);
                Vec::new()
            }
            None => {
                self.open_session_mode_picker();
                Vec::new()
            }
        }
    }

    fn run_permissions_command(&mut self, arg: Option<String>) -> Vec<Effect> {
        match arg {
            Some(text) => self.apply_permission_arg(&text),
            None => {
                self.open_permission_mode_picker();
                Vec::new()
            }
        }
    }

    fn apply_session_mode_arg(&mut self, text: &str) {
        match parse_session_mode(text) {
            Some(mode) => self.set_session_mode(mode),
            None => self
                .chat
                .push_error(format!("unknown mode `{text}` (use code or plan)")),
        }
    }

    fn apply_permission_picker_id(&mut self, id: &str) {
        match id {
            "require" => self.set_permission_mode(PermissionMode::Default),
            "auto" => self.set_permission_mode(PermissionMode::AcceptEdits),
            "allow-all" => self.confirm_allow_all(),
            other => {
                let _ = self.apply_permission_arg(other);
            }
        }
    }

    fn apply_permission_arg(&mut self, text: &str) -> Vec<Effect> {
        match parse_permission_arg(text) {
            Some(PermissionMode::BypassPermissions) => {
                self.confirm_allow_all();
                Vec::new()
            }
            Some(mode) => {
                self.set_permission_mode(mode);
                Vec::new()
            }
            None => {
                self.chat.push_error(format!(
                    "unknown level `{text}` (use require, auto, or allow-all)"
                ));
                Vec::new()
            }
        }
    }

    fn set_session_mode(&mut self, mode: SessionMode) {
        self.session.session_mode = mode;
        self.chat
            .push_info(format!("session mode set to {}", session_mode_label(mode)));
    }

    fn set_permission_mode(&mut self, mode: PermissionMode) {
        if !self.caps.permissions.modes.contains(&mode) {
            self.chat.push_error(format!(
                "agent does not support {} mode",
                permission_mode_label(mode)
            ));
            return;
        }
        self.session.permission_mode = mode;
        self.chat.push_info(format!(
            "permissions set to {}",
            permission_mode_label(mode)
        ));
    }

    fn confirm_allow_all(&mut self) {
        if self.session.permission_mode == PermissionMode::BypassPermissions {
            self.chat.push_info("permissions already allow-all");
            return;
        }
        if !self
            .caps
            .permissions
            .modes
            .contains(&PermissionMode::BypassPermissions)
        {
            self.chat
                .push_error("agent does not support allow-all permissions");
            return;
        }
        self.overlay = Overlay::Confirm(ConfirmPrompt {
            title: "Bypass all tool permissions?".to_owned(),
            message: "Mutating tools will run without prompts until you change /permissions."
                .to_owned(),
            action: ConfirmAction::AllowAllPermissions,
        });
    }

    fn open_session_mode_picker(&mut self) {
        let current = self.session.session_mode;
        let items = vec![
            PickerItem {
                id: "code".to_owned(),
                label: "code".to_owned(),
                detail: (current == SessionMode::Code).then(|| "current".to_owned()),
                enabled: true,
            },
            PickerItem {
                id: "plan".to_owned(),
                label: "plan".to_owned(),
                detail: Some("read-only research".to_owned()),
                enabled: true,
            },
        ];
        self.overlay = Overlay::Picker(PickerState::new(
            "session mode",
            items,
            PickerAction::SetSessionMode,
        ));
    }

    fn open_permission_mode_picker(&mut self) {
        let supported = &self.caps.permissions.modes;
        let current = self.session.permission_mode;
        let items = permission_picker_items(supported, current);
        self.overlay = Overlay::Picker(PickerState::new(
            "permissions",
            items,
            PickerAction::SetPermissionMode,
        ));
    }

    fn run_shell_command(&mut self, shell: &str) -> Vec<Effect> {
        let shell = shell.trim();
        if shell.is_empty() {
            self.chat
                .push_error("usage: /command <shell command>".to_owned());
            return Vec::new();
        }
        self.overlay = Overlay::ShellCommand(ShellCommandOverlay::running(shell));
        vec![Effect::RunShellCommand {
            command: shell.to_owned(),
        }]
    }

    fn copy_chat(&mut self) -> Vec<Effect> {
        let text = self.chat.plain_text();
        if text.trim().is_empty() {
            self.status.notice = Some("nothing to copy".to_owned());
            return Vec::new();
        }
        self.status.notice = Some("chat copied to clipboard".to_owned());
        vec![Effect::CopyToClipboard { text }]
    }

    fn toggle_mouse_capture(&mut self) -> Vec<Effect> {
        self.mouse_capture = !self.mouse_capture;
        self.status.notice = Some(if self.mouse_capture {
            "mouse scroll on — drag-select off (Ctrl+M to toggle)".to_owned()
        } else {
            "mouse select on — drag to copy, Ctrl+Shift+C copies all".to_owned()
        });
        vec![Effect::SetMouseCapture(self.mouse_capture)]
    }

    fn set_model(&mut self, model: ModelRef) -> Vec<Effect> {
        if is_copilot_model(&model) && !has_copilot_credentials() {
            return self.start_copilot_login(PendingCopilotAuth::SetModel(model));
        }
        if is_copilot_model(&model)
            && self.kind == AgentKind::Native
            && !self.providers.iter().any(|provider| provider == "copilot")
        {
            self.pending_model = Some(model);
            return vec![Effect::SwitchAgent {
                kind: AgentKind::Native,
                invalidate: true,
            }];
        }
        self.chat.push_info(format!("model set to {model}"));
        self.session.model = Some(model.clone());
        vec![Effect::SaveLastModel(model)]
    }

    fn start_copilot_login(&mut self, pending: PendingCopilotAuth) -> Vec<Effect> {
        self.pending_copilot_auth = Some(pending);
        self.overlay = Overlay::Login(LoginState::Starting);
        vec![Effect::StartLogin]
    }

    fn resume_after_copilot_login(&mut self, pending: PendingCopilotAuth) -> Vec<Effect> {
        match pending {
            PendingCopilotAuth::SwitchAgent(kind) => {
                self.status.notice = Some(format!("switching to {kind}…"));
                vec![Effect::SwitchAgent {
                    kind,
                    invalidate: false,
                }]
            }
            PendingCopilotAuth::ApplyProvider(name) => {
                if self.kind == AgentKind::Native
                    && !self.providers.iter().any(|provider| provider == &name)
                {
                    self.pending_provider = Some(name);
                    vec![Effect::SwitchAgent {
                        kind: AgentKind::Native,
                        invalidate: true,
                    }]
                } else {
                    self.apply_provider(&name)
                }
            }
            PendingCopilotAuth::SetModel(model) => {
                if self.kind == AgentKind::Native
                    && !self.providers.iter().any(|provider| provider == "copilot")
                {
                    self.pending_model = Some(model);
                    vec![Effect::SwitchAgent {
                        kind: AgentKind::Native,
                        invalidate: true,
                    }]
                } else {
                    self.set_model(model)
                }
            }
        }
    }

    fn open_model_picker(&mut self) -> Vec<Effect> {
        match &self.caps.models {
            ModelDiscovery::Static { models } => {
                let items = models.iter().map(static_model_item).collect();
                self.overlay = Overlay::Picker(PickerState::new(
                    "select model",
                    items,
                    PickerAction::SetModel,
                ));
                Vec::new()
            }
            ModelDiscovery::None if self.providers.is_empty() => {
                self.chat.push_info("this agent owns model selection");
                Vec::new()
            }
            _ => {
                if self.catalog.is_empty() {
                    self.awaiting_model_picker = true;
                    self.status.notice = Some("fetching models…".to_owned());
                    vec![Effect::ListModels]
                } else {
                    self.open_catalog_picker();
                    Vec::new()
                }
            }
        }
    }

    fn open_catalog_picker(&mut self) {
        let items = self.catalog.iter().map(catalog_item).collect();
        self.overlay = Overlay::Picker(PickerState::new(
            "select model",
            items,
            PickerAction::SetModel,
        ));
    }

    fn open_provider_picker(&mut self) -> Vec<Effect> {
        if self.providers.is_empty() {
            self.chat
                .push_info("no runtime providers — the agent owns model selection");
            return Vec::new();
        }
        let current = self.current_provider();
        let items = self
            .providers
            .iter()
            .map(|name| PickerItem {
                id: name.clone(),
                label: name.clone(),
                detail: (current.as_deref() == Some(name)).then(|| "current".to_owned()),
                enabled: true,
            })
            .collect();
        self.overlay = Overlay::Picker(PickerState::new(
            "select provider",
            items,
            PickerAction::SwitchProvider,
        ));
        // Prefetch so selection can resolve the provider's default model.
        if self.catalog.is_empty() {
            vec![Effect::ListModels]
        } else {
            Vec::new()
        }
    }

    fn open_agent_picker(&mut self) {
        let items = AgentKind::ALL
            .iter()
            .map(|kind| PickerItem {
                id: kind.id().to_owned(),
                label: kind.id().to_owned(),
                detail: (*kind == self.kind).then(|| "current".to_owned()),
                enabled: true,
            })
            .collect();
        self.overlay = Overlay::Picker(PickerState::new(
            "select agent",
            items,
            PickerAction::SwitchAgent,
        ));
    }

    fn current_provider(&self) -> Option<String> {
        let model = self.session.model.as_ref()?;
        let (provider, _) = model.split();
        provider.map(str::to_owned)
    }

    fn apply_provider(&mut self, name: &str) -> Vec<Effect> {
        if name == "copilot" && !has_copilot_credentials() {
            return self.start_copilot_login(PendingCopilotAuth::ApplyProvider(name.to_owned()));
        }
        if self.providers.is_empty() {
            self.chat
                .push_info("no runtime providers — the agent owns model selection");
            return Vec::new();
        }
        if !self.providers.iter().any(|provider| provider == name) {
            if name == "copilot" && self.kind == AgentKind::Native {
                self.pending_provider = Some(name.to_owned());
                return vec![Effect::SwitchAgent {
                    kind: AgentKind::Native,
                    invalidate: true,
                }];
            }
            self.chat.push_error(format!(
                "provider `{name}` is not registered (available: {})",
                self.providers.join(", ")
            ));
            return Vec::new();
        }
        match self
            .catalog
            .iter()
            .find(|entry| entry.provider.as_str() == name)
        {
            Some(entry) => self.set_model(entry.model_ref()),
            None => {
                self.pending_provider = Some(name.to_owned());
                self.status.notice = Some("fetching models…".to_owned());
                vec![Effect::ListModels]
            }
        }
    }

    fn switch_agent(&mut self, id: &str) -> Vec<Effect> {
        match AgentKind::parse(id) {
            Some(kind) if kind == self.kind => {
                self.chat.push_info(format!("already on {kind}"));
                Vec::new()
            }
            Some(AgentKind::Copilot) if !has_copilot_credentials() => {
                self.start_copilot_login(PendingCopilotAuth::SwitchAgent(AgentKind::Copilot))
            }
            Some(kind) => {
                self.status.notice = Some(format!("switching to {kind}…"));
                vec![Effect::SwitchAgent {
                    kind,
                    invalidate: false,
                }]
            }
            None => {
                self.chat.push_error(format!(
                    "unknown agent `{id}` (available: native, claude-code, copilot)"
                ));
                Vec::new()
            }
        }
    }

    // ── engine events ───────────────────────────────────────────────────────

    fn on_engine(&mut self, event: SessionEvent) -> Vec<Effect> {
        self.session.last_seq = self.session.last_seq.max(event.seq);
        match &event.payload {
            AgentEvent::TurnStarted { .. } => {
                if !self.session.turn.is_running() {
                    self.session.turn = TurnPhase::Running {
                        started: Instant::now(),
                    };
                }
                Vec::new()
            }
            AgentEvent::MessageStarted { role, .. } => {
                if *role == agentloop_contracts::Role::Assistant && !self.session.turn.is_running()
                {
                    self.session.turn = TurnPhase::Running {
                        started: Instant::now(),
                    };
                }
                self.chat.apply(&event.payload);
                Vec::new()
            }
            AgentEvent::MarkdownDelta { .. } | AgentEvent::ThinkingDelta { .. } => {
                if !self.session.turn.is_running() {
                    self.session.turn = TurnPhase::Running {
                        started: Instant::now(),
                    };
                }
                self.chat.apply(&event.payload);
                Vec::new()
            }
            AgentEvent::TurnCompleted { summary, .. } => {
                self.session.turn = TurnPhase::Idle;
                self.status.notice = None;
                self.status.total_usage.add(&summary.usage);
                if summary.cost_usd.is_some() {
                    self.status.last_cost_usd = summary.cost_usd;
                }
                self.chat.finalize_drafts();
                if summary.stop_reason == TurnStopReason::Cancelled {
                    self.chat.push_info("turn interrupted");
                }
                Vec::new()
            }
            AgentEvent::PermissionRequested {
                id,
                call_id,
                title,
                detail,
                options,
            } => {
                let prompt = PermissionPrompt {
                    id: id.clone(),
                    call_id: call_id.clone(),
                    title: title.clone(),
                    detail: detail.clone(),
                    options: options.clone(),
                    selected: 0,
                };
                if self.overlay.is_active() {
                    self.pending_permissions.push_back(prompt);
                } else {
                    self.overlay = Overlay::Permission(prompt);
                }
                Vec::new()
            }
            AgentEvent::PermissionResolved { id, .. } => {
                self.pending_permissions.retain(|p| &p.id != id);
                if matches!(&self.overlay, Overlay::Permission(p) if &p.id == id) {
                    self.overlay = Overlay::None;
                }
                self.drain_pending();
                Vec::new()
            }
            AgentEvent::QuestionRequested { id, questions } => {
                let prompt = QuestionPrompt::new(id.clone(), questions.clone());
                if self.overlay.is_active() {
                    self.pending_questions.push_back(prompt);
                } else {
                    self.overlay = Overlay::Question(prompt);
                }
                Vec::new()
            }
            AgentEvent::QuestionResolved { id, .. } => {
                self.pending_questions.retain(|q| &q.id != id);
                if matches!(&self.overlay, Overlay::Question(q) if &q.id == id) {
                    self.overlay = Overlay::None;
                }
                self.drain_pending();
                Vec::new()
            }
            AgentEvent::Gap { from_seq } => vec![Effect::Resync {
                from_seq: *from_seq,
            }],
            AgentEvent::EngineInfo { capabilities, .. } => {
                let was_supported = self.caps.reasoning_visible;
                self.caps = capabilities.clone();
                self.commands = CommandIndex::new(&self.caps.commands);
                if self.caps.reasoning_visible && !was_supported {
                    self.show_thinking = true;
                } else if !self.caps.reasoning_visible {
                    self.show_thinking = false;
                }
                Vec::new()
            }
            payload => {
                self.chat.apply(payload);
                Vec::new()
            }
        }
    }

    fn drain_pending(&mut self) {
        if self.overlay.is_active() {
            return;
        }
        if let Some(prompt) = self.pending_permissions.pop_front() {
            self.overlay = Overlay::Permission(prompt);
        } else if let Some(prompt) = self.pending_questions.pop_front() {
            self.overlay = Overlay::Question(prompt);
        }
    }

    // ── task results ────────────────────────────────────────────────────────

    fn on_task(&mut self, result: TaskResult) -> Vec<Effect> {
        match result {
            TaskResult::TurnFinished(outcome) => {
                self.session.turn = TurnPhase::Idle;
                if let Err(message) = outcome {
                    // Turn failures render from the event stream; this line
                    // is only a fallback signal (e.g. TurnInProgress).
                    self.status.last_error = Some(message);
                }
                Vec::new()
            }
            TaskResult::CompactFinished(outcome) => {
                self.session.turn = TurnPhase::Idle;
                self.status.notice = None;
                if let Err(message) = outcome {
                    self.status.last_error = Some(message);
                }
                Vec::new()
            }
            TaskResult::Models(Ok(entries)) => {
                self.catalog = entries;
                self.status.notice = None;
                let mut effects = Vec::new();
                if let Some(name) = self.pending_provider.take() {
                    match self
                        .catalog
                        .iter()
                        .find(|entry| entry.provider.as_str() == name)
                    {
                        Some(entry) => effects.extend(self.set_model(entry.model_ref())),
                        None => self
                            .chat
                            .push_error(format!("provider `{name}` listed no models")),
                    }
                } else if self.awaiting_model_picker {
                    self.open_catalog_picker();
                }
                self.awaiting_model_picker = false;
                effects
            }
            TaskResult::Models(Err(message)) => {
                self.awaiting_model_picker = false;
                self.pending_provider = None;
                self.status.notice = None;
                self.status.last_error = Some(format!("model listing failed: {message}"));
                Vec::new()
            }
            TaskResult::EngineSwitched(outcome) => match *outcome {
                Ok(bootstrap) => {
                    let pending_model = self.pending_model.take();
                    let pending_provider = self.pending_provider.take();
                    self.install_bootstrap(bootstrap, true);
                    let mut effects = Vec::new();
                    if let Some(model) = pending_model {
                        effects.extend(self.set_model(model));
                    } else if let Some(name) = pending_provider {
                        effects.extend(self.apply_provider(&name));
                    }
                    effects
                }
                Err(message) => {
                    self.status.notice = None;
                    self.chat.push_error(format!("switch failed: {message}"));
                    Vec::new()
                }
            },
            TaskResult::SessionReset(Ok(id)) => {
                self.session.id = id;
                self.session.last_seq = 0;
                self.session.turn = TurnPhase::Idle;
                self.status.notice = None;
                self.chat = ChatState::default();
                self.markdown_cache.clear();
                self.overlay = Overlay::None;
                self.pending_permissions.clear();
                self.pending_questions.clear();
                self.chat.push_info("new session");
                Vec::new()
            }
            TaskResult::SessionReset(Err(message)) => {
                self.status.notice = None;
                self.chat
                    .push_error(format!("new session failed: {message}"));
                Vec::new()
            }
            TaskResult::Resynced(Ok(transcript)) => {
                self.chat.rebuild_from_transcript(&transcript);
                Vec::new()
            }
            TaskResult::Resynced(Err(message)) => {
                self.status.last_error = Some(format!("resync failed: {message}"));
                Vec::new()
            }
            TaskResult::LoginFinished(Ok(())) => {
                if matches!(self.overlay, Overlay::Login(_)) {
                    self.overlay = Overlay::None;
                    self.drain_pending();
                }
                self.chat.push_info("signed in to GitHub Copilot");
                if let Some(pending) = self.pending_copilot_auth.take() {
                    self.resume_after_copilot_login(pending)
                } else {
                    self.status.notice = Some("reloading providers…".to_owned());
                    vec![Effect::SwitchAgent {
                        kind: AgentKind::Native,
                        invalidate: true,
                    }]
                }
            }
            TaskResult::LoginFinished(Err(message)) => {
                self.pending_copilot_auth = None;
                if message.contains("cancelled") {
                    if matches!(self.overlay, Overlay::Login(_)) {
                        self.overlay = Overlay::None;
                        self.drain_pending();
                    }
                } else if matches!(self.overlay, Overlay::Login(_)) {
                    self.overlay = Overlay::Login(LoginState::Failed { message });
                } else {
                    self.chat.push_error(format!("login failed: {message}"));
                }
                Vec::new()
            }
            TaskResult::ShellCommand { command, outcome } => {
                self.on_shell_command_finished(&command, outcome);
                Vec::new()
            }
        }
    }

    fn on_shell_command_finished(&mut self, command: &str, outcome: ShellCommandOutcome) {
        let summary = shell_command_summary(command, &outcome);
        let cancelled = matches!(outcome, ShellCommandOutcome::Cancelled { .. });
        let overlay_matches = matches!(
            &self.overlay,
            Overlay::ShellCommand(state) if state.command == command
        );
        if overlay_matches {
            if let Overlay::ShellCommand(state) = &mut self.overlay {
                match outcome {
                    ShellCommandOutcome::Completed { output, exit_code } => {
                        state.phase = ShellCommandPhase::Done { output, exit_code };
                        state.scroll = 0;
                    }
                    ShellCommandOutcome::Cancelled { .. } => {
                        self.overlay = Overlay::None;
                        self.drain_pending();
                    }
                    ShellCommandOutcome::Failed { message } => {
                        state.phase = ShellCommandPhase::Failed { message };
                        state.scroll = 0;
                    }
                }
            }
            if !cancelled {
                self.chat.push_info(summary);
            }
        } else {
            self.chat.push_info(summary);
        }
    }

    /// Adopt a (new or resumed) session and its agent's capabilities.
    fn install_bootstrap(&mut self, bootstrap: SessionBootstrap, announce: bool) {
        self.kind = bootstrap.kind;
        self.caps = bootstrap.hello.capabilities.clone();
        self.engine_name = bootstrap.hello.engine.name.clone();
        self.engine_version = bootstrap.hello.engine.version.clone();
        self.commands = CommandIndex::new(&self.caps.commands);
        self.session = SessionState {
            id: bootstrap.session,
            model: bootstrap.model,
            turn: TurnPhase::Idle,
            last_seq: 0,
            permission_mode: bootstrap
                .permission_mode
                .unwrap_or(PermissionMode::AcceptEdits),
            session_mode: SessionMode::Code,
        };
        self.providers = bootstrap.providers;
        self.catalog.clear();
        self.pending_provider = None;
        self.awaiting_model_picker = false;
        self.overlay = Overlay::None;
        self.pending_permissions.clear();
        self.pending_questions.clear();
        self.status.notice = None;
        self.markdown_cache.clear();
        self.show_thinking = bootstrap.hello.capabilities.reasoning_visible;
        if announce {
            self.chat = ChatState::default();
        }
        let resumed = match &bootstrap.transcript {
            Some(transcript) => {
                self.chat.rebuild_from_transcript(transcript);
                true
            }
            None => false,
        };
        if announce {
            let session_note = if resumed {
                "resumed previous session"
            } else {
                "new session"
            };
            self.chat
                .push_info(format!("switched to {} — {session_note}", self.kind));
        }
    }

    // ── login progress ──────────────────────────────────────────────────────

    fn on_login(&mut self, event: LoginEvent) {
        // Ignore progress after the user dismissed the overlay.
        let Overlay::Login(state) = &mut self.overlay else {
            return;
        };
        match event {
            LoginEvent::CodeReady {
                user_code,
                verification_uri,
                expires_in,
            } => {
                *state = LoginState::CodeReady {
                    user_code,
                    verification_uri,
                    expires_in,
                    since: Instant::now(),
                };
            }
            LoginEvent::Polling => {
                // Keep the code visible while polling; only leave CodeReady
                // when verification starts.
            }
            LoginEvent::Verifying => {
                *state = LoginState::Verifying;
            }
            LoginEvent::Succeeded => {
                // Terminal state arrives via TaskResult::LoginFinished.
            }
            _ => {}
        }
    }

    // ── ticks ───────────────────────────────────────────────────────────────

    fn on_tick(&mut self) {
        let busy = self.session.turn.is_running()
            || matches!(self.overlay, Overlay::Login(_))
            || matches!(
                self.overlay,
                Overlay::ShellCommand(ShellCommandOverlay {
                    phase: ShellCommandPhase::Running { .. },
                    ..
                })
            );
        if busy {
            self.status.spinner = self.status.spinner.wrapping_add(1);
            self.dirty = true;
        }
        if let Some(armed) = self.quit_armed_at {
            if armed.elapsed() > QUIT_WINDOW {
                self.quit_armed_at = None;
                if self.status.notice.as_deref() == Some("press ctrl+c again to exit") {
                    self.status.notice = None;
                }
                self.dirty = true;
            }
        }
    }
}

fn shell_command_summary(command: &str, outcome: &ShellCommandOutcome) -> String {
    match outcome {
        ShellCommandOutcome::Completed { exit_code, .. } => match exit_code {
            Some(0) | None => format!("`{command}` finished"),
            Some(code) => format!("`{command}` exited with code {code}"),
        },
        ShellCommandOutcome::Cancelled { .. } => format!("`{command}` cancelled"),
        ShellCommandOutcome::Failed { message } => {
            format!("`{command}` failed: {message}")
        }
    }
}

fn static_model_item(model: &ModelInfo) -> PickerItem {
    PickerItem {
        id: model.id.clone(),
        label: model
            .display_name
            .clone()
            .unwrap_or_else(|| model.id.clone()),
        detail: model_badges(model),
        enabled: true,
    }
}

fn catalog_item(entry: &CatalogEntry) -> PickerItem {
    PickerItem {
        id: entry.model_ref().0,
        label: format!("{}/{}", entry.provider, entry.model.id),
        detail: model_badges(&entry.model),
        enabled: true,
    }
}

fn parse_session_mode(text: &str) -> Option<SessionMode> {
    match text.trim().to_lowercase().as_str() {
        "code" => Some(SessionMode::Code),
        "plan" => Some(SessionMode::Plan),
        _ => None,
    }
}

fn parse_permission_arg(text: &str) -> Option<PermissionMode> {
    match text.trim().to_lowercase().as_str() {
        "require" | "req" | "default" => Some(PermissionMode::Default),
        "auto" | "accept-edits" => Some(PermissionMode::AcceptEdits),
        "allow-all" | "all" | "bypass" => Some(PermissionMode::BypassPermissions),
        _ => None,
    }
}

fn permission_picker_items(
    supported: &[PermissionMode],
    current: PermissionMode,
) -> Vec<PickerItem> {
    let choices = [
        (
            "require",
            "require",
            "ask on mutating tools",
            PermissionMode::Default,
        ),
        (
            "auto",
            "auto",
            "auto-allow file edits",
            PermissionMode::AcceptEdits,
        ),
        (
            "allow-all",
            "allow all",
            "bypass all prompts",
            PermissionMode::BypassPermissions,
        ),
    ];
    choices
        .into_iter()
        .map(|(id, label, detail, mode)| {
            let enabled = supported.contains(&mode);
            PickerItem {
                id: id.to_owned(),
                label: label.to_owned(),
                detail: Some(if !enabled {
                    "not supported by agent".to_owned()
                } else if current == mode {
                    "current".to_owned()
                } else {
                    detail.to_owned()
                }),
                enabled,
            }
        })
        .collect()
}

fn is_copilot_model(model: &ModelRef) -> bool {
    model.split().0 == Some("copilot")
}

fn model_badges(model: &ModelInfo) -> Option<String> {
    let mut badges = Vec::new();
    if let Some(window) = model.context_window {
        badges.push(format!("{}k ctx", window / 1000));
    }
    if model.reasoning {
        badges.push("reasoning".to_owned());
    }
    if model.vision {
        badges.push("vision".to_owned());
    }
    (!badges.is_empty()).then(|| badges.join(" · "))
}

#[cfg(test)]
mod copilot_auth_tests {
    use super::*;
    use crate::chat::ChatItem;
    use crate::commands::LocalCommand;
    use crate::events::SessionBootstrap;
    use crate::files::FileIndex;
    use agentloop_contracts::{AgentCaps, Hello, SessionId};

    fn isolated_bootstrap() -> SessionBootstrap {
        SessionBootstrap {
            kind: AgentKind::Native,
            hello: Hello::new(AgentCaps::default()),
            session: SessionId::from("sess-test"),
            providers: vec!["anthropic".to_owned()],
            model: None,
            transcript: None,
            trace: Vec::new(),
            permission_mode: None,
        }
    }

    fn test_app(bootstrap: SessionBootstrap) -> App {
        App::new(bootstrap, PathBuf::from("."), FileIndex::default())
    }

    fn without_copilot_credentials<R>(f: impl FnOnce() -> R) -> R {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_home = dir.path().join("config");
        temp_env::with_vars(
            [
                ("COPILOT_GITHUB_TOKEN", None::<&str>),
                ("GH_COPILOT_TOKEN", None::<&str>),
                ("XDG_CONFIG_HOME", Some(config_home.to_str().expect("utf8"))),
            ],
            f,
        )
    }

    #[test]
    fn switch_to_copilot_agent_without_token_starts_login() {
        without_copilot_credentials(|| {
            let mut app = test_app(isolated_bootstrap());
            let effects = app.run_local(LocalCommand::Agent {
                arg: Some("copilot".to_owned()),
            });
            assert!(effects.contains(&Effect::StartLogin));
            assert!(matches!(app.overlay, Overlay::Login(_)));
            assert_eq!(
                app.pending_copilot_auth,
                Some(PendingCopilotAuth::SwitchAgent(AgentKind::Copilot))
            );
        });
    }

    #[test]
    fn apply_copilot_provider_without_token_starts_login() {
        without_copilot_credentials(|| {
            let mut app = test_app(isolated_bootstrap());
            let effects = app.apply_provider("copilot");
            assert!(effects.contains(&Effect::StartLogin));
            assert_eq!(
                app.pending_copilot_auth,
                Some(PendingCopilotAuth::ApplyProvider("copilot".to_owned()))
            );
        });
    }

    #[test]
    fn set_copilot_model_without_token_starts_login() {
        without_copilot_credentials(|| {
            let mut app = test_app(isolated_bootstrap());
            let effects = app.set_model(ModelRef::from("copilot/gpt-4.1"));
            assert!(effects.contains(&Effect::StartLogin));
            assert_eq!(
                app.pending_copilot_auth,
                Some(PendingCopilotAuth::SetModel(ModelRef::from(
                    "copilot/gpt-4.1"
                )))
            );
        });
    }

    #[test]
    fn deprecated_login_slash_command_shows_hint() {
        let mut app = test_app(isolated_bootstrap());
        let effects = app.on_submit("/login");
        assert!(effects.is_empty());
        assert!(app.chat.items.iter().any(|item| {
            matches!(item, ChatItem::Info { text } if text.contains("starts automatically"))
        }));
    }
}

#[cfg(test)]
mod session_tests {
    use super::*;
    use agentloop_contracts::SessionId;

    fn test_session() -> SessionState {
        SessionState {
            id: SessionId::from("sess-test"),
            model: None,
            turn: TurnPhase::Idle,
            last_seq: 0,
            permission_mode: PermissionMode::AcceptEdits,
            session_mode: SessionMode::Code,
        }
    }

    #[test]
    fn effective_permission_mode_plan_overrides_security() {
        let mut session = test_session();
        session.session_mode = SessionMode::Plan;
        assert_eq!(session.effective_permission_mode(), PermissionMode::Plan);
    }
}
