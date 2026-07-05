//! The application state and its Elm-style reducer.
//!
//! [`App::update`] is pure with respect to I/O: it mutates state and returns
//! [`Effect`]s for the runtime to execute. Key routing precedence:
//! Ctrl+C/Ctrl+D → active overlay → input popup → global chords → editor.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use agentloop_cli_core::{
    AgentKind, CatalogEntry, CliPrefs, InstallTarget, LoginEvent, McpStore,
    delegated_agents_enabled, format_relative_time, has_copilot_credentials,
    model_provider_available, needs_provider_setup, parse_install_target, session_display_label,
};
use agentloop_contracts::{
    AgentCaps, AgentEvent, Effort, ModelDiscovery, ModelInfo, ModelRef, PermissionDecision,
    PermissionMode, PermissionRequestId, PromptInput, QuestionId, SessionEvent, SessionId,
    ThinkingConfig, TokenUsage, TurnOptions, TurnStopReason,
};

use crate::chat::ChatState;
use crate::commands::{CommandIndex, LocalCommand, McpSubcommand, Route};
use crate::events::{
    AppEvent, Effect, EngineBootstrap, SessionBootstrap, ShellCommandOutcome, TaskResult,
};
use crate::files::FileIndex;
use crate::input::{InputOutcome, InputState};
use crate::overlay::{
    self, CommandPaletteAction, CommandPaletteEntry, CommandPaletteState, ConfirmAction,
    ConfirmPrompt, ConnectWizardState, ConnectWizardStep, LoginState, McpExplorerPhase,
    McpExplorerState, McpInstallChoice, McpInstallMode, McpInstallState, McpListItem, McpListState,
    Overlay, OverlayOutcome, PermissionPrompt, PickerAction, PickerChoice, PickerItem, PickerState,
    QuestionPrompt, ShellCommandOverlay, ShellCommandPhase,
};
use crate::theme::{self, BuiltinTheme, Theme};
use crate::ui::MarkdownCache;

/// PageUp/PageDown scroll step in wrapped lines.
const SCROLL_STEP: usize = 10;
/// Arrow-key scroll step when the prompt is empty.
const ARROW_SCROLL_STEP: usize = 3;
/// How long a toast stays visible.
const TOAST_TTL: Duration = Duration::from_secs(4);
/// At most this many toasts queue before the oldest is dropped.
const TOAST_CAP: usize = 3;
/// Transcript marker for an interrupted turn.
pub(crate) const INTERRUPT_NOTE: &str = "⎿ Interrupted";
/// Transcript marker for a turn stopped by the per-turn step limit — without
/// this, hitting the limit looks identical to the agent quietly giving up.
pub(crate) const MAX_ITERATIONS_NOTE: &str = "⎿ Stopped: reached the per-turn step limit. \
     Send another message to continue, or raise max_iterations in config.json for longer runs.";

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

/// Top-level screen: empty home vs active session chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppRoute {
    Home,
    Session,
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
    /// How hard the agent works: scales reasoning depth, tool aggressiveness,
    /// and (at the top) orchestration. Sent as `TurnOptions.effort`.
    pub effort: Effort,
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

/// A transient notification shown on the line above the input for a few
/// seconds; never enters the transcript.
#[derive(Debug)]
pub struct Toast {
    pub text: String,
    pub created: Instant,
}

/// Status-bar and notification-line state.
#[derive(Debug, Default)]
pub struct StatusState {
    /// Accumulated from `TurnCompleted` summaries.
    pub total_usage: TokenUsage,
    pub last_cost_usd: Option<f64>,
    /// Transient notifications, newest last; expired on tick.
    pub toasts: VecDeque<Toast>,
    /// Spinner animation counter, advanced by ticks while busy.
    pub spinner: usize,
    /// Busy-line verb index, picked at turn start and stable for the turn.
    pub turn_verb_idx: usize,
    /// Streamed output characters this turn (approximate tokens = chars/4,
    /// snapped to reported usage as messages materialize).
    pub turn_output_chars: u64,
    /// Prompt-side context size from the last `TurnCompleted`
    /// (`usage.input + cache_read`), for the context-% segment.
    pub last_context_tokens: Option<u64>,
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
    /// Extended-thinking token budget; `None` = off for provider turns.
    pub thinking_budget: Option<u32>,
    /// Agent switch in flight (for Copilot probe-fail tip).
    pending_switch_kind: Option<AgentKind>,
    pub should_quit: bool,
    /// Session working directory (`--workdir`); scopes `@` file search.
    pub workdir: PathBuf,
    /// Indexed files under [`Self::workdir`] for `@` mention autocomplete.
    pub file_index: FileIndex,
    /// Installed MCP servers (`~/.config/agentloop/mcp.json`).
    pub mcp_store: McpStore,
    /// Enabled MCP servers in the current native session (status bar).
    pub mcp_enabled: usize,
    /// Prompts submitted while a turn was running; sent in order after each
    /// turn completes. Cleared when the user interrupts.
    pub queued_prompts: std::collections::VecDeque<String>,
    /// Fallback model chain sent with every turn (from config.json).
    pub fallback_models: Vec<ModelRef>,
    /// Conversation captured at an agent switch, injected into the next prompt
    /// so context carries across agents. `None` once consumed or when idle.
    carried_context: Option<String>,
    /// Active color theme id (for the picker's "current" marker and persist).
    pub theme_id: String,
    /// Whether the terminal supports truecolor; drives theme resolution.
    truecolor: bool,
    /// Theme snapshot taken when the picker opens, restored on cancel.
    saved_theme: Option<Theme>,
    /// Show centered home hints when the transcript is empty.
    pub home_screen: bool,
    /// User dismissed the getting-started connect card.
    pub getting_started_dismissed: bool,
    /// Cached `git rev-parse --abbrev-ref HEAD` for the sidebar footer.
    pub git_branch: Option<String>,
    /// Open the model picker after the next successful provider connect.
    connect_then_pick_model: bool,
    /// Home vs session chat route.
    pub route: AppRoute,
    /// First user message held until lazy session creation completes.
    pending_first_prompt: Option<String>,
    /// Sidebar session label (title or formatted timestamp).
    pub session_label: Option<String>,
    dirty: bool,
}

impl App {
    /// Build the app on the home screen (engine ready, no session yet).
    pub fn new_home(engine: EngineBootstrap, workdir: PathBuf, file_index: FileIndex) -> Self {
        Self {
            kind: engine.kind,
            caps: engine.hello.capabilities.clone(),
            engine_name: engine.hello.engine.name.clone(),
            engine_version: engine.hello.engine.version.clone(),
            session: SessionState {
                id: SessionId(String::new()),
                model: None,
                turn: TurnPhase::Idle,
                last_seq: 0,
                permission_mode: PermissionMode::AcceptEdits,
                session_mode: SessionMode::Code,
                effort: Effort::default(),
            },
            chat: ChatState::default(),
            input: InputState::default(),
            commands: CommandIndex::new(&engine.hello.capabilities.commands),
            overlay: Overlay::None,
            pending_permissions: VecDeque::new(),
            pending_questions: VecDeque::new(),
            providers: engine.providers,
            catalog: Vec::new(),
            pending_provider: None,
            awaiting_model_picker: false,
            pending_copilot_auth: None,
            pending_model: None,
            status: StatusState::default(),
            markdown_cache: MarkdownCache::default(),
            mouse_capture: true,
            show_thinking: engine.hello.capabilities.reasoning_visible,
            thinking_budget: None,
            pending_switch_kind: None,
            should_quit: false,
            workdir,
            file_index,
            mcp_store: McpStore::load(),
            mcp_enabled: engine.mcp_enabled,
            queued_prompts: std::collections::VecDeque::new(),
            fallback_models: Vec::new(),
            carried_context: None,
            theme_id: BuiltinTheme::DEFAULT.id().to_owned(),
            truecolor: theme::terminal_supports_truecolor(),
            saved_theme: None,
            home_screen: true,
            getting_started_dismissed: false,
            git_branch: None,
            connect_then_pick_model: false,
            route: AppRoute::Home,
            pending_first_prompt: None,
            session_label: None,
            dirty: true,
        }
    }

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
                effort: Effort::default(),
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
            // On by default: capturing the wheel (CSI ?1000h) keeps scrolling
            // inside the transcript — without it the wheel scrolls the terminal's
            // own scrollback, letting you scroll up "over" the CLI into
            // pre-launch output, which is poor TUI behaviour. The escape-code
            // garbage this used to cause was really a UTF-8 truncation panic
            // (fixed) plus a panic hook that tore down the terminal on a caught
            // worker panic (fixed) — not mouse capture itself. Selection then
            // needs a terminal modifier (⌥ iTerm2 · Fn Terminal.app · Shift
            // Linux); Ctrl+M flips to no-modifier select (persisted via
            // `apply_loaded_prefs`). Reload hardening: force_raw_mode + MCP
            // process-group isolation keep raw mode from being clobbered.
            mouse_capture: true,
            show_thinking: bootstrap.hello.capabilities.reasoning_visible,
            thinking_budget: None,
            pending_switch_kind: None,
            should_quit: false,
            workdir,
            file_index,
            mcp_store: McpStore::load(),
            mcp_enabled: bootstrap.mcp_enabled,
            queued_prompts: std::collections::VecDeque::new(),
            fallback_models: Vec::new(),
            carried_context: None,
            theme_id: BuiltinTheme::DEFAULT.id().to_owned(),
            truecolor: theme::terminal_supports_truecolor(),
            saved_theme: None,
            home_screen: true,
            getting_started_dismissed: false,
            git_branch: None,
            connect_then_pick_model: false,
            route: AppRoute::Session,
            pending_first_prompt: None,
            session_label: None,
            dirty: true,
        };
        app.install_bootstrap(bootstrap, false);
        app
    }

    /// Whether the UI is on the empty home screen (no session transcript).
    pub fn is_home_route(&self) -> bool {
        self.route == AppRoute::Home
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

    /// Apply persisted CLI preferences (modes, thinking, mouse capture).
    pub fn apply_loaded_prefs(&mut self, prefs: &CliPrefs) {
        if let Some(ref text) = prefs.session_mode {
            if let Some(mode) = parse_session_mode(text) {
                self.session.session_mode = mode;
            }
        }
        if let Some(ref text) = prefs.permission_mode {
            if let Some(mode) = parse_stored_permission_mode(text) {
                self.session.permission_mode = mode;
            }
        }
        if let Some(ref text) = prefs.effort {
            if let Some(level) = parse_effort_level(text) {
                self.session.effort = level;
            }
        }
        if let Some(visible) = prefs.thinking_visible {
            self.show_thinking = visible;
        }
        if let Some(capture) = prefs.mouse_capture {
            self.mouse_capture = capture;
        }
        if let Some(home) = prefs.home_screen {
            self.home_screen = home;
        }
        if prefs.getting_started_dismissed == Some(true) {
            self.getting_started_dismissed = true;
        }
        self.thinking_budget = prefs.thinking_budget;
        self.fallback_models = prefs
            .fallback_models
            .iter()
            .map(|model| ModelRef(model.clone()))
            .collect();
        let builtin = prefs
            .theme
            .as_deref()
            .and_then(BuiltinTheme::from_id)
            .unwrap_or(BuiltinTheme::DEFAULT);
        theme::set_active(builtin.resolve(self.truecolor));
        self.theme_id = builtin.id().to_owned();
    }

    /// Show a transient notification above the input (never in transcript).
    pub fn toast(&mut self, text: impl Into<String>) {
        self.status.toasts.push_back(Toast {
            text: text.into(),
            created: Instant::now(),
        });
        while self.status.toasts.len() > TOAST_CAP {
            self.status.toasts.pop_front();
        }
        self.dirty = true;
    }

    /// Enter the running phase and reset the per-turn busy-line state.
    /// Drop queued prompts (on interrupt): the user is taking back control.
    fn clear_prompt_queue(&mut self) {
        if !self.queued_prompts.is_empty() {
            self.toast(format!(
                "cleared {} queued prompt(s)",
                self.queued_prompts.len()
            ));
            self.queued_prompts.clear();
        }
    }

    fn begin_turn(&mut self) {
        if self.session.turn.is_running() {
            return;
        }
        self.session.turn = TurnPhase::Running {
            started: Instant::now(),
        };
        self.status.turn_verb_idx = pick_verb_idx();
        self.status.turn_output_chars = 0;
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
            AppEvent::OpenAiOAuth(event) => {
                self.dirty = true;
                self.on_openai_oauth(event);
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
                if self.overlay.is_active() && overlay::handle_paste(&mut self.overlay, &text) {
                    return Vec::new();
                }
                self.input.paste(&text);
                let skip_refresh = text.len() > crate::input::PASTE_PLACEHOLDER_MIN_CHARS
                    || self.input.has_pasted_blocks();
                if !skip_refresh {
                    let refresh_start = std::time::Instant::now();
                    self.input
                        .refresh_popup(&self.commands, &self.file_index, &self.workdir);
                    // #region agent log
                    if text.len() > 1_000 {
                        crate::debug_agent::log_refresh_popup(
                            self.input.text().len(),
                            refresh_start.elapsed().as_micros(),
                        );
                    }
                    // #endregion
                }
                self.dirty = true;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn on_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        // Copy transcript before generic Ctrl/Cmd+C handling.
        if matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && key.modifiers.contains(KeyModifiers::SHIFT)
        {
            return self.copy_chat();
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

        // Global chords before the editor (when no modal is open).
        if !self.overlay.is_active() {
            if matches!(key.code, KeyCode::Char('p') | KeyCode::Char('P'))
                && key.modifiers.contains(KeyModifiers::CONTROL)
            {
                return self.open_command_palette();
            }
            if matches!(key.code, KeyCode::Char('?'))
                && (key.modifiers.is_empty()
                    || key.modifiers == KeyModifiers::CONTROL
                    || key.modifiers == KeyModifiers::SHIFT)
            {
                self.overlay = Overlay::WhichKey;
                return Vec::new();
            }
        }

        let popup_open = self.input.popup.is_some();

        // Global chords that don't collide with the popup.
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) if !popup_open => {
                if self.session.turn.is_running() {
                    self.clear_prompt_queue();
                    self.toast("interrupting…");
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
            // Expand/collapse the latest thinking block.
            (KeyCode::Char('t'), KeyModifiers::CONTROL) => {
                if self.thinking_visible() {
                    self.chat.toggle_last_thinking();
                }
                return Vec::new();
            }
            // Expand/collapse the focused (or last) tool result.
            (KeyCode::Char('o'), KeyModifiers::CONTROL) => {
                self.chat.toggle_focused_tool_expand();
                return Vec::new();
            }
            // Shift+Tab (BackTab on most terminals) cycles the working mode.
            (KeyCode::BackTab, _) => {
                return self.cycle_ui_mode();
            }
            (KeyCode::Tab, KeyModifiers::SHIFT) => {
                return self.cycle_ui_mode();
            }
            (KeyCode::Tab, KeyModifiers::NONE) if self.input.is_empty() => {
                if self.chat.cycle_tool_focus(false) {
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
        match self
            .input
            .handle_key(key, &self.commands, &self.file_index, &self.workdir)
        {
            InputOutcome::Submitted(line) => self.on_submit(&line),
            InputOutcome::Consumed | InputOutcome::Ignored => Vec::new(),
        }
    }

    fn on_ctrl_c(&mut self) -> Vec<Effect> {
        if self.session.turn.is_running() {
            self.clear_prompt_queue();
            self.toast("interrupting…");
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
            self.toast("input cleared — ctrl+c again to exit");
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
        if effects
            .iter()
            .any(|effect| matches!(effect, Effect::CancelOpenAiOAuth))
        {
            // OAuth overlay stays open until user picks another method.
        }
        if let Some(info) = outcome.info {
            self.chat.push_info(info);
        }
        if let Some(action) = outcome.confirmed {
            effects.extend(self.apply_confirm_action(action));
        }
        if outcome.mcp_list_saved {
            self.sync_mcp_list_overlay();
            effects.extend(self.save_mcp_list());
        }
        if let Some(choice) = outcome.mcp_install {
            effects.extend(self.apply_mcp_install_choice(choice));
        }
        if let Some(id) = outcome.preview_theme {
            self.preview_theme(&id);
        }
        if outcome.revert_theme {
            self.revert_theme();
        }
        if outcome.close {
            self.overlay = Overlay::None;
            self.drain_pending();
        }
        if let Some(choice) = outcome.choice {
            effects.extend(self.apply_picker_choice(choice));
        }
        if let Some(action) = outcome.palette_action {
            effects.extend(self.apply_palette_action(action));
        }
        if let Some((id, config)) = outcome.connect_wizard {
            self.dismiss_getting_started();
            self.toast(format!("validating {id}…"));
            effects.push(Effect::ValidateProvider { id, config });
        }
        if outcome.start_copilot_login {
            self.dismiss_getting_started();
            effects.extend(
                self.start_copilot_login(PendingCopilotAuth::ApplyProvider("copilot".to_owned())),
            );
        }
        if let Some(method) = outcome.start_openai_oauth {
            self.dismiss_getting_started();
            effects.push(Effect::StartOpenAiOAuth { method });
        }
        effects
    }

    fn save_mcp_list(&mut self) -> Vec<Effect> {
        if let Err(err) = self.mcp_store.save() {
            self.chat
                .push_error(format!("failed to save mcp.json: {err}"));
            return Vec::new();
        }
        if self.kind != AgentKind::Native {
            self.toast("MCP config saved — switch to native to apply");
            return Vec::new();
        }
        self.toast("reloading MCP servers…");
        vec![Effect::ReloadEngine { invalidate: true }]
    }

    fn apply_mcp_install_choice(&mut self, choice: McpInstallChoice) -> Vec<Effect> {
        if self.session.turn.is_running() {
            self.toast("turn in progress — esc to cancel first");
            return Vec::new();
        }
        let (target, registry_id, import_path) = match choice {
            McpInstallChoice::Registry { id } => (InstallTarget::Unknown, Some(id), None),
            McpInstallChoice::Npm { package } => (InstallTarget::Npm(package), None, None),
            McpInstallChoice::Import { path } => (
                InstallTarget::Unknown,
                None,
                Some(std::path::PathBuf::from(path)),
            ),
        };
        vec![Effect::McpInstall {
            target,
            registry_id,
            import_path,
        }]
    }

    fn apply_confirm_action(&mut self, action: ConfirmAction) -> Vec<Effect> {
        match action {
            ConfirmAction::AllowAllPermissions => {
                self.set_permission_mode(PermissionMode::BypassPermissions)
            }
            ConfirmAction::McpRemove { name } => {
                if self.session.turn.is_running() {
                    self.toast("turn in progress — esc to cancel first");
                    return Vec::new();
                }
                if let Err(err) = self.mcp_store.remove(&name) {
                    self.chat.push_error(err.to_string());
                    return Vec::new();
                }
                if let Err(err) = self.mcp_store.save() {
                    self.chat
                        .push_error(format!("failed to save mcp.json: {err}"));
                    return Vec::new();
                }
                self.toast(format!("removed MCP server `{name}`"));
                if self.kind == AgentKind::Native {
                    vec![Effect::ReloadEngine { invalidate: true }]
                } else {
                    Vec::new()
                }
            }
            ConfirmAction::McpImport { servers } => {
                let added = self.mcp_store.import_missing(servers);
                self.save_mcp_list_reporting(&added)
            }
            ConfirmAction::SaveProviderAnyway { id, config } => {
                self.save_provider(&id, config, None)
            }
        }
    }

    fn dismiss_getting_started(&mut self) {
        if self.getting_started_dismissed {
            return;
        }
        self.getting_started_dismissed = true;
        let _ = CliPrefs::dismiss_getting_started();
    }

    /// Whether the getting-started connect card should render.
    pub fn show_getting_started(&self) -> bool {
        !self.getting_started_dismissed && needs_provider_setup(&self.providers, &CliPrefs::load())
    }

    /// Save `mcp_store` and report which of `imported` names were actually
    /// added (used after `/mcp-import`, where some names may have already
    /// existed between confirming and applying).
    fn save_mcp_list_reporting(&mut self, imported: &[String]) -> Vec<Effect> {
        if let Err(err) = self.mcp_store.save() {
            self.chat
                .push_error(format!("failed to save mcp.json: {err}"));
            return Vec::new();
        }
        if imported.is_empty() {
            self.toast("nothing new to import");
            return Vec::new();
        }
        self.toast(format!("imported: {}", imported.join(", ")));
        if self.kind == AgentKind::Native {
            vec![Effect::ReloadEngine { invalidate: true }]
        } else {
            Vec::new()
        }
    }

    fn sync_mcp_list_overlay(&mut self) {
        let Overlay::McpList(state) = &self.overlay else {
            return;
        };
        for item in &state.items {
            if let Some(server) = self
                .mcp_store
                .servers
                .iter_mut()
                .find(|server| server.config.name == item.name)
            {
                server.config.enabled = item.enabled;
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
            PickerChoice::SetPermissionMode(id) => self.apply_permission_picker_id(&id),
            PickerChoice::SetTheme(id) => self.apply_theme(&id),
            PickerChoice::SetEffort(id) => {
                self.apply_effort_arg(&id);
                Vec::new()
            }
            PickerChoice::ResumeSession(id) => {
                if id == self.session.id.0 {
                    self.toast("already on this session");
                    Vec::new()
                } else {
                    self.toast(format!("resuming {id}…"));
                    vec![Effect::ResumeSession {
                        id: SessionId::from(id),
                    }]
                }
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

    /// Snapshot the current conversation to inject into the first prompt on
    /// the next agent, so context carries across a switch. Capped to bound the
    /// injected prompt; a no-op when there's nothing to carry.
    fn capture_carry_context(&mut self) {
        const CARRY_MAX: usize = 8000;
        let text = self.chat.plain_text();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            self.carried_context = None;
            return;
        }
        let digest = if trimmed.len() <= CARRY_MAX {
            trimmed.to_owned()
        } else {
            let start = trimmed.len() - CARRY_MAX;
            let start = (start..=trimmed.len())
                .find(|&idx| trimmed.is_char_boundary(idx))
                .unwrap_or(trimmed.len());
            format!("[…earlier conversation truncated…]\n{}", &trimmed[start..])
        };
        self.carried_context = Some(digest);
    }

    fn submit_prompt(&mut self, line: &str) -> Vec<Effect> {
        if self.route == AppRoute::Home {
            self.pending_first_prompt = Some(line.to_owned());
            self.dirty = true;
            return vec![Effect::OpenSession {
                model: self.session.model.clone(),
            }];
        }
        if self.session.turn.is_running() {
            self.queued_prompts.push_back(line.to_owned());
            self.dirty = true;
            return Vec::new();
        }
        let mut input = crate::files::expand_file_mentions(line, &self.workdir, &self.file_index);
        // Carry the pre-switch conversation into the first prompt on the new
        // agent (works for every agent, since it's just prompt text).
        if let Some(context) = self.carried_context.take() {
            input = format!(
                "Context from earlier in this conversation, before I switched agents:\n\n\
                 {context}\n\n---\n\nContinuing from the above, here is my request:\n\n{input}"
            );
        }
        self.begin_turn();
        vec![Effect::SubmitPrompt {
            input: PromptInput::text(&input),
            opts: self.turn_options(),
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
                self.toast("starting new session…");
                vec![Effect::NewSession]
            }
            LocalCommand::Clear => {
                self.toast("clearing chat…");
                vec![Effect::ClearSession]
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
            LocalCommand::Theme { arg: Some(name) } => self.apply_theme(name.trim()),
            LocalCommand::Theme { arg: None } => {
                self.open_theme_picker();
                Vec::new()
            }
            LocalCommand::Effort { arg: Some(level) } => {
                self.apply_effort_arg(level.trim());
                Vec::new()
            }
            LocalCommand::Effort { arg: None } => {
                self.open_effort_picker();
                Vec::new()
            }
            LocalCommand::Compact => self.run_compact(),
            LocalCommand::Connect { arg } => self.run_connect(arg),
            LocalCommand::Providers => self.run_providers(),
            LocalCommand::Roles { preset: None } => self.run_roles(),
            LocalCommand::Roles { preset: Some(name) } => self.run_roles_preset(&name),
            LocalCommand::Disconnect { arg } => self.run_disconnect(arg),
            LocalCommand::Mcps => self.open_mcp_list(),
            LocalCommand::Mcp { sub } => self.run_mcp_command(sub),
            LocalCommand::McpInstall { arg } => self.run_mcp_install(arg),
            LocalCommand::McpRemove { name } => self.run_mcp_remove(&name),
            LocalCommand::Merge => {
                if self.session.turn.is_running() {
                    self.chat.push_error(
                        "a turn is running — wait for it to finish or press Esc, \
                         then /merge (git can't safely touch the worktree mid-turn)",
                    );
                    return Vec::new();
                }
                self.toast("verifying and merging the isolated workspace…");
                vec![Effect::IntegrateWorkspace]
            }
            LocalCommand::Discard => {
                if self.session.turn.is_running() {
                    self.chat.push_error(
                        "a turn is running — wait for it to finish or press Esc, \
                         then /discard (git can't safely touch the worktree mid-turn)",
                    );
                    return Vec::new();
                }
                self.toast("discarding the isolated workspace…");
                vec![Effect::DiscardWorkspace]
            }
            LocalCommand::Isolation { arg } => self.run_isolation(arg),
            LocalCommand::Init => self.run_init(),
            LocalCommand::McpImport => self.run_mcp_import(),
            LocalCommand::Sessions => self.open_sessions_picker(),
        }
    }

    fn mcp_change_blocked(&mut self) -> bool {
        if self.session.turn.is_running() {
            self.toast("turn in progress — esc to cancel first");
            true
        } else {
            false
        }
    }

    fn open_mcp_list(&mut self) -> Vec<Effect> {
        if self.mcp_store.servers.is_empty() {
            self.toast("no MCP servers — use /mcp-install");
            return Vec::new();
        }
        let items = self
            .mcp_store
            .servers
            .iter()
            .map(|server| McpListItem {
                name: server.config.name.clone(),
                source: server.source_label(),
                enabled: server.config.enabled,
            })
            .collect();
        self.overlay = Overlay::McpList(McpListState {
            items,
            filter: String::new(),
            selected: 0,
            dirty: false,
        });
        Vec::new()
    }

    fn run_mcp_command(&mut self, sub: McpSubcommand) -> Vec<Effect> {
        if self.mcp_change_blocked() {
            return Vec::new();
        }
        match sub {
            McpSubcommand::Attach { name } => {
                if !self
                    .mcp_store
                    .servers
                    .iter()
                    .any(|server| server.config.name == name)
                {
                    self.chat.push_error(format!(
                        "unknown MCP server `{name}` — /mcps to list or /mcp-install"
                    ));
                    return Vec::new();
                }
                match self.mcp_store.enable(&name) {
                    Ok(changed) => {
                        if let Err(err) = self.mcp_store.save() {
                            self.chat
                                .push_error(format!("failed to save mcp.json: {err}"));
                            return Vec::new();
                        }
                        if changed {
                            self.toast(format!("enabled MCP server `{name}` — reloading…"));
                        } else {
                            self.toast(format!("MCP server `{name}` already enabled"));
                        }
                        if self.kind == AgentKind::Native {
                            vec![Effect::ReloadEngine { invalidate: true }]
                        } else {
                            self.chat
                                .push_info("switch to native agent for MCP tools in session");
                            Vec::new()
                        }
                    }
                    Err(err) => {
                        self.chat.push_error(err.to_string());
                        Vec::new()
                    }
                }
            }
            McpSubcommand::Explore { name } => {
                if !self
                    .mcp_store
                    .servers
                    .iter()
                    .any(|server| server.config.name == name)
                {
                    self.chat.push_error(format!("unknown MCP server `{name}`"));
                    return Vec::new();
                }
                self.overlay = Overlay::McpExplorer(McpExplorerState {
                    server: name.clone(),
                    phase: McpExplorerPhase::Loading,
                    selected: 0,
                    filter: String::new(),
                    args_input: "{}".to_owned(),
                    args_mode: false,
                    scroll: 0,
                });
                vec![Effect::McpListTools { server: name }]
            }
        }
    }

    fn run_mcp_install(&mut self, arg: Option<String>) -> Vec<Effect> {
        if self.mcp_change_blocked() {
            return Vec::new();
        }
        if let Some(arg) = arg.filter(|text| !text.trim().is_empty()) {
            let target = parse_install_target(&arg);
            return vec![Effect::McpInstall {
                target,
                registry_id: None,
                import_path: None,
            }];
        }
        self.overlay = Overlay::McpInstall(McpInstallState {
            mode: McpInstallMode::Registry,
            filter: String::new(),
            selected: 0,
            input: String::new(),
            input_mode: false,
        });
        Vec::new()
    }

    fn run_mcp_remove(&mut self, name: &str) -> Vec<Effect> {
        if self.mcp_change_blocked() {
            return Vec::new();
        }
        let name = name.trim();
        if name.is_empty() {
            self.chat.push_error("usage: /mcp-remove <name>");
            return Vec::new();
        }
        if !self
            .mcp_store
            .servers
            .iter()
            .any(|server| server.config.name == name)
        {
            self.chat.push_error(format!("unknown MCP server `{name}`"));
            return Vec::new();
        }
        self.overlay = Overlay::Confirm(ConfirmPrompt {
            title: format!("Remove MCP server `{name}`?"),
            message: "This deletes the server from mcp.json (clone dirs are kept on disk)."
                .to_owned(),
            action: ConfirmAction::McpRemove {
                name: name.to_owned(),
            },
        });
        Vec::new()
    }

    /// `/init` — snapshot the currently-enabled MCP integrations into this
    /// project's `.agent/mcp.json`, so they're documented and can travel with
    /// the project (e.g. committed to source control). Never overwrites an
    /// existing project file — that could clobber a teammate's setup; run
    /// `/mcp-import` instead to adopt what it declares.
    fn run_init(&mut self) -> Vec<Effect> {
        let project_file = agentloop_cli_core::mcp_project_path(&self.workdir);
        if project_file.exists() {
            let declared = McpStore::load_project(&self.workdir)
                .map(|store| store.servers.len())
                .unwrap_or(0);
            self.chat.push_info(format!(
                "{} already declares {declared} MCP integration(s) — \
                 run /mcp-import to adopt any not yet installed, or edit it directly",
                project_file.display()
            ));
            return Vec::new();
        }
        if self.mcp_store.servers.is_empty() {
            self.chat.push_info(
                "no MCP servers configured yet — connect some with /mcp-install, \
                 then run /init again to save them for this project",
            );
            return Vec::new();
        }
        if let Err(err) = self.mcp_store.export_to_project(&self.workdir) {
            self.chat
                .push_error(format!("failed to write {}: {err}", project_file.display()));
            return Vec::new();
        }
        self.chat.push_info(format!(
            "saved {} MCP integration(s) to {} — commit it so this project's setup travels with it. \
             Use /permissions auto (or bypass) to run hands-off with them.",
            self.mcp_store.servers.len(),
            project_file.display()
        ));
        Vec::new()
    }

    /// `/mcp-import` — review and adopt MCP integrations this project
    /// declares in `.agent/mcp.json` that aren't installed globally yet.
    /// Requires confirmation since a project file can point at arbitrary
    /// launch commands — never applied silently.
    fn run_mcp_import(&mut self) -> Vec<Effect> {
        let project = match McpStore::load_project(&self.workdir) {
            Ok(project) => project,
            Err(err) => {
                self.chat.push_error(format!(
                    "failed to read {}: {err}",
                    agentloop_cli_core::mcp_project_path(&self.workdir).display()
                ));
                return Vec::new();
            }
        };
        let existing: std::collections::HashSet<&str> = self
            .mcp_store
            .servers
            .iter()
            .map(|server| server.config.name.as_str())
            .collect();
        let new_servers: Vec<_> = project
            .servers
            .into_iter()
            .filter(|server| !existing.contains(server.config.name.as_str()))
            .collect();
        if new_servers.is_empty() {
            self.toast("nothing to import — up to date");
            return Vec::new();
        }
        let names = new_servers
            .iter()
            .map(|server| server.config.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        self.overlay = Overlay::Confirm(ConfirmPrompt {
            title: format!("Import {} MCP integration(s)?", new_servers.len()),
            message: format!(
                "This project declares: {names}. They'll run with whatever launch \
                 command is configured — only import from projects you trust."
            ),
            action: ConfirmAction::McpImport {
                servers: new_servers,
            },
        });
        Vec::new()
    }

    fn run_compact(&mut self) -> Vec<Effect> {
        if self.session.turn.is_running() {
            self.toast("turn in progress — esc to cancel first");
            return Vec::new();
        }
        self.begin_turn();
        self.toast("compacting session…");
        vec![Effect::CompactSession {
            opts: self.turn_options(),
        }]
    }

    fn toggle_thinking(&mut self) -> Vec<Effect> {
        if !self.caps.reasoning_visible {
            self.toast("this agent does not expose reasoning");
            return Vec::new();
        }
        self.show_thinking = !self.show_thinking;
        if self.show_thinking {
            self.toast("thinking visible (/thinking off to hide)");
        } else {
            self.toast("thinking hidden (/thinking on to show)");
        }
        self.persist_mode_prefs();
        Vec::new()
    }

    const CONNECT_USAGE: &'static str = "usage: /connect <name> <base_url> [api_key] [model] [--force] · \
         template form (name it anything): /connect <name> <openai|deepseek> <api_key> [model] · \
         key may be {env:VAR}; omit it for keyless local endpoints (LM Studio) · \
         a known id skips the URL: /connect deepseek <api_key> · \
         AWS Bedrock: /connect bedrock <api_key>";

    /// `/isolation [on|off|required]`: no arg shows status; an arg toggles
    /// workspace mode for future sessions (persisted; rebuilds the native
    /// service so the next `/new` honors it; the current session is unchanged).
    fn run_isolation(&mut self, arg: Option<String>) -> Vec<Effect> {
        let Some(arg) = arg.filter(|a| !a.trim().is_empty()) else {
            return vec![Effect::WorkspaceStatus];
        };
        let policy = match arg.trim().to_lowercase().as_str() {
            "on" | "optional" => agentloop_contracts::IsolationPolicy::Optional,
            "required" | "force" => agentloop_contracts::IsolationPolicy::Required,
            "off" | "never" => agentloop_contracts::IsolationPolicy::Never,
            other => {
                self.chat.push_error(format!(
                    "unknown isolation mode `{other}` — use `on`, `off`, or `required`"
                ));
                return Vec::new();
            }
        };
        if let Err(err) = agentloop_cli_core::CliPrefs::remember_isolation(policy) {
            self.chat
                .push_error(format!("could not save isolation preference: {err}"));
            return Vec::new();
        }
        if policy == agentloop_contracts::IsolationPolicy::Never {
            self.toast("workspace mode OFF — new sessions run directly in the project");
        } else {
            self.toast("workspace mode ON — run /new to start an isolated session");
        }
        vec![Effect::SetIsolation { policy }]
    }

    fn run_connect(&mut self, arg: Option<String>) -> Vec<Effect> {
        let Some(arg) = arg.filter(|a| !a.trim().is_empty()) else {
            self.open_connect_wizard();
            return Vec::new();
        };
        let mut tokens: Vec<&str> = arg.split_whitespace().collect();
        if tokens.first() == Some(&"remove") {
            return self.run_disconnect(tokens.get(1).map(|s| (*s).to_owned()));
        }
        let force = tokens.iter().position(|t| *t == "--force").map(|idx| {
            tokens.remove(idx);
        });
        if tokens.len() < 2 {
            self.chat.push_info(Self::CONNECT_USAGE);
            return Vec::new();
        }
        let id = tokens[0].to_lowercase();
        if !id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
            || id.is_empty()
        {
            self.chat
                .push_error(format!("invalid provider id `{id}` — use a-z, 0-9, -, _"));
            return Vec::new();
        }
        if self.providers.iter().any(|p| p == &id) {
            self.chat
                .push_error(format!("`{id}` is already a registered provider"));
            return Vec::new();
        }
        // First-party providers that aren't OpenAI-compatible (Bedrock) connect
        // by storing an API key the engine builds them from — not as a custom
        // OpenAI endpoint. Form: `/connect bedrock <api-key>`.
        if id == "bedrock" {
            return self.save_provider_key(&id, tokens[1]);
        }
        let second_is_url = tokens[1].contains("://");
        let config = if let Some((base_url, default_model, thinking)) =
            agentloop_cli_core::known_provider_defaults(&id).filter(|_| !second_is_url)
        {
            // (1) The id is itself a known provider: `/connect deepseek <key> [model]`.
            agentloop_cli_core::ProviderConfig {
                name: None,
                base_url: base_url.to_owned(),
                api_key: tokens[1].to_owned(),
                models: Vec::new(),
                default_model: Some(
                    tokens
                        .get(2)
                        .map(|s| (*s).to_owned())
                        .unwrap_or_else(|| default_model.to_owned()),
                ),
                thinking,
            }
        } else if let Some((base_url, default_model, thinking)) =
            agentloop_cli_core::known_provider_defaults(&tokens[1].to_lowercase())
                .filter(|_| !second_is_url)
        {
            // (2) Custom name + provider template: `/connect mygpt openai <key> [model]`.
            // The second token names a known template that supplies the base URL,
            // so any provider can be connected under an arbitrary name.
            agentloop_cli_core::ProviderConfig {
                name: None,
                base_url: base_url.to_owned(),
                api_key: tokens.get(2).map(|s| (*s).to_owned()).unwrap_or_default(),
                models: Vec::new(),
                default_model: Some(
                    tokens
                        .get(3)
                        .map(|s| (*s).to_owned())
                        .unwrap_or_else(|| default_model.to_owned()),
                ),
                thinking,
            }
        } else {
            // (3) Explicit base URL: `/connect <id> <base_url> [key] [model]`.
            agentloop_cli_core::ProviderConfig {
                name: None,
                base_url: tokens[1].to_owned(),
                // No third token = keyless local endpoint (LM Studio).
                api_key: tokens.get(2).map(|s| (*s).to_owned()).unwrap_or_default(),
                models: Vec::new(),
                default_model: tokens.get(3).map(|s| (*s).to_owned()),
                thinking: false,
            }
        };
        if force.is_some() {
            return self.save_provider(&id, config, None);
        }
        self.toast(format!("validating {id}…"));
        vec![Effect::ValidateProvider { id, config }]
    }

    /// Persist a validated (or forced) provider and rebuild the native engine.
    fn save_provider(
        &mut self,
        id: &str,
        config: agentloop_cli_core::ProviderConfig,
        model_count: Option<usize>,
    ) -> Vec<Effect> {
        self.dismiss_getting_started();
        if let Err(err) = agentloop_cli_core::CliPrefs::remember_provider(id, config) {
            self.chat.push_error(format!("could not save {id}: {err}"));
            return Vec::new();
        }
        match model_count {
            Some(count) => self.toast(format!("connected {id} ({count} models)")),
            None => self.toast(format!("saved {id} without validation")),
        }
        self.pending_provider = Some(id.to_owned());
        if self.connect_then_pick_model {
            self.connect_then_pick_model = false;
            self.awaiting_model_picker = true;
        }
        vec![Effect::ReloadEngine { invalidate: true }]
    }

    /// Persist an API key for a built-in provider (Bedrock) and rebuild the
    /// native engine, which picks the key up via `EngineOptions.provider_keys`.
    fn save_provider_key(&mut self, id: &str, key: &str) -> Vec<Effect> {
        if key.trim().is_empty() {
            self.chat.push_info(format!(
                "usage: /connect {id} <api-key>  (pick a model with /model)"
            ));
            return Vec::new();
        }
        self.dismiss_getting_started();
        if let Err(err) = agentloop_cli_core::CliPrefs::remember_provider_key(id, key) {
            self.chat.push_error(format!("could not save {id}: {err}"));
            return Vec::new();
        }
        self.toast(format!("connected {id}"));
        self.pending_provider = Some(id.to_owned());
        if self.connect_then_pick_model {
            self.connect_then_pick_model = false;
            self.awaiting_model_picker = true;
        }
        vec![Effect::ReloadEngine { invalidate: true }]
    }

    fn run_disconnect(&mut self, arg: Option<String>) -> Vec<Effect> {
        let Some(id) = arg.filter(|a| !a.trim().is_empty()) else {
            self.chat.push_info("usage: /disconnect <id>");
            return Vec::new();
        };
        match agentloop_cli_core::CliPrefs::forget_provider(id.trim()) {
            Ok(true) => {
                self.toast(format!("disconnected {}", id.trim()));
                vec![Effect::ReloadEngine { invalidate: true }]
            }
            // Not a custom OpenAI-compatible provider — maybe a built-in whose
            // API key was connected from the CLI (e.g. Bedrock).
            Ok(false) => match agentloop_cli_core::CliPrefs::forget_provider_key(id.trim()) {
                Ok(true) => {
                    self.toast(format!("disconnected {}", id.trim()));
                    vec![Effect::ReloadEngine { invalidate: true }]
                }
                Ok(false) => {
                    self.chat
                        .push_info(format!("no connected provider `{}`", id.trim()));
                    Vec::new()
                }
                Err(err) => {
                    self.chat.push_error(err.to_string());
                    Vec::new()
                }
            },
            Err(err) => {
                self.chat.push_error(err.to_string());
                Vec::new()
            }
        }
    }

    fn run_providers(&mut self) -> Vec<Effect> {
        let prefs = agentloop_cli_core::CliPrefs::load();
        let mut lines = vec!["providers:".to_owned()];
        for id in &self.providers {
            let custom = prefs.providers.get(id);
            match custom {
                Some(config) => lines.push(format!(
                    "  {id} · custom · {} · {} models",
                    config.base_url,
                    config.models.len()
                )),
                None => lines.push(format!("  {id} · built-in")),
            }
        }
        for (id, config) in &prefs.providers {
            if !self.providers.iter().any(|p| p == id) {
                lines.push(format!(
                    "  {id} · custom · {} · not loaded (rebuild pending or env key missing)",
                    config.base_url
                ));
            }
        }
        lines.push("add: /connect · remove: /disconnect <id>".to_owned());
        for line in lines {
            self.chat.push_info(line);
        }
        Vec::new()
    }

    fn run_roles(&mut self) -> Vec<Effect> {
        let prefs = agentloop_cli_core::CliPrefs::load();
        for line in role_lines(&prefs) {
            self.chat.push_info(line);
        }
        // Surface the automatic DeepSeek split (applied in-memory at engine
        // build) so `/roles` doesn't misleadingly show searcher/worker as
        // inheriting when DeepSeek is active.
        let on_deepseek = self
            .session
            .model
            .as_ref()
            .is_some_and(|m| m.0.starts_with("deepseek/"));
        if on_deepseek
            && !prefs.roles.contains_key("searcher")
            && !prefs.roles.contains_key("worker")
        {
            self.chat.push_info(
                "auto (DeepSeek active): searcher → deepseek-v4-flash · worker → deepseek-v4-pro \
                 (in-memory; run /roles preset deepseek to pin it)",
            );
        }
        Vec::new()
    }

    /// `/roles preset <name>` — apply a named role/model preset. Only
    /// `deepseek` today: research subagents (`searcher`) → `deepseek-v4-flash`,
    /// implementation (`worker`) and the main/planner session → `deepseek-v4-pro`
    /// (the DeepSeek analog of Claude Code's Haiku-search / Sonnet-work split).
    fn run_roles_preset(&mut self, name: &str) -> Vec<Effect> {
        if self.session.turn.is_running() {
            self.toast("turn in progress — esc to cancel first");
            return Vec::new();
        }
        match name.trim() {
            "deepseek" => {
                if let Err(err) = CliPrefs::apply_deepseek_roles_preset() {
                    self.chat
                        .push_error(format!("failed to save preset: {err}"));
                    return Vec::new();
                }
                self.chat.push_info(
                    "DeepSeek auto preset: planner + worker → deepseek-v4-pro, \
                     research (searcher) → deepseek-v4-flash. Needs DEEPSEEK_API_KEY \
                     or a `deepseek` provider.",
                );
                // Put the main/planner session on the strong model, then reload
                // so the new role model chains take effect.
                let mut effects = self.set_model(ModelRef::from("deepseek/deepseek-v4-pro"));
                effects.push(Effect::ReloadEngine { invalidate: true });
                effects
            }
            other => {
                self.chat.push_info(format!(
                    "unknown roles preset `{other}` (available: deepseek)"
                ));
                Vec::new()
            }
        }
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
                self.toast("thinking visible");
                self.persist_mode_prefs();
                Vec::new()
            }
            Some("off") => {
                self.thinking_budget = None;
                self.persist_thinking_budget();
                self.toast("thinking off");
                Vec::new()
            }
            Some("hide") => {
                self.show_thinking = false;
                self.toast("thinking hidden");
                self.persist_mode_prefs();
                Vec::new()
            }
            Some("low") => self.set_thinking_budget(4096),
            Some("medium") => self.set_thinking_budget(12_288),
            Some("high") => self.set_thinking_budget(32_768),
            Some(text) => {
                if let Ok(budget) = text.parse::<u32>() {
                    if budget == 0 {
                        self.thinking_budget = None;
                        self.persist_thinking_budget();
                        self.toast("thinking off");
                        return Vec::new();
                    }
                    return self.set_thinking_budget(budget);
                }
                self.chat.push_error(format!(
                    "unknown /thinking value `{text}` (use off, low, medium, high, or a token count)"
                ));
                Vec::new()
            }
        }
    }

    fn set_thinking_budget(&mut self, budget: u32) -> Vec<Effect> {
        self.thinking_budget = Some(budget);
        self.persist_thinking_budget();
        self.toast(format!(
            "thinking budget set to {}",
            crate::ui::fmt_thinking_budget_k(budget)
        ));
        Vec::new()
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

    fn apply_permission_picker_id(&mut self, id: &str) -> Vec<Effect> {
        match id {
            "require" => self.set_permission_mode(PermissionMode::Default),
            "auto" => self.set_permission_mode(PermissionMode::AcceptEdits),
            "allow-all" => {
                self.confirm_allow_all();
                Vec::new()
            }
            other => self.apply_permission_arg(other),
        }
    }

    fn apply_permission_arg(&mut self, text: &str) -> Vec<Effect> {
        match parse_permission_arg(text) {
            Some(PermissionMode::BypassPermissions) => {
                self.confirm_allow_all();
                Vec::new()
            }
            Some(mode) => self.set_permission_mode(mode),
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
        self.toast(format!("session mode set to {}", session_mode_label(mode)));
        self.persist_mode_prefs();
    }

    /// Parse and apply an effort level from a command/flag/picker argument.
    /// Public so startup can apply the `--effort` flag; an unknown value shows
    /// an error and keeps the current level.
    pub fn apply_effort_arg(&mut self, text: &str) {
        match parse_effort_level(text) {
            Some(level) => self.set_effort(level),
            None => self.chat.push_error(format!(
                "unknown effort `{text}` (use low, medium, high, xhigh, or max)"
            )),
        }
    }

    fn set_effort(&mut self, level: Effort) {
        self.session.effort = level;
        self.toast(format!("effort set to {}", effort_label(level)));
        if let Err(err) = CliPrefs::remember_effort(effort_label(level)) {
            tracing::warn!(target: "prefs", "failed to save effort preference: {err}");
        }
    }

    fn open_effort_picker(&mut self) {
        let current = self.session.effort;
        let items = [
            (Effort::Low, "fastest; minimal reasoning, terse"),
            (Effort::Medium, "balanced speed and rigor"),
            (Effort::High, "think, read, verify (default)"),
            (Effort::XHigh, "broad exploration; parallel + reviewer"),
            (Effort::Max, "exhaustive; cross-verify everything"),
        ]
        .into_iter()
        .map(|(level, detail)| PickerItem {
            id: effort_label(level).to_owned(),
            label: effort_label(level).to_owned(),
            detail: if level == current {
                Some("current".to_owned())
            } else {
                Some(detail.to_owned())
            },
            enabled: true,
        })
        .collect();
        self.overlay = Overlay::Picker(PickerState::new("effort", items, PickerAction::SetEffort));
    }

    fn set_permission_mode(&mut self, mode: PermissionMode) -> Vec<Effect> {
        if !self.caps.permissions.modes.contains(&mode) {
            self.chat.push_error(format!(
                "agent does not support {} mode",
                permission_mode_label(mode)
            ));
            return Vec::new();
        }
        self.session.permission_mode = mode;
        self.toast(format!(
            "permissions set to {}",
            permission_mode_label(mode)
        ));
        self.persist_mode_prefs();
        let mut effects = self.sync_turn_permission_mode();
        if mode == PermissionMode::BypassPermissions {
            effects.extend(self.grant_pending_permissions_for_bypass());
        }
        effects
    }

    /// Shift+Tab cycle: (code, require) → (code, accept-edits) → plan → back.
    /// Bypass is never in the cycle — only `/permissions allow-all` reaches it.
    fn cycle_ui_mode(&mut self) -> Vec<Effect> {
        if self.session.session_mode == SessionMode::Plan {
            self.set_session_mode(SessionMode::Code);
            return self.set_permission_mode(PermissionMode::Default);
        }
        if self.session.permission_mode == PermissionMode::Default
            && self
                .caps
                .permissions
                .modes
                .contains(&PermissionMode::AcceptEdits)
        {
            return self.set_permission_mode(PermissionMode::AcceptEdits);
        }
        self.set_session_mode(SessionMode::Plan);
        self.sync_turn_permission_mode()
    }

    fn sync_turn_permission_mode(&self) -> Vec<Effect> {
        if self.kind != AgentKind::Native {
            return Vec::new();
        }
        vec![Effect::SetTurnPermissionMode {
            mode: Some(self.session.effective_permission_mode()),
        }]
    }

    fn grant_pending_permissions_for_bypass(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        if let Overlay::Permission(prompt) = &self.overlay {
            effects.push(Effect::RespondPermission {
                id: prompt.id.clone(),
                decision: PermissionDecision::AllowOnce,
                session: prompt.session.clone(),
            });
            self.overlay = Overlay::None;
        }
        for prompt in self.pending_permissions.drain(..) {
            effects.push(Effect::RespondPermission {
                id: prompt.id,
                decision: PermissionDecision::AllowOnce,
                session: prompt.session,
            });
        }
        self.drain_pending();
        effects
    }

    fn confirm_allow_all(&mut self) {
        if self.session.permission_mode == PermissionMode::BypassPermissions {
            self.toast("permissions already allow-all");
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

    /// Open the theme picker. Snapshots the active theme so cancelling (Esc)
    /// or moving off a live-previewed row restores it.
    fn open_theme_picker(&mut self) {
        self.saved_theme = Some(theme::active());
        let items: Vec<PickerItem> = BuiltinTheme::all()
            .iter()
            .map(|builtin| PickerItem {
                id: builtin.id().to_owned(),
                label: builtin.id().to_owned(),
                detail: (builtin.id() == self.theme_id).then(|| "current".to_owned()),
                enabled: true,
            })
            .collect();
        let mut picker = PickerState::new("theme", items, PickerAction::SetTheme);
        if let Some(idx) = picker
            .items
            .iter()
            .position(|item| item.id == self.theme_id)
        {
            picker.selected = idx;
        }
        self.overlay = Overlay::Picker(picker);
        self.dirty = true;
    }

    /// Apply a theme live without persisting (theme-picker highlight moved).
    fn preview_theme(&mut self, id: &str) {
        if let Some(builtin) = BuiltinTheme::from_id(id) {
            theme::set_active(builtin.resolve(self.truecolor));
            self.markdown_cache.clear();
            self.dirty = true;
        }
    }

    /// Restore the theme snapshotted when the picker opened (picker cancelled).
    fn revert_theme(&mut self) {
        if let Some(saved) = self.saved_theme.take() {
            theme::set_active(saved);
            self.markdown_cache.clear();
            self.dirty = true;
        }
    }

    /// Commit a theme by id: activate, remember the choice, and persist it.
    fn apply_theme(&mut self, id: &str) -> Vec<Effect> {
        let Some(builtin) = BuiltinTheme::from_id(id.trim()) else {
            let names = BuiltinTheme::all()
                .iter()
                .map(|builtin| builtin.id())
                .collect::<Vec<_>>()
                .join(", ");
            self.chat
                .push_info(format!("unknown theme `{}` — try: {names}", id.trim()));
            return Vec::new();
        };
        theme::set_active(builtin.resolve(self.truecolor));
        self.theme_id = builtin.id().to_owned();
        self.saved_theme = None;
        self.markdown_cache.clear();
        self.dirty = true;
        if let Err(err) = CliPrefs::remember_theme(builtin.id()) {
            tracing::warn!(target: "prefs", "failed to save theme preference: {err}");
        }
        self.toast(format!("theme: {}", builtin.id()));
        Vec::new()
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
            self.toast("nothing to copy");
            return Vec::new();
        }
        // Success is toasted by the runtime once the clipboard write lands.
        vec![Effect::CopyToClipboard { text }]
    }

    fn toggle_mouse_capture(&mut self) -> Vec<Effect> {
        self.mouse_capture = !self.mouse_capture;
        if let Err(err) = CliPrefs::remember_mouse_capture(self.mouse_capture) {
            tracing::warn!(target: "prefs", "failed to save mouse-capture preference: {err}");
        }
        self.toast(if self.mouse_capture {
            "wheel scrolls transcript · drag with ⌥/Fn/Shift to select (saved)"
        } else {
            "drag selects text · wheel may scroll terminal · Ctrl+M switches back (saved)"
        });
        vec![Effect::SetMouseCapture(self.mouse_capture)]
    }

    fn set_model(&mut self, model: ModelRef) -> Vec<Effect> {
        // `<provider>/auto` (or bare `auto` for the active provider) is a UX
        // shortcut for that provider's smart-orchestration preset, not a
        // literal model id — route it before any real-model handling.
        if model.0 == "auto" {
            // Bare `auto` needs an active provider to resolve against; without
            // one there's nothing to key off, so reject it instead of saving
            // the literal id "auto" (which would persist and reload as an
            // invalid model).
            return match self.current_provider() {
                Some(provider) => self.apply_auto_model(&provider),
                None => {
                    self.chat.push_info(
                        "no active provider — use `/model <provider>/auto` or pick a provider first",
                    );
                    Vec::new()
                }
            };
        }
        if let Some(provider) = model.0.strip_suffix("/auto") {
            return self.apply_auto_model(provider);
        }
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
        self.toast(format!("model set to {model}"));
        self.session.model = Some(model.clone());
        vec![Effect::SaveLastModel(model)]
    }

    /// Apply a provider's "auto" smart-orchestration mode (selected from the
    /// model picker or `/model <provider>/auto`). Currently only DeepSeek has
    /// one — it reuses the `deepseek` roles preset (v4-pro plans & builds,
    /// v4-flash researches).
    fn apply_auto_model(&mut self, provider: &str) -> Vec<Effect> {
        match provider {
            "deepseek" => self.run_roles_preset("deepseek"),
            other => {
                self.chat.push_info(format!(
                    "`{other}` has no auto mode — pick a specific model"
                ));
                Vec::new()
            }
        }
    }

    fn start_copilot_login(&mut self, pending: PendingCopilotAuth) -> Vec<Effect> {
        self.pending_copilot_auth = Some(pending);
        self.overlay = Overlay::Login(LoginState::Starting);
        vec![Effect::StartLogin]
    }

    fn resume_after_copilot_login(&mut self, pending: PendingCopilotAuth) -> Vec<Effect> {
        match pending {
            PendingCopilotAuth::SwitchAgent(kind) => {
                self.toast(format!("switching to {kind}…"));
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
                    self.toast("fetching models…");
                    vec![Effect::ListModels]
                } else {
                    self.open_catalog_picker();
                    Vec::new()
                }
            }
        }
    }

    fn open_catalog_picker(&mut self) {
        let current = self.current_provider();
        let mut items: Vec<PickerItem> = Vec::new();
        let prefs = CliPrefs::load();
        if self.catalog.is_empty() || prefs.recent_models.is_empty() {
            // no section headers when there's nothing to section
        } else {
            let recent: Vec<PickerItem> = prefs
                .recent_models
                .iter()
                .filter_map(|stored| {
                    let model = ModelRef(stored.clone());
                    if !model_provider_available(&model, &self.providers) {
                        return None;
                    }
                    if let Some(provider) = current.as_deref() {
                        let (entry_provider, _) = model.split();
                        if entry_provider != Some(provider) {
                            return None;
                        }
                    }
                    Some(catalog_item_from_ref(&model, self.session.model.as_ref()))
                })
                .collect();
            if !recent.is_empty() {
                items.push(PickerItem {
                    id: "__section_recent".to_owned(),
                    label: "Recent".to_owned(),
                    detail: None,
                    enabled: false,
                });
                items.extend(recent);
                items.push(PickerItem {
                    id: "__section_all".to_owned(),
                    label: "All".to_owned(),
                    detail: None,
                    enabled: false,
                });
            }
        }
        // First row: the provider's "auto" smart-orchestration mode, if it has
        // one (DeepSeek). Offered by provider name, independent of the fetched
        // catalog, so it shows even if listing failed.
        if let Some(provider) = current.as_deref() {
            if let Some(detail) = auto_mode_detail(provider) {
                items.push(PickerItem {
                    id: format!("{provider}/auto"),
                    label: "auto".to_owned(),
                    detail: Some(detail.to_owned()),
                    enabled: true,
                });
            }
        }
        // Scope the model list to the active provider so, e.g., a DeepSeek
        // session doesn't list Copilot/Anthropic models. Fall back to the full
        // catalog when nothing matches (no active provider, or it listed none).
        let scoped: Vec<&CatalogEntry> = match current.as_deref() {
            Some(provider) => self
                .catalog
                .iter()
                .filter(|entry| entry.provider.as_str() == provider)
                .collect(),
            None => self.catalog.iter().collect(),
        };
        let entries = if scoped.is_empty() {
            self.catalog.iter().collect::<Vec<_>>()
        } else {
            scoped
        };
        items.extend(entries.into_iter().map(catalog_item));
        let title = match current.as_deref() {
            Some(provider) => format!("select model · {provider}"),
            None => "select model".to_owned(),
        };
        self.overlay = Overlay::Picker(PickerState::new(title, items, PickerAction::SetModel));
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
        let items = AgentKind::selectable()
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

    fn open_command_palette(&mut self) -> Vec<Effect> {
        let entries = self.build_command_palette();
        self.overlay = Overlay::CommandPalette(CommandPaletteState {
            entries,
            filter: String::new(),
            selected: 0,
        });
        Vec::new()
    }

    fn build_command_palette(&self) -> Vec<CommandPaletteEntry> {
        let mut entries = Vec::new();
        if self.providers.is_empty() {
            entries.push(CommandPaletteEntry {
                title: "/connect".to_owned(),
                description: "connect an LLM provider".to_owned(),
                category: "suggested",
                section_header: Some("Suggested".to_owned()),
                key_hint: None,
                action: CommandPaletteAction::Local(LocalCommand::Connect { arg: None }),
            });
        }
        entries.push(CommandPaletteEntry {
            title: "/model".to_owned(),
            description: "pick or set the session model".to_owned(),
            category: "suggested",
            section_header: if self.providers.is_empty() {
                None
            } else {
                Some("Suggested".to_owned())
            },
            key_hint: None,
            action: CommandPaletteAction::OpenModelPicker,
        });
        entries.push(CommandPaletteEntry {
            title: "/sessions".to_owned(),
            description: "resume a past session".to_owned(),
            category: "suggested",
            section_header: None,
            key_hint: None,
            action: CommandPaletteAction::OpenSessionsPicker,
        });
        entries.push(CommandPaletteEntry {
            title: "/help".to_owned(),
            description: "keys and commands".to_owned(),
            category: "suggested",
            section_header: None,
            key_hint: None,
            action: CommandPaletteAction::Local(LocalCommand::Help),
        });
        for entry in self.commands.entries() {
            let title = format!("/{}", entry.name);
            let key_hint = entry.args_hint.clone();
            let action = if entry.source == "cli" {
                match self.commands.route(&title) {
                    Route::Local(command) => CommandPaletteAction::Local(command),
                    Route::Engine => CommandPaletteAction::EngineCommand(entry.name.clone()),
                    Route::Plain => continue,
                }
            } else {
                CommandPaletteAction::EngineCommand(entry.name.clone())
            };
            entries.push(CommandPaletteEntry {
                title,
                description: entry.description.clone(),
                category: entry.category,
                section_header: None,
                key_hint,
                action,
            });
        }
        entries.extend([
            CommandPaletteEntry {
                title: "Open model picker".to_owned(),
                description: "browse models for the active provider".to_owned(),
                category: "action",
                section_header: Some("Actions".to_owned()),
                key_hint: Some("/model".to_owned()),
                action: CommandPaletteAction::OpenModelPicker,
            },
            CommandPaletteEntry {
                title: "Open provider picker".to_owned(),
                description: "switch LLM provider".to_owned(),
                category: "action",
                section_header: None,
                key_hint: Some("/provider".to_owned()),
                action: CommandPaletteAction::OpenProviderPicker,
            },
            CommandPaletteEntry {
                title: "Open agent picker".to_owned(),
                description: "switch agent implementation".to_owned(),
                category: "action",
                section_header: None,
                key_hint: Some("/agent".to_owned()),
                action: CommandPaletteAction::OpenAgentPicker,
            },
            CommandPaletteEntry {
                title: "Resume past session".to_owned(),
                description: "pick from saved sessions".to_owned(),
                category: "action",
                section_header: None,
                key_hint: Some("/sessions".to_owned()),
                action: CommandPaletteAction::OpenSessionsPicker,
            },
            CommandPaletteEntry {
                title: "Scroll to bottom".to_owned(),
                description: "follow live transcript".to_owned(),
                category: "action",
                section_header: None,
                key_hint: Some("End".to_owned()),
                action: CommandPaletteAction::ScrollToBottom,
            },
            CommandPaletteEntry {
                title: "Toggle thinking blocks".to_owned(),
                description: "show or hide reasoning".to_owned(),
                category: "action",
                section_header: None,
                key_hint: Some("Ctrl+T".to_owned()),
                action: CommandPaletteAction::ToggleThinking,
            },
            CommandPaletteEntry {
                title: "Copy transcript".to_owned(),
                description: "copy chat to clipboard".to_owned(),
                category: "action",
                section_header: None,
                key_hint: Some("Ctrl+Shift+C".to_owned()),
                action: CommandPaletteAction::CopyTranscript,
            },
            CommandPaletteEntry {
                title: "Cycle permission mode".to_owned(),
                description: "shift through security levels".to_owned(),
                category: "action",
                section_header: None,
                key_hint: Some("Shift+Tab".to_owned()),
                action: CommandPaletteAction::CyclePermissionMode,
            },
            CommandPaletteEntry {
                title: "Toggle mouse capture".to_owned(),
                description: "wheel scroll vs text selection".to_owned(),
                category: "action",
                section_header: None,
                key_hint: Some("Ctrl+M".to_owned()),
                action: CommandPaletteAction::ToggleMouseCapture,
            },
        ]);
        entries
    }

    fn apply_palette_action(&mut self, action: CommandPaletteAction) -> Vec<Effect> {
        match action {
            CommandPaletteAction::Local(command) => self.run_local(command),
            CommandPaletteAction::EngineCommand(name) => self.submit_prompt(&format!("/{name}")),
            CommandPaletteAction::OpenModelPicker => self.open_model_picker(),
            CommandPaletteAction::OpenProviderPicker => self.open_provider_picker(),
            CommandPaletteAction::OpenAgentPicker => {
                self.open_agent_picker();
                Vec::new()
            }
            CommandPaletteAction::OpenSessionsPicker => self.open_sessions_picker(),
            CommandPaletteAction::ScrollToBottom => {
                self.chat.scroll.scroll_to_bottom();
                Vec::new()
            }
            CommandPaletteAction::ToggleThinking => self.run_thinking_command(None),
            CommandPaletteAction::CopyTranscript => self.copy_chat(),
            CommandPaletteAction::CyclePermissionMode => self.cycle_ui_mode(),
            CommandPaletteAction::ToggleMouseCapture => self.toggle_mouse_capture(),
        }
    }

    fn open_sessions_picker(&mut self) -> Vec<Effect> {
        self.toast("loading sessions…");
        vec![Effect::ListSessions]
    }

    fn open_sessions_picker_with(&mut self, summaries: Vec<agentloop_cli_core::SessionSummary>) {
        if summaries.is_empty() {
            self.chat
                .push_info("no saved sessions for this directory yet");
            return;
        }
        let current = self.session.id.0.clone();
        let items = summaries
            .into_iter()
            .map(|summary| {
                let label = session_display_label(&summary).to_owned();
                let when = format_relative_time(summary.updated_at_ms);
                let is_current = summary.id.0 == current;
                PickerItem {
                    id: summary.id.0,
                    label,
                    detail: Some(if is_current {
                        format!("{when} · current")
                    } else {
                        when
                    }),
                    enabled: !is_current,
                }
            })
            .collect();
        self.overlay = Overlay::Picker(PickerState::new(
            "sessions",
            items,
            PickerAction::ResumeSession,
        ));
    }

    fn open_connect_wizard(&mut self) {
        self.dismiss_getting_started();
        self.overlay = Overlay::ConnectWizard(ConnectWizardState::new_gallery());
        self.skip_connect_gallery_headers();
    }

    /// Auto-open the connect gallery once on startup when no providers exist.
    pub fn open_connect_gallery_on_startup(&mut self) {
        self.overlay = Overlay::ConnectWizard(ConnectWizardState::new_gallery());
        self.skip_connect_gallery_headers();
    }

    fn skip_connect_gallery_headers(&mut self) {
        if let Overlay::ConnectWizard(state) = &mut self.overlay {
            let rows = state.gallery_rows();
            if let Some(idx) = rows
                .iter()
                .position(|row| !matches!(row, crate::overlay::ConnectGalleryRow::Header(_)))
            {
                state.selected = idx;
            }
        }
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
                self.toast("fetching models…");
                vec![Effect::ListModels]
            }
        }
    }

    fn switch_agent(&mut self, id: &str) -> Vec<Effect> {
        match AgentKind::parse(id) {
            Some(kind) if kind == self.kind => {
                self.toast(format!("already on {kind}"));
                Vec::new()
            }
            Some(kind) if kind != AgentKind::Native && !delegated_agents_enabled() => {
                let alternative = match kind {
                    AgentKind::Copilot => {
                        " — try `/provider copilot` for the same model via the stable native loop"
                    }
                    _ => "",
                };
                self.chat.push_info(format!(
                    "{kind} is disabled for now (native-only mode){alternative}. \
                     Set FLEX_ENABLE_DELEGATED_AGENTS=1 to re-enable it."
                ));
                Vec::new()
            }
            Some(AgentKind::Copilot) if !has_copilot_credentials() => {
                self.start_copilot_login(PendingCopilotAuth::SwitchAgent(AgentKind::Copilot))
            }
            Some(kind) => {
                self.pending_switch_kind = Some(kind);
                self.toast(format!("switching to {kind}…"));
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
                self.begin_turn();
                Vec::new()
            }
            AgentEvent::MessageStarted { role, .. } => {
                if *role == agentloop_contracts::Role::Assistant {
                    self.begin_turn();
                }
                self.chat.apply(&event.payload);
                Vec::new()
            }
            AgentEvent::MarkdownDelta { text, .. } | AgentEvent::ThinkingDelta { text, .. } => {
                self.begin_turn();
                self.status.turn_output_chars += text.len() as u64;
                self.chat.apply(&event.payload);
                Vec::new()
            }
            AgentEvent::AssistantMessage { usage, .. } => {
                // Snap the approximate live counter to reported usage.
                if let Some(usage) = usage {
                    self.status.turn_output_chars = self
                        .status
                        .turn_output_chars
                        .max(usage.output.saturating_mul(4));
                }
                self.chat.apply(&event.payload);
                Vec::new()
            }
            AgentEvent::TurnCompleted { summary, .. } => {
                self.session.turn = TurnPhase::Idle;
                self.status.total_usage.add(&summary.usage);
                self.status.last_context_tokens =
                    Some(summary.usage.input + summary.usage.cache_read.unwrap_or(0));
                if summary.cost_usd.is_some() {
                    self.status.last_cost_usd = summary.cost_usd;
                }
                self.chat.finalize_drafts();
                match summary.stop_reason {
                    TurnStopReason::Cancelled => self.chat.push_info(INTERRUPT_NOTE),
                    TurnStopReason::MaxIterations => self.chat.push_info(MAX_ITERATIONS_NOTE),
                    _ => {}
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
                self.queue_permission(make_permission_prompt(
                    id.clone(),
                    call_id.clone(),
                    title.clone(),
                    detail.clone(),
                    options.clone(),
                    None,
                    None,
                ));
                Vec::new()
            }
            AgentEvent::PermissionResolved { id, .. } => {
                self.clear_permission(id);
                Vec::new()
            }
            AgentEvent::QuestionRequested { id, questions } => {
                self.queue_question(QuestionPrompt::new(id.clone(), questions.clone()));
                Vec::new()
            }
            AgentEvent::QuestionResolved { id, .. } => {
                self.clear_question(id);
                Vec::new()
            }
            AgentEvent::SubagentEvent {
                child_session,
                event: inner,
            } => {
                // Relayed child control-plane events surface in the parent
                // TUI tagged with the child session and its role badge; the
                // answer routes back to the child (Effect session field).
                match inner.as_ref() {
                    AgentEvent::PermissionRequested {
                        id,
                        call_id,
                        title,
                        detail,
                        options,
                    } => {
                        self.queue_permission(make_permission_prompt(
                            id.clone(),
                            call_id.clone(),
                            title.clone(),
                            detail.clone(),
                            options.clone(),
                            Some(child_session.clone()),
                            self.chat.subagent_role(child_session),
                        ));
                    }
                    // Covers both user answers and the engine's ask_timeout
                    // auto-deny — a relayed prompt never dangles.
                    AgentEvent::PermissionResolved { id, .. } => self.clear_permission(id),
                    AgentEvent::QuestionRequested { id, questions } => {
                        let mut prompt = QuestionPrompt::new(id.clone(), questions.clone());
                        prompt.session = Some(child_session.clone());
                        prompt.role = self.chat.subagent_role(child_session);
                        self.queue_question(prompt);
                    }
                    AgentEvent::QuestionResolved { id, .. } => self.clear_question(id),
                    _ => {}
                }
                self.chat.apply(&event.payload);
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
            AgentEvent::ToolCallUpdated { .. } => {
                self.chat.apply(&event.payload);
                Vec::new()
            }
            AgentEvent::CompactionBoundary { summary } => {
                self.chat.apply(&event.payload);
                if summary.strategy.starts_with("auto_") {
                    let savings = match (summary.tokens_before, summary.tokens_after) {
                        (Some(before), Some(after)) if before > after => {
                            format!(" (~{before} → ~{after} tokens)")
                        }
                        _ => String::new(),
                    };
                    self.toast(format!(
                        "Auto-compacted context{savings} — approaching limit"
                    ));
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

    /// Show a permission prompt, or queue it while another modal is open.
    fn queue_permission(&mut self, prompt: PermissionPrompt) {
        if self.overlay.is_active() {
            self.pending_permissions.push_back(prompt);
        } else {
            self.overlay = Overlay::Permission(prompt);
        }
    }

    /// Drop a permission prompt (answered or auto-denied) wherever it lives.
    fn clear_permission(&mut self, id: &PermissionRequestId) {
        self.pending_permissions.retain(|p| &p.id != id);
        if matches!(&self.overlay, Overlay::Permission(p) if &p.id == id) {
            self.overlay = Overlay::None;
        }
        self.drain_pending();
    }

    /// Show a question prompt, or queue it while another modal is open.
    fn queue_question(&mut self, prompt: QuestionPrompt) {
        if self.overlay.is_active() {
            self.pending_questions.push_back(prompt);
        } else {
            self.overlay = Overlay::Question(prompt);
        }
    }

    /// Drop a question prompt (answered elsewhere) wherever it lives.
    fn clear_question(&mut self, id: &QuestionId) {
        self.pending_questions.retain(|q| &q.id != id);
        if matches!(&self.overlay, Overlay::Question(q) if &q.id == id) {
            self.overlay = Overlay::None;
        }
        self.drain_pending();
    }

    fn apply_fresh_session(&mut self, id: SessionId, banner: &str) {
        self.session.id = id;
        self.session.last_seq = 0;
        self.session.turn = TurnPhase::Idle;
        self.chat = ChatState::default();
        self.markdown_cache.clear();
        self.overlay = Overlay::None;
        self.pending_permissions.clear();
        self.pending_questions.clear();
        self.status.toasts.clear();
        self.status.turn_output_chars = 0;
        let name = self.engine_name.clone();
        let version = self.engine_version.clone();
        let cwd = self.workdir.display().to_string();
        self.chat.push_splash(name, version, cwd);
        self.toast(banner.to_owned());
    }

    // ── task results ────────────────────────────────────────────────────────

    fn on_task(&mut self, result: TaskResult) -> Vec<Effect> {
        match result {
            TaskResult::TurnFinished(outcome) => {
                self.session.turn = TurnPhase::Idle;
                if let Err(message) = outcome {
                    // Turn failures render from the event stream; this toast
                    // is only a fallback signal (e.g. TurnInProgress).
                    self.toast(message);
                }
                if let Some(next) = self.queued_prompts.pop_front() {
                    self.toast(format!("sending queued prompt ({} left)", {
                        self.queued_prompts.len()
                    }));
                    return self.submit_prompt(&next);
                }
                Vec::new()
            }
            TaskResult::CompactFinished(outcome) => {
                self.session.turn = TurnPhase::Idle;
                if let Err(message) = outcome {
                    self.toast(message);
                }
                Vec::new()
            }
            TaskResult::ProviderValidated { id, config, result } => match result {
                Ok(count) => {
                    self.connect_then_pick_model = true;
                    self.save_provider(&id, config, Some(count))
                }
                Err(message) => {
                    self.connect_then_pick_model = true;
                    self.overlay = Overlay::Confirm(ConfirmPrompt {
                        title: format!("Could not reach `{id}`"),
                        message: format!(
                            "{message}\n\nSave this provider anyway? Chat may fail until the endpoint works."
                        ),
                        action: ConfirmAction::SaveProviderAnyway { id, config },
                    });
                    Vec::new()
                }
            },
            TaskResult::Models(Ok(entries)) => {
                self.catalog = entries;
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
                self.toast(format!("model listing failed: {message}"));
                Vec::new()
            }
            TaskResult::EngineSwitched(outcome) => match *outcome {
                Ok(bootstrap) => {
                    self.pending_switch_kind = None;
                    let pending_model = self.pending_model.take();
                    let pending_provider = self.pending_provider.take();
                    // Snapshot the conversation before the switch wipes it, so
                    // the next prompt carries context to the new agent.
                    self.capture_carry_context();
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
                    if self.pending_switch_kind == Some(AgentKind::Copilot)
                        && has_copilot_credentials()
                    {
                        self.toast(
                            "Use /provider copilot for the Copilot API (no CLI install needed)",
                        );
                    }
                    self.pending_switch_kind = None;
                    self.chat.push_error(format!("switch failed: {message}"));
                    Vec::new()
                }
            },
            TaskResult::SessionReset(Ok(id)) => {
                self.apply_fresh_session(id, "new session");
                Vec::new()
            }
            TaskResult::SessionReset(Err(message)) => {
                self.chat
                    .push_error(format!("new session failed: {message}"));
                Vec::new()
            }
            TaskResult::SessionCleared(Ok(id)) => {
                self.apply_fresh_session(id, "chat cleared");
                Vec::new()
            }
            TaskResult::SessionCleared(Err(message)) => {
                self.chat.push_error(format!("clear failed: {message}"));
                Vec::new()
            }
            TaskResult::Resynced(Ok(transcript)) => {
                self.chat.rebuild_from_transcript(&transcript);
                Vec::new()
            }
            TaskResult::Resynced(Err(message)) => {
                self.toast(format!("resync failed: {message}"));
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
                    self.toast("reloading providers…");
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
            TaskResult::McpInstallFinished(result) => match result {
                Ok(name) => {
                    self.mcp_store = McpStore::load();
                    self.toast(format!("installed MCP server `{name}`"));
                    if self.kind == AgentKind::Native {
                        vec![Effect::ReloadEngine { invalidate: true }]
                    } else {
                        Vec::new()
                    }
                }
                Err(message) => {
                    self.toast(message);
                    Vec::new()
                }
            },
            TaskResult::McpToolsListed { server, result } => {
                self.on_mcp_tools_listed(&server, result);
                Vec::new()
            }
            TaskResult::McpToolCalled {
                server,
                tool,
                result,
            } => {
                self.on_mcp_tool_called(&server, &tool, result);
                Vec::new()
            }
            TaskResult::PermissionRespondFailed { message } => {
                self.toast(format!("permission response failed: {message}"));
                self.chat.push_error(
                    "could not deliver permission decision — press Esc to cancel the turn"
                        .to_owned(),
                );
                Vec::new()
            }
            TaskResult::EngineReloaded(outcome) => match *outcome {
                Ok(bootstrap) => {
                    self.mcp_enabled = bootstrap.mcp_enabled;
                    if self.kind == AgentKind::Native {
                        let model = self.session.model.clone();
                        let restarted = bootstrap.session_restarted;
                        self.install_bootstrap(bootstrap, false);
                        self.session.model = model;
                        if restarted {
                            self.toast("session restarted after MCP reload");
                        } else {
                            self.toast("MCP servers reloaded");
                        }
                    }
                    Vec::new()
                }
                Err(message) => {
                    self.chat
                        .push_error(format!("engine reload failed: {message}"));
                    Vec::new()
                }
            },
            TaskResult::WorkspaceIntegrated(Ok(message)) => {
                self.chat.push_info(message);
                Vec::new()
            }
            TaskResult::WorkspaceIntegrated(Err(message)) => {
                self.chat.push_error(message);
                Vec::new()
            }
            TaskResult::WorkspaceDiscarded(Ok(message)) => {
                self.chat.push_info(message);
                Vec::new()
            }
            TaskResult::WorkspaceDiscarded(Err(message)) => {
                self.chat.push_error(message);
                Vec::new()
            }
            TaskResult::WorkspaceStatusReported(Ok(message)) => {
                self.chat.push_info(message);
                Vec::new()
            }
            TaskResult::WorkspaceStatusReported(Err(message)) => {
                self.chat.push_error(message);
                Vec::new()
            }
            TaskResult::SessionsListed(summaries) => {
                self.open_sessions_picker_with(summaries);
                Vec::new()
            }
            TaskResult::SessionResumed(outcome) => match *outcome {
                Ok(bootstrap) => {
                    self.install_bootstrap(bootstrap, true);
                    Vec::new()
                }
                Err(message) => {
                    self.chat.push_error(format!("resume failed: {message}"));
                    Vec::new()
                }
            },
            TaskResult::SessionOpened(result) => match result {
                Ok(bootstrap) => {
                    self.install_bootstrap(bootstrap, false);
                    if let Some(line) = self.pending_first_prompt.take() {
                        return self.submit_prompt(&line);
                    }
                    Vec::new()
                }
                Err(message) => {
                    self.pending_first_prompt = None;
                    self.toast(message);
                    Vec::new()
                }
            },
            TaskResult::OpenAiOAuthFinished(result) => {
                self.overlay = Overlay::None;
                match result {
                    Ok(()) => {
                        self.dismiss_getting_started();
                        self.connect_then_pick_model = true;
                        let config = agentloop_cli_core::ProviderConfig {
                            name: Some("OpenAI".to_owned()),
                            base_url: "https://api.openai.com/v1".to_owned(),
                            api_key: String::new(),
                            default_model: Some("gpt-4.1-mini".to_owned()),
                            thinking: false,
                            models: Vec::new(),
                        };
                        self.toast("OpenAI signed in — validating…");
                        vec![Effect::ValidateProvider {
                            id: "openai".to_owned(),
                            config,
                        }]
                    }
                    Err(message) => {
                        self.toast(message);
                        self.open_connect_wizard();
                        Vec::new()
                    }
                }
            }
        }
    }

    fn on_mcp_tools_listed(
        &mut self,
        server: &str,
        result: Result<Vec<agentloop_mcp::McpRemoteTool>, String>,
    ) {
        let Overlay::McpExplorer(state) = &mut self.overlay else {
            return;
        };
        if state.server != server {
            return;
        }
        match result {
            Ok(tools) => {
                state.phase = McpExplorerPhase::Tools { tools };
                state.selected = 0;
            }
            Err(message) => {
                state.phase = McpExplorerPhase::Failed { message };
            }
        }
    }

    fn on_mcp_tool_called(&mut self, server: &str, tool: &str, result: Result<String, String>) {
        let Overlay::McpExplorer(state) = &mut self.overlay else {
            return;
        };
        if state.server != server {
            return;
        }
        state.args_mode = false;
        match result {
            Ok(output) => {
                state.phase = McpExplorerPhase::Result {
                    output: format!("{tool}:\n{output}"),
                    is_error: false,
                };
                state.scroll = 0;
            }
            Err(message) => {
                state.phase = McpExplorerPhase::Failed {
                    message: format!("{tool} failed: {message}"),
                };
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
                // The overlay already showed the output; a toast suffices.
                self.toast(summary);
            }
        } else {
            self.chat.push_info(summary);
        }
    }

    /// Adopt a (new or resumed) session and its agent's capabilities.
    fn install_bootstrap(&mut self, bootstrap: SessionBootstrap, announce: bool) {
        let prev_session_mode = self.session.session_mode;
        let prev_permission_mode = self.session.permission_mode;
        let prev_effort = self.session.effort;
        let prev_show_thinking = self.show_thinking;
        let prev_thinking_budget = self.thinking_budget;
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
            permission_mode: bootstrap.permission_mode.unwrap_or(prev_permission_mode),
            session_mode: prev_session_mode,
            effort: prev_effort,
        };
        self.route = AppRoute::Session;
        self.session_label = bootstrap
            .session_title
            .clone()
            .or_else(|| bootstrap.session_created_at_ms.map(format_relative_time));
        self.providers = bootstrap.providers;
        self.catalog.clear();
        self.pending_provider = None;
        self.awaiting_model_picker = false;
        self.overlay = Overlay::None;
        self.pending_permissions.clear();
        self.pending_questions.clear();
        self.status.toasts.clear();
        self.markdown_cache.clear();
        self.show_thinking = bootstrap.hello.capabilities.reasoning_visible;
        if announce {
            self.session.session_mode = prev_session_mode;
            self.session.permission_mode = prev_permission_mode;
            self.show_thinking = prev_show_thinking;
            self.thinking_budget = prev_thinking_budget;
        }
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
            let mut note = if resumed {
                "resumed previous session".to_owned()
            } else {
                "new session".to_owned()
            };
            if self.carried_context.is_some() {
                note.push_str(" · your conversation carries into the next message");
            }
            self.chat
                .push_info(format!("switched to {} — {note}", self.kind));
        }
        self.mcp_enabled = bootstrap.mcp_enabled;
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

    fn on_openai_oauth(&mut self, event: agentloop_cli_core::OpenAiOAuthEvent) {
        let Overlay::ConnectWizard(state) = &mut self.overlay else {
            return;
        };
        if state.step != ConnectWizardStep::OAuthWaiting {
            return;
        }
        use agentloop_cli_core::OpenAiOAuthEvent;
        match event {
            OpenAiOAuthEvent::Started {
                url, instructions, ..
            } => {
                state.oauth_url = Some(url);
                state.oauth_instructions = Some(instructions);
                state.oauth_waiting = true;
            }
            OpenAiOAuthEvent::Waiting => {
                state.oauth_waiting = true;
            }
            OpenAiOAuthEvent::Succeeded => {
                // Terminal state arrives via TaskResult::OpenAiOAuthFinished.
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
        self.status.spinner = self.status.spinner.wrapping_add(1);
        // #region agent log
        if self.status.spinner % 10 == 0 {
            crate::debug_agent::log(
                "C",
                "app.rs:on_tick",
                "tick sample",
                serde_json::json!({
                    "spinner": self.status.spinner,
                    "isHomeRoute": self.is_home_route(),
                    "isHomeScreen": self.chat.is_home_screen(),
                    "inputEmpty": self.input.is_empty(),
                    "providersEmpty": self.providers.is_empty(),
                    "busy": busy,
                }),
            );
        }
        // #endregion
        if busy {
            self.dirty = true;
        } else if self.is_home_route() && self.input.is_empty() {
            if self.status.spinner % crate::input::ROTATE_HINT_TICKS == 0 {
                self.input.rotate_placeholder(self.status.spinner);
                self.dirty = true;
            }
        } else if self.providers.is_empty() && self.status.spinner % 50 == 0 {
            // Narrow-terminal connect hint alternates every 5s; no need to repaint every tick.
            self.dirty = true;
        }
        let live_toasts = self.status.toasts.len();
        self.status
            .toasts
            .retain(|toast| toast.created.elapsed() < TOAST_TTL);
        if self.status.toasts.len() != live_toasts {
            self.dirty = true;
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

/// One-line description of a provider's "auto" smart-orchestration mode, or
/// `None` if it has no auto mode. Gates whether the model picker shows an
/// `auto` row for the active provider.
fn auto_mode_detail(provider: &str) -> Option<&'static str> {
    match provider {
        "deepseek" => Some("smart split · v4-pro plans & builds · v4-flash researches"),
        _ => None,
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

fn catalog_item_from_ref(model: &ModelRef, current: Option<&ModelRef>) -> PickerItem {
    PickerItem {
        id: model.0.clone(),
        label: model.0.clone(),
        detail: current
            .filter(|active| active.0 == model.0)
            .map(|_| "current".to_owned()),
        enabled: true,
    }
}

fn make_permission_prompt(
    id: PermissionRequestId,
    call_id: Option<agentloop_contracts::ToolCallId>,
    title: String,
    detail: Option<String>,
    options: Vec<agentloop_contracts::PermissionDecisionKind>,
    session: Option<SessionId>,
    role: Option<String>,
) -> PermissionPrompt {
    let diff = crate::tool_output::diff_from_permission_detail(&title, detail.as_deref());
    PermissionPrompt {
        id,
        call_id,
        title,
        detail,
        options,
        selected: 0,
        session,
        role,
        diff,
        diff_expanded: false,
    }
}

fn parse_session_mode(text: &str) -> Option<SessionMode> {
    match text.trim().to_lowercase().as_str() {
        "code" => Some(SessionMode::Code),
        "plan" => Some(SessionMode::Plan),
        _ => None,
    }
}

fn parse_effort_level(text: &str) -> Option<Effort> {
    match text.trim().to_lowercase().as_str() {
        "low" => Some(Effort::Low),
        "medium" | "med" => Some(Effort::Medium),
        "high" => Some(Effort::High),
        "xhigh" | "extra" => Some(Effort::XHigh),
        "max" | "maximum" => Some(Effort::Max),
        _ => None,
    }
}

fn effort_label(level: Effort) -> &'static str {
    match level {
        Effort::Low => "low",
        Effort::Medium => "medium",
        Effort::High => "high",
        Effort::XHigh => "xhigh",
        Effort::Max => "max",
    }
}

fn parse_stored_permission_mode(text: &str) -> Option<PermissionMode> {
    parse_permission_arg(text).or_else(|| match text.trim().to_lowercase().as_str() {
        "accept-edits" => Some(PermissionMode::AcceptEdits),
        "dont-ask" => Some(PermissionMode::DontAsk),
        "bypass" => Some(PermissionMode::BypassPermissions),
        _ => None,
    })
}

fn permission_mode_pref_value(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::Default => "default",
        PermissionMode::AcceptEdits => "accept-edits",
        PermissionMode::Plan => "plan",
        PermissionMode::DontAsk => "dont-ask",
        PermissionMode::BypassPermissions => "bypass",
        _ => "default",
    }
}

fn thinking_config_from_prefs(budget: Option<u32>, caps: &AgentCaps) -> Option<ThinkingConfig> {
    let budget = budget?;
    caps.reasoning_visible.then_some(ThinkingConfig {
        budget_tokens: budget,
    })
}

impl App {
    fn turn_options(&self) -> TurnOptions {
        TurnOptions {
            model: self.session.model.clone(),
            fallback_models: self.fallback_models.clone(),
            permission_mode: Some(self.session.effective_permission_mode()),
            // Effort is the primary reasoning knob; an explicit `/thinking`
            // budget (if set) still wins in the loop.
            effort: Some(self.session.effort),
            thinking: thinking_config_from_prefs(self.thinking_budget, &self.caps),
            ..TurnOptions::default()
        }
    }

    fn persist_mode_prefs(&self) {
        if let Err(err) = CliPrefs::remember_modes(
            session_mode_label(self.session.session_mode),
            permission_mode_pref_value(self.session.permission_mode),
            self.show_thinking,
        ) {
            tracing::warn!(target: "prefs", "failed to save mode preferences: {err}");
        }
    }

    fn persist_thinking_budget(&self) {
        if let Err(err) = CliPrefs::remember_thinking_budget(self.thinking_budget) {
            tracing::warn!(target: "prefs", "failed to save thinking budget: {err}");
        }
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

/// `/roles` display lines: merged built-in + configured roles, then skipped
/// entries with reasons, then the config-path footer.
fn role_lines(prefs: &agentloop_cli_core::CliPrefs) -> Vec<String> {
    use agentloop_engine::{RoleRegistry, RoleToolProfile};
    let (specs, skipped) = agentloop_cli_core::role_specs(prefs);
    let mut lines = vec!["roles:".to_owned()];
    match RoleRegistry::with_defaults(specs) {
        Ok(registry) => {
            let mut names: Vec<String> = registry
                .spawnable()
                .into_iter()
                .map(|(name, _)| name)
                .collect();
            names.push("main".to_owned());
            for name in names {
                let Some(spec) = registry.get(&name) else {
                    continue;
                };
                let source = if prefs.roles.contains_key(&name) {
                    "config"
                } else {
                    "default"
                };
                let chain = if spec.models.is_empty() {
                    "inherit".to_owned()
                } else {
                    spec.models
                        .iter()
                        .map(|model| model.0.clone())
                        .collect::<Vec<_>>()
                        .join(" → ")
                };
                let tools = match &spec.tools {
                    RoleToolProfile::ReadOnly => "read-only".to_owned(),
                    RoleToolProfile::Full => "full".to_owned(),
                    RoleToolProfile::Allow(list) => list.join(","),
                };
                lines.push(format!(
                    "  {name} · {chain} · tools: {tools} · split: {} · max_parallel: {} · {source}",
                    spec.split, spec.max_parallel
                ));
            }
        }
        Err(err) => lines.push(format!("  role registry error: {err}")),
    }
    for (name, reason) in skipped {
        lines.push(format!("  {name} · skipped: {reason}"));
    }
    lines.push(format!(
        "edit roles in {}",
        agentloop_cli_core::config_path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "config.json".to_owned())
    ));
    lines
}

/// A pseudo-random verb index; one pick per turn is all the randomness the
/// busy line needs, so clock jitter beats a rand dependency.
fn pick_verb_idx() -> usize {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or(0);
    nanos as usize % crate::theme::SPINNER_VERBS.len()
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
            mcp_enabled: 0,
            session_restarted: false,
            isolated: false,
            session_title: None,
            session_created_at_ms: None,
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
        // The copilot delegator is feature-flagged off by default (see
        // `delegated_agents_enabled`); this test covers its login flow, which
        // only runs when the flag is on.
        temp_env::with_var("FLEX_ENABLE_DELEGATED_AGENTS", Some("1"), || {
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
mod status_tests {
    use super::*;
    use crate::events::SessionBootstrap;
    use crate::files::FileIndex;
    use agentloop_contracts::{AgentCaps, Hello, SessionId};

    fn test_app() -> App {
        let bootstrap = SessionBootstrap {
            kind: AgentKind::Native,
            hello: Hello::new(AgentCaps::default()),
            session: SessionId::from("sess-test"),
            providers: vec!["anthropic".to_owned()],
            model: None,
            transcript: None,
            trace: Vec::new(),
            permission_mode: None,
            mcp_enabled: 0,
            session_restarted: false,
            isolated: false,
            session_title: None,
            session_created_at_ms: None,
        };
        App::new(bootstrap, PathBuf::from("."), FileIndex::default())
    }

    fn finished_turn() -> TaskResult {
        TaskResult::TurnFinished(Ok(agentloop_contracts::TurnSummary {
            turn_id: agentloop_contracts::TurnId::generate(),
            stop_reason: agentloop_contracts::TurnStopReason::EndTurn,
            usage: agentloop_contracts::TokenUsage::default(),
            cost_usd: None,
            num_model_calls: 1,
            num_tool_calls: 0,
            duration_ms: 10,
        }))
    }

    #[test]
    fn shift_tab_cycles_require_accept_edits_plan() {
        let mut app = test_app();
        app.caps.permissions.modes = vec![
            PermissionMode::Default,
            PermissionMode::AcceptEdits,
            PermissionMode::Plan,
        ];
        app.session.session_mode = SessionMode::Code;
        app.session.permission_mode = PermissionMode::Default;

        app.cycle_ui_mode();
        assert_eq!(app.session.permission_mode, PermissionMode::AcceptEdits);
        assert_eq!(app.session.session_mode, SessionMode::Code);

        app.cycle_ui_mode();
        assert_eq!(app.session.session_mode, SessionMode::Plan);

        app.cycle_ui_mode();
        assert_eq!(app.session.session_mode, SessionMode::Code);
        assert_eq!(app.session.permission_mode, PermissionMode::Default);
    }

    #[test]
    fn prompts_queue_while_running_and_drain_after_turn() {
        let mut app = test_app();
        app.begin_turn();
        let effects = app.submit_prompt("second question");
        assert!(effects.is_empty(), "running turn queues instead of sending");
        assert_eq!(app.queued_prompts.len(), 1);
        // Turn finishes → queued prompt auto-submits.
        let effects = app.on_task(finished_turn());
        assert_eq!(effects.len(), 1, "queued prompt submits on turn end");
        assert!(app.queued_prompts.is_empty());
        assert!(app.session.turn.is_running());
    }

    #[test]
    fn interrupt_clears_the_prompt_queue() {
        let mut app = test_app();
        app.begin_turn();
        app.submit_prompt("queued one");
        app.clear_prompt_queue();
        assert!(app.queued_prompts.is_empty());
        let effects = app.on_task(finished_turn());
        assert!(effects.is_empty(), "nothing drains after an interrupt");
    }

    #[test]
    fn toasts_cap_at_three_dropping_oldest() {
        let mut app = test_app();
        for idx in 0..5 {
            app.toast(format!("toast {idx}"));
        }
        assert_eq!(app.status.toasts.len(), 3);
        assert_eq!(
            app.status.toasts.front().map(|t| t.text.as_str()),
            Some("toast 2")
        );
        assert_eq!(
            app.status.toasts.back().map(|t| t.text.as_str()),
            Some("toast 4")
        );
    }

    #[test]
    fn begin_turn_resets_busy_counters_once() {
        let mut app = test_app();
        app.status.turn_output_chars = 999;
        app.begin_turn();
        assert!(app.session.turn.is_running());
        assert_eq!(app.status.turn_output_chars, 0);
        // A second begin_turn mid-turn must not reset the counter or verb.
        app.status.turn_output_chars = 40;
        let verb = app.status.turn_verb_idx;
        app.begin_turn();
        assert_eq!(app.status.turn_output_chars, 40);
        assert_eq!(app.status.turn_verb_idx, verb);
    }

    #[test]
    fn delta_events_accumulate_output_chars() {
        let mut app = test_app();
        let event = agentloop_contracts::SessionEvent {
            session_id: SessionId::from("sess-test"),
            seq: 1,
            turn_id: None,
            ts_ms: 0,
            payload: AgentEvent::MarkdownDelta {
                message_id: agentloop_contracts::MessageId::from("m1"),
                text: "abcdefgh".to_owned(),
            },
        };
        app.on_engine(event);
        assert_eq!(app.status.turn_output_chars, 8);
        assert!(app.session.turn.is_running());
    }

    #[test]
    fn relayed_permission_prompt_tags_session_and_role_and_clears() {
        use crate::overlay::Overlay;
        use agentloop_contracts::{
            AgentEvent, PermissionDecision, PermissionDecisionKind, PermissionRequestId,
            SessionEvent, ToolCallId,
        };
        let mut app = test_app();
        let child = SessionId::from("child-1");
        let mut seq = 0;
        let mut engine = |app: &mut App, payload: AgentEvent| {
            seq += 1;
            app.on_engine(SessionEvent {
                session_id: SessionId::from("sess-test"),
                seq,
                turn_id: None,
                ts_ms: 0,
                payload,
            });
        };
        engine(
            &mut app,
            AgentEvent::SubagentStarted {
                child_session: child.clone(),
                task: "do protected work".to_owned(),
                call_id: None,
                role: Some("worker".to_owned()),
            },
        );
        engine(
            &mut app,
            AgentEvent::SubagentEvent {
                child_session: child.clone(),
                event: Box::new(AgentEvent::PermissionRequested {
                    id: PermissionRequestId::from("perm-1"),
                    call_id: Some(ToolCallId::from("c1")),
                    title: "Allow `Bash`?".to_owned(),
                    detail: None,
                    options: vec![PermissionDecisionKind::AllowOnce],
                }),
            },
        );
        let Overlay::Permission(prompt) = &app.overlay else {
            panic!("expected permission overlay");
        };
        assert_eq!(
            prompt.session.as_ref().map(SessionId::as_str),
            Some("child-1")
        );
        assert_eq!(prompt.role.as_deref(), Some("worker"));

        engine(
            &mut app,
            AgentEvent::SubagentEvent {
                child_session: child,
                event: Box::new(AgentEvent::PermissionResolved {
                    id: PermissionRequestId::from("perm-1"),
                    decision: PermissionDecision::Deny { reason: None },
                }),
            },
        );
        assert!(
            !app.overlay.is_active(),
            "relayed resolution clears the prompt"
        );
    }
}

#[cfg(test)]
mod session_tests {
    use super::*;
    use crate::chat::ChatItem;
    use crate::commands::LocalCommand;
    use crate::events::{Effect, SessionBootstrap};
    use crate::files::FileIndex;
    use agentloop_cli_core::AgentKind;
    use agentloop_contracts::{AgentCaps, Hello, PermissionCaps, PermissionMode, SessionId};
    use std::path::Path;

    fn native_caps() -> AgentCaps {
        AgentCaps {
            permissions: PermissionCaps {
                interactive: true,
                modes: vec![
                    PermissionMode::Default,
                    PermissionMode::AcceptEdits,
                    PermissionMode::BypassPermissions,
                ],
                tool_scoping: true,
            },
            reasoning_visible: true,
            ..AgentCaps::default()
        }
    }

    fn native_test_app() -> App {
        let bootstrap = SessionBootstrap {
            kind: AgentKind::Native,
            hello: Hello::new(native_caps()),
            session: SessionId::from("sess-test"),
            providers: vec!["anthropic".to_owned()],
            model: None,
            transcript: None,
            trace: Vec::new(),
            permission_mode: None,
            mcp_enabled: 0,
            session_restarted: false,
            isolated: false,
            session_title: None,
            session_created_at_ms: None,
        };
        App::new(bootstrap, PathBuf::from("."), FileIndex::default())
    }

    fn native_test_app_at(workdir: PathBuf) -> App {
        let bootstrap = SessionBootstrap {
            kind: AgentKind::Native,
            hello: Hello::new(native_caps()),
            session: SessionId::from("sess-test"),
            providers: vec!["anthropic".to_owned()],
            model: None,
            transcript: None,
            trace: Vec::new(),
            permission_mode: None,
            mcp_enabled: 0,
            session_restarted: false,
            isolated: false,
            session_title: None,
            session_created_at_ms: None,
        };
        App::new(bootstrap, workdir, FileIndex::default())
    }

    /// Run `f` with an isolated `XDG_CONFIG_HOME` — so `App::new`'s internal
    /// `McpStore::load()` reads/writes a throwaway global config instead of
    /// the real user's `~/.config/agentloop/mcp.json` — and a fresh project
    /// directory for `.agent/mcp.json`.
    fn with_isolated_project<R>(f: impl FnOnce(&Path) -> R) -> R {
        let config_home = tempfile::tempdir().expect("tempdir");
        let project_dir = tempfile::tempdir().expect("tempdir");
        temp_env::with_var(
            "XDG_CONFIG_HOME",
            Some(config_home.path().to_str().expect("utf8")),
            || f(project_dir.path()),
        )
    }

    #[test]
    fn init_writes_project_mcp_json_when_servers_enabled() {
        with_isolated_project(|project| {
            let mut app = native_test_app_at(project.to_path_buf());
            app.mcp_store
                .install_npm("@modelcontextprotocol/server-fetch", Some("fetch"))
                .expect("install");

            app.run_init();

            let project_store = McpStore::load_project(project).expect("load project");
            assert_eq!(project_store.servers.len(), 1);
            assert_eq!(project_store.servers[0].config.name, "fetch");
            assert!(
                matches!(app.chat.items.last(), Some(ChatItem::Info { text }) if text.contains("saved"))
            );
        });
    }

    #[test]
    fn init_does_not_overwrite_existing_project_file() {
        with_isolated_project(|project| {
            let mut seed = McpStore::default();
            seed.install_npm("server-memory", Some("memory"))
                .expect("install");
            seed.export_to_project(project).expect("export");

            let mut app = native_test_app_at(project.to_path_buf());
            app.mcp_store
                .install_npm("@modelcontextprotocol/server-fetch", Some("fetch"))
                .expect("install");
            app.run_init();

            let project_store = McpStore::load_project(project).expect("load project");
            assert_eq!(
                project_store.servers.len(),
                1,
                "existing project file must not be overwritten"
            );
            assert_eq!(project_store.servers[0].config.name, "memory");
            assert!(
                matches!(app.chat.items.last(), Some(ChatItem::Info { text }) if text.contains("already declares"))
            );
        });
    }

    #[test]
    fn init_with_no_servers_hints_at_mcp_install() {
        with_isolated_project(|project| {
            let mut app = native_test_app_at(project.to_path_buf());
            app.run_init();
            assert!(!agentloop_cli_core::mcp_project_path(project).exists());
            assert!(
                matches!(app.chat.items.last(), Some(ChatItem::Info { text }) if text.contains("/mcp-install"))
            );
        });
    }

    #[test]
    fn mcp_import_opens_confirm_for_new_project_servers() {
        with_isolated_project(|project| {
            let mut seed = McpStore::default();
            seed.install_npm("@modelcontextprotocol/server-fetch", Some("fetch"))
                .expect("install");
            seed.export_to_project(project).expect("export");

            let mut app = native_test_app_at(project.to_path_buf());
            app.run_mcp_import();

            match &app.overlay {
                Overlay::Confirm(prompt) => match &prompt.action {
                    ConfirmAction::McpImport { servers } => {
                        assert_eq!(servers.len(), 1);
                        assert_eq!(servers[0].config.name, "fetch");
                    }
                    other => panic!("expected McpImport action, got {other:?}"),
                },
                other => panic!("expected a confirm overlay, got {other:?}"),
            }
        });
    }

    #[test]
    fn mcp_import_with_nothing_new_just_toasts() {
        with_isolated_project(|project| {
            let mut seed = McpStore::default();
            seed.install_npm("server-memory", Some("memory"))
                .expect("install");
            seed.export_to_project(project).expect("export");

            let mut app = native_test_app_at(project.to_path_buf());
            app.mcp_store
                .install_npm("server-memory", Some("memory"))
                .expect("install");
            app.run_mcp_import();

            assert!(matches!(app.overlay, Overlay::None));
            assert!(
                app.status
                    .toasts
                    .back()
                    .is_some_and(|t| t.text.contains("up to date"))
            );
        });
    }

    #[test]
    fn confirming_mcp_import_merges_into_global_store() {
        with_isolated_project(|project| {
            let mut app = native_test_app_at(project.to_path_buf());
            let incoming = {
                let mut seed = McpStore::default();
                seed.install_npm("@modelcontextprotocol/server-fetch", Some("fetch"))
                    .expect("install");
                seed.servers
            };

            app.apply_confirm_action(ConfirmAction::McpImport { servers: incoming });

            assert_eq!(app.mcp_store.servers.len(), 1);
            assert_eq!(app.mcp_store.servers[0].config.name, "fetch");
        });
    }

    fn test_session() -> SessionState {
        SessionState {
            id: SessionId::from("sess-test"),
            model: None,
            turn: TurnPhase::Idle,
            last_seq: 0,
            permission_mode: PermissionMode::AcceptEdits,
            session_mode: SessionMode::Code,
            effort: Effort::default(),
        }
    }

    #[test]
    fn effective_permission_mode_plan_overrides_security() {
        let mut session = test_session();
        session.session_mode = SessionMode::Plan;
        assert_eq!(session.effective_permission_mode(), PermissionMode::Plan);
    }

    #[test]
    fn bypass_mode_is_sent_on_submit_prompt() {
        let mut app = native_test_app();
        app.session.permission_mode = PermissionMode::BypassPermissions;
        let effects = app.submit_prompt("hello");
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            Effect::SubmitPrompt { opts, .. } => {
                assert_eq!(
                    opts.permission_mode,
                    Some(PermissionMode::BypassPermissions)
                );
            }
            other => panic!("expected SubmitPrompt, got {other:?}"),
        }
    }

    #[test]
    fn agent_switch_carries_conversation_into_next_prompt() {
        let mut app = native_test_app();
        app.chat
            .apply(&agentloop_contracts::AgentEvent::UserMessage {
                message_id: agentloop_contracts::MessageId::from("u1"),
                content: vec![agentloop_contracts::ContentBlock::markdown(
                    "remember the blue widget",
                )],
            });
        app.capture_carry_context();
        assert!(app.carried_context.is_some(), "context should be captured");

        let effects = app.submit_prompt("what did I ask about?");
        assert!(
            app.carried_context.is_none(),
            "carry is consumed after one prompt"
        );
        match effects.first() {
            Some(Effect::SubmitPrompt { input, .. }) => {
                let text = input.joined_text();
                assert!(
                    text.contains("remember the blue widget"),
                    "carried context missing from prompt: {text}"
                );
                assert!(
                    text.contains("what did I ask about?"),
                    "user request missing from prompt: {text}"
                );
            }
            other => panic!("expected SubmitPrompt, got {other:?}"),
        }
    }

    #[test]
    fn deepseek_roles_preset_sets_planner_model_and_reloads() {
        // Isolate the config path so applying the preset never touches the
        // real user config. Assert the handler *wiring* only — the persisted
        // role mapping is covered by prefs::tests to avoid a racy disk
        // round-trip here (XDG_CONFIG_HOME is process-global).
        let dir = tempfile::tempdir().expect("tempdir");
        temp_env::with_var(
            "XDG_CONFIG_HOME",
            Some(dir.path().to_str().expect("utf8")),
            || {
                let mut app = native_test_app();
                let effects = app.run_roles_preset("deepseek");
                // Planner/main session moves to the strong model...
                assert_eq!(
                    app.session.model.as_ref().map(|m| m.0.as_str()),
                    Some("deepseek/deepseek-v4-pro")
                );
                // ...and the engine reloads so the new role chains take effect.
                assert!(
                    effects
                        .iter()
                        .any(|e| matches!(e, Effect::ReloadEngine { .. })),
                    "expected a ReloadEngine effect, got {effects:?}"
                );
            },
        );
    }

    #[test]
    fn unknown_roles_preset_reports_available() {
        let mut app = native_test_app();
        let effects = app.run_roles_preset("bogus");
        assert!(effects.is_empty());
        assert!(
            matches!(app.chat.items.last(), Some(ChatItem::Info { text }) if text.contains("available: deepseek"))
        );
    }

    #[test]
    fn mouse_capture_on_by_default_traps_wheel_in_transcript() {
        let app = native_test_app();
        assert!(
            app.mouse_capture,
            "capture must default on so the wheel scrolls the transcript, not over the CLI"
        );
    }

    #[test]
    fn apply_loaded_prefs_restores_saved_mouse_capture_choice() {
        let mut app = native_test_app();
        assert!(app.mouse_capture, "default is capture on");
        let prefs = agentloop_cli_core::CliPrefs {
            mouse_capture: Some(false),
            ..Default::default()
        };
        app.apply_loaded_prefs(&prefs);
        assert!(
            !app.mouse_capture,
            "a persisted Ctrl+M choice must survive a restart"
        );
    }

    #[test]
    fn connect_deepseek_shortcut_infers_url_and_model() {
        let mut app = native_test_app();
        let effects = app.run_connect(Some("deepseek sk-test-123".to_owned()));
        match effects.first() {
            Some(Effect::ValidateProvider { id, config }) => {
                assert_eq!(id, "deepseek");
                assert_eq!(config.base_url, "https://api.deepseek.com/v1");
                assert_eq!(config.api_key, "sk-test-123");
                assert_eq!(config.default_model.as_deref(), Some("deepseek-v4-pro"));
                assert!(config.thinking);
            }
            other => panic!("expected ValidateProvider, got {other:?}"),
        }
    }

    #[test]
    fn connect_openai_shortcut_infers_url_and_model() {
        let mut app = native_test_app();
        let effects = app.run_connect(Some("openai sk-proj-abc".to_owned()));
        match effects.first() {
            Some(Effect::ValidateProvider { id, config }) => {
                assert_eq!(id, "openai");
                assert_eq!(config.base_url, "https://api.openai.com/v1");
                assert_eq!(config.api_key, "sk-proj-abc");
                assert_eq!(config.default_model.as_deref(), Some("gpt-4.1-mini"));
                assert!(!config.thinking);
            }
            other => panic!("expected ValidateProvider, got {other:?}"),
        }
    }

    #[test]
    fn connect_openai_shortcut_honors_explicit_model() {
        let mut app = native_test_app();
        let effects = app.run_connect(Some("openai sk-proj-abc gpt-4o".to_owned()));
        match effects.first() {
            Some(Effect::ValidateProvider { id, config }) => {
                assert_eq!(id, "openai");
                assert_eq!(config.base_url, "https://api.openai.com/v1");
                assert_eq!(config.default_model.as_deref(), Some("gpt-4o"));
            }
            other => panic!("expected ValidateProvider, got {other:?}"),
        }
    }

    #[test]
    fn connect_full_form_still_uses_given_url() {
        let mut app = native_test_app();
        let effects = app.run_connect(Some("myllm http://localhost:1234/v1".to_owned()));
        match effects.first() {
            Some(Effect::ValidateProvider { id, config }) => {
                assert_eq!(id, "myllm");
                assert_eq!(config.base_url, "http://localhost:1234/v1");
                assert!(config.api_key.is_empty(), "keyless local endpoint");
                assert!(!config.thinking);
            }
            other => panic!("expected ValidateProvider, got {other:?}"),
        }
    }

    #[test]
    fn auto_mode_detail_only_for_deepseek() {
        assert!(auto_mode_detail("deepseek").is_some());
        assert!(auto_mode_detail("copilot").is_none());
        assert!(auto_mode_detail("anthropic").is_none());
    }

    #[test]
    fn set_model_auto_for_provider_without_auto_is_graceful() {
        let mut app = native_test_app();
        // `<provider>/auto` for a provider that has no auto mode must not set a
        // literal "…/auto" model, and must not trip the Copilot login path.
        let effects = app.run_local(LocalCommand::Model {
            arg: Some("copilot/auto".to_owned()),
        });
        assert!(effects.is_empty());
        assert!(
            app.session.model.is_none(),
            "must not set a literal /auto model"
        );
        assert!(matches!(app.overlay, crate::overlay::Overlay::None));
    }

    fn catalog_entry(provider: &str, model: &str) -> CatalogEntry {
        CatalogEntry {
            provider: agentloop_contracts::ProviderId::from(provider),
            model: agentloop_contracts::ModelInfo {
                id: model.to_owned(),
                display_name: None,
                context_window: None,
                reasoning: false,
                vision: false,
            },
        }
    }

    #[test]
    fn model_picker_scopes_to_active_provider_and_offers_auto() {
        let mut app = native_test_app();
        app.session.model = Some(ModelRef::from("deepseek/deepseek-v4-pro"));
        app.catalog = vec![
            catalog_entry("deepseek", "deepseek-v4-pro"),
            catalog_entry("deepseek", "deepseek-v4-flash"),
            catalog_entry("copilot", "gpt-5"),
            catalog_entry("anthropic", "claude-sonnet-5"),
        ];
        app.open_catalog_picker();
        let crate::overlay::Overlay::Picker(picker) = &app.overlay else {
            panic!("expected a picker overlay");
        };
        // First row is the DeepSeek auto mode.
        assert_eq!(picker.items[0].label, "auto");
        assert_eq!(picker.items[0].id, "deepseek/auto");
        // No Copilot or Anthropic models leak into a DeepSeek session.
        assert!(
            picker
                .items
                .iter()
                .all(|item| item.label == "auto" || item.id.starts_with("deepseek/")),
            "picker leaked non-deepseek models: {:?}",
            picker.items.iter().map(|i| &i.id).collect::<Vec<_>>()
        );
        // Both DeepSeek models are present.
        assert!(
            picker
                .items
                .iter()
                .any(|i| i.id == "deepseek/deepseek-v4-flash")
        );
    }

    #[test]
    fn switch_agent_to_delegated_kind_is_refused_by_default() {
        temp_env::with_var_unset("FLEX_ENABLE_DELEGATED_AGENTS", || {
            let mut app = native_test_app();
            let effects = app.switch_agent("claude-code");
            assert!(effects.is_empty(), "no effect should fire when disabled");
            assert_eq!(app.kind, AgentKind::Native, "kind must not change");
            assert!(
                matches!(
                    app.chat.items.last(),
                    Some(ChatItem::Info { text }) if text.contains("disabled")
                ),
                "expected a disabled-agent info line, got {:?}",
                app.chat.items.last()
            );
        });
    }

    #[test]
    fn open_agent_picker_lists_only_native_by_default() {
        temp_env::with_var_unset("FLEX_ENABLE_DELEGATED_AGENTS", || {
            let mut app = native_test_app();
            app.open_agent_picker();
            match &app.overlay {
                Overlay::Picker(picker) => {
                    assert_eq!(picker.items.len(), 1);
                    assert_eq!(picker.items[0].id, "native");
                }
                other => panic!("expected a picker overlay, got {other:?}"),
            }
        });
    }

    #[test]
    fn allow_all_command_syncs_live_turn_mode() {
        let mut app = native_test_app();
        app.overlay = Overlay::Confirm(ConfirmPrompt {
            title: "Bypass all tool permissions?".to_owned(),
            message: String::new(),
            action: ConfirmAction::AllowAllPermissions,
        });
        let outcome = OverlayOutcome {
            confirmed: Some(ConfirmAction::AllowAllPermissions),
            close: true,
            ..OverlayOutcome::default()
        };
        let effects = app.apply_overlay_outcome(outcome);
        assert_eq!(
            app.session.permission_mode,
            PermissionMode::BypassPermissions
        );
        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::SetTurnPermissionMode {
                    mode: Some(PermissionMode::BypassPermissions)
                }
            )),
            "expected live turn permission sync, got {effects:?}"
        );
    }

    #[test]
    fn install_bootstrap_preserves_permission_mode_on_reload() {
        let mut app = native_test_app();
        app.session.permission_mode = PermissionMode::BypassPermissions;
        app.session.session_mode = SessionMode::Plan;
        let bootstrap = SessionBootstrap {
            kind: AgentKind::Native,
            hello: Hello::new(native_caps()),
            session: SessionId::from("sess-reloaded"),
            providers: vec!["anthropic".to_owned()],
            model: None,
            transcript: None,
            trace: Vec::new(),
            permission_mode: None,
            mcp_enabled: 1,
            session_restarted: false,
            isolated: false,
            session_title: None,
            session_created_at_ms: None,
        };
        app.install_bootstrap(bootstrap, false);
        assert_eq!(
            app.session.permission_mode,
            PermissionMode::BypassPermissions
        );
        assert_eq!(app.session.session_mode, SessionMode::Plan);
    }

    #[test]
    fn permissions_allow_all_routes_to_confirm() {
        let mut app = native_test_app();
        app.run_local(LocalCommand::Permissions {
            arg: Some("allow-all".to_owned()),
        });
        assert!(matches!(app.overlay, Overlay::Confirm(_)));
    }

    #[test]
    fn connect_two_tokens_defaults_empty_key() {
        let mut app = native_test_app();
        let effects = app.run_local(LocalCommand::Connect {
            arg: Some("lmstudio http://localhost:1234/v1".to_owned()),
        });
        let Some(Effect::ValidateProvider { id, config }) = effects.first() else {
            panic!("expected ValidateProvider, got {effects:?}");
        };
        assert_eq!(id, "lmstudio");
        assert_eq!(config.base_url, "http://localhost:1234/v1");
        assert_eq!(config.api_key, "", "omitted key means keyless endpoint");
    }
}

#[cfg(test)]
mod roles_tests {
    use super::role_lines;
    use agentloop_cli_core::{CliPrefs, RoleConfig};

    #[test]
    fn roles_listing_shows_builtins_when_config_empty() {
        let lines = role_lines(&CliPrefs::default());
        for name in ["searcher", "worker", "reviewer", "main"] {
            assert!(
                lines
                    .iter()
                    .any(|line| line.contains(name) && line.contains("default")),
                "missing built-in {name} in {lines:?}"
            );
        }
        assert!(!lines.iter().any(|line| line.contains("skipped")));
        assert!(lines.last().expect("footer").starts_with("edit roles in"));
    }

    #[test]
    fn roles_listing_marks_config_source_and_skips() {
        let mut prefs = CliPrefs::default();
        prefs.roles.insert(
            "searcher".to_owned(),
            RoleConfig {
                models: vec!["deepseek/deepseek-chat".to_owned()],
                ..RoleConfig::default()
            },
        );
        prefs
            .roles
            .insert("Bad Name!".to_owned(), RoleConfig::default());
        let lines = role_lines(&prefs);
        assert!(lines.iter().any(|line| line.contains("searcher")
            && line.contains("deepseek/deepseek-chat")
            && line.contains("config")));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Bad Name!") && line.contains("skipped:"))
        );
    }
}
