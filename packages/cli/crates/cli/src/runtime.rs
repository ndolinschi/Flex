//! Tokio + terminal runtime: wire reducer effects to engine operations.

use std::io::{self, Write};
use std::process::Command as ProcessCommand;
use std::sync::Arc;
use std::time::Duration;

use std::time::Instant;

use anyhow::{Context, Result};
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste, EventStream};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;

use agentloop_cli_core::{
    AgentKind, CliPrefs, EngineHub, InstallTarget, LoginEvent, McpStore, ModelCatalog,
    SessionController, login_copilot, resolve_stored_model,
};
use agentloop_contracts::{ModelRef, NewSessionParams, SessionId};
use agentloop_core::EventStream as EngineEventStream;

use crate::app::App;
use crate::args::{Args, Command, ResumeMode};
use crate::events::{AppEvent, Effect, SessionBootstrap, ShellCommandOutcome, TaskResult};
use crate::files::FileIndex;

const EVENT_QUEUE: usize = 1024;
const TICK_MS: u64 = 100;
const FRAME_BUDGET: Duration = Duration::from_millis(16);

/// Run either the TUI or the headless auth subcommand.
pub async fn run(args: Args) -> Result<()> {
    match args.command.clone() {
        Command::Tui { resume } => run_tui(args, resume).await,
        Command::AuthLogin => run_auth_login().await,
        Command::ListSessions => run_list_sessions(args).await,
    }
}

/// `flex sessions` — print recent sessions for the working directory
/// and exit, so the user can copy an id for `--resume <id>`.
async fn run_list_sessions(args: Args) -> Result<()> {
    let sessions = agentloop_cli_core::list_recent_sessions(&args.workdir, 20).await;
    let mut out = io::stdout();
    if sessions.is_empty() {
        writeln!(out, "No saved sessions for {}", args.workdir.display())?;
        return Ok(());
    }
    writeln!(out, "Recent sessions for {}:", args.workdir.display())?;
    for summary in sessions {
        writeln!(out, "  {}  {}", summary.id.0, summary.preview)?;
    }
    writeln!(
        out,
        "\nResume with `flex --resume <id>`, or `--continue` for the most recent."
    )?;
    Ok(())
}

async fn run_tui(mut args: Args, resume: ResumeMode) -> Result<()> {
    // Restore the terminal on any panic (main loop or a spawned task) before
    // the default hook prints — otherwise a crash leaves raw mode + mouse
    // tracking on and the shell echoes escape sequences on every mouse move.
    install_panic_hook();
    // Delegated agents (claude-code, copilot) are feature-flagged off by
    // default — see `delegated_agents_enabled`. A `--agent` requesting one
    // falls back to native rather than attempting a disabled path.
    let disabled_agent_requested =
        args.agent != AgentKind::Native && !agentloop_cli_core::delegated_agents_enabled();
    if disabled_agent_requested {
        args.agent = AgentKind::Native;
    }
    let (tx, mut rx) = mpsc::channel(EVENT_QUEUE);
    let mut hub = EngineHub::new(
        args.workdir.clone(),
        args.provider.clone(),
        args.model.clone(),
    );
    // Resume a prior session when asked: seed the hub so `bootstrap_session`
    // resumes it instead of opening fresh. A missing or unknown target
    // degrades to a new session rather than failing startup.
    match &resume {
        ResumeMode::New => {}
        ResumeMode::Continue => match agentloop_cli_core::most_recent_session(&args.workdir).await {
            Some(id) => hub.remember_session(args.agent, id),
            None => tracing::info!(
                target: "session",
                "no prior session for this directory; starting fresh"
            ),
        },
        ResumeMode::Session(id) => {
            let session = SessionId::from(id.clone());
            if agentloop_cli_core::session_exists(&session).await {
                hub.remember_session(args.agent, session);
            } else {
                tracing::warn!(target: "session", "session `{id}` not found; starting fresh");
            }
        }
    }
    let cli_model = args.model.clone().map(ModelRef);
    let prefs = CliPrefs::load();
    let saved_model = if cli_model.is_none() {
        prefs.last_model.clone()
    } else {
        None
    };
    let requested_model = cli_model
        .clone()
        .or_else(|| saved_model.as_ref().map(|stored| ModelRef(stored.clone())));
    let (controller, events, mut bootstrap) =
        bootstrap_session(&mut hub, args.agent, requested_model.clone()).await?;
    bootstrap.model = resolve_startup_model(cli_model.as_ref(), saved_model.as_deref(), &bootstrap);
    if cli_model.is_some() {
        if let Some(model) = bootstrap.model.as_ref() {
            if let Err(err) = CliPrefs::remember_model(model) {
                tracing::warn!(target: "prefs", "failed to save --model preference: {err}");
            }
        }
    }
    spawn_engine_forwarder(events, tx.clone());
    spawn_terminal_forwarder(tx.clone());
    spawn_tick_forwarder(tx.clone());
    spawn_interrupt_forwarder(tx.clone());
    spawn_signal_restore();

    let workdir = args.workdir.clone();
    let file_index = match tokio::task::spawn_blocking({
        let workdir = workdir.clone();
        move || FileIndex::build(&workdir)
    })
    .await
    {
        Ok(Ok(index)) => index,
        Ok(Err(err)) => {
            tracing::warn!(target: "files", "file index unavailable: {err}");
            FileIndex::default()
        }
        Err(err) => {
            tracing::warn!(target: "files", "file index worker failed: {err}");
            FileIndex::default()
        }
    };

    let mut app = App::new(bootstrap, workdir, file_index);
    app.apply_loaded_prefs(&prefs);
    // A `--effort` flag overrides the saved default for this run (and persists,
    // like `--model`).
    if let Some(effort) = args.effort.as_deref() {
        app.apply_effort_arg(effort);
    }
    if disabled_agent_requested {
        app.chat.push_info(
            "--agent is native-only for now — delegated agents are disabled \
             (set FLEX_ENABLE_DELEGATED_AGENTS=1 to re-enable)",
        );
    }
    let executor = EffectExecutor::new(hub, controller, args.agent, tx.clone());
    let mut terminal = TerminalSession::enter()?;
    // Sync the terminal to the app's mouse-capture state (default on, or a
    // persisted Ctrl+M choice from `apply_loaded_prefs`) — `enter()` leaves
    // capture off, so this is what actually turns it on for reliable scroll.
    terminal.set_mouse_capture(app.mouse_capture)?;
    terminal.draw(&mut app)?;

    // Ticks left in the raw-mode guard window after a reload — an MCP child
    // spawned by the reload can knock the terminal out of raw mode a little
    // after the reload lands, and crossterm's enable_raw_mode() no-ops once it
    // thinks raw is on, so we force-re-apply for a couple of seconds.
    let mut raw_guard: u8 = 0;
    while let Some(first) = rx.recv().await {
        let batch = coalesce_events(&mut rx, first).await;
        // An engine reload restarts MCP servers (child processes) that share
        // our controlling terminal and can disturb its state.
        let engine_reloaded = batch
            .iter()
            .any(|event| matches!(event, AppEvent::Task(TaskResult::EngineReloaded(_))));
        let has_tick = batch.iter().any(|event| matches!(event, AppEvent::Tick));
        let mut effects = Vec::new();
        for event in batch {
            effects.extend(app.update(event));
        }
        let should_quit =
            app.should_quit || effects.iter().any(|effect| matches!(effect, Effect::Quit));
        apply_terminal_effects(&mut terminal, &mut app, &effects)?;
        let engine_effects = effects
            .into_iter()
            .filter(|effect| {
                !matches!(
                    effect,
                    Effect::SetMouseCapture(_)
                        | Effect::CopyToClipboard { .. }
                        | Effect::SaveLastModel(_)
                )
            })
            .collect::<Vec<_>>();
        executor.execute_all(engine_effects);
        let mut force_redraw = false;
        if engine_reloaded {
            terminal.reassert(app.mouse_capture)?;
            raw_guard = 20; // guard the next ~2s of ticks against a late clobber
            force_redraw = true;
        } else if has_tick && raw_guard > 0 {
            // A child that reset the shared tty's termios turns ECHO back on
            // (scroll wheel then echoes as `^[[<..M` text) but leaves mouse
            // tracking set — so forcing raw mode back on is all we need. It has
            // to be a disable→enable toggle: crossterm's enable_raw_mode() alone
            // no-ops here because it still believes raw mode is on.
            force_raw_mode();
            raw_guard -= 1;
            // Repaint periodically to wipe whatever echoed before we caught it.
            if raw_guard == 0 || raw_guard % 5 == 0 {
                terminal.reassert(app.mouse_capture)?;
                force_redraw = true;
            }
        }
        if force_redraw || app.is_dirty() {
            terminal.draw(&mut app)?;
            app.clear_dirty();
        }
        if should_quit {
            break;
        }
    }
    Ok(())
}

/// Collect events until the queue is quiet for the frame budget.
/// Content-bearing engine events paint immediately without waiting.
async fn coalesce_events(rx: &mut mpsc::Receiver<AppEvent>, first: AppEvent) -> Vec<AppEvent> {
    let urgent = is_immediate_paint(&first);
    let deadline = if urgent {
        Instant::now()
    } else {
        Instant::now() + FRAME_BUDGET
    };
    let mut batch = vec![first];
    loop {
        while let Ok(event) = rx.try_recv() {
            batch.push(event);
        }
        if Instant::now() >= deadline {
            break;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(event)) => batch.push(event),
            _ => break,
        }
    }
    batch
}

/// Whether the first event in a batch should skip the frame coalesce window.
fn is_immediate_paint(event: &AppEvent) -> bool {
    match event {
        AppEvent::Engine(session) => matches!(
            session.payload,
            agentloop_contracts::AgentEvent::MessageStarted { .. }
                | agentloop_contracts::AgentEvent::MarkdownDelta { .. }
                | agentloop_contracts::AgentEvent::ThinkingDelta { .. }
                | agentloop_contracts::AgentEvent::TextSnapshot { .. }
                | agentloop_contracts::AgentEvent::ToolProgress { .. }
                | agentloop_contracts::AgentEvent::ToolCallUpdated { .. }
        ),
        _ => false,
    }
}

fn apply_terminal_effects(
    terminal: &mut TerminalSession,
    app: &mut App,
    effects: &[Effect],
) -> Result<()> {
    for effect in effects {
        match effect {
            Effect::SetMouseCapture(enabled) => {
                terminal.set_mouse_capture(*enabled)?;
            }
            Effect::CopyToClipboard { text } => match crate::clipboard::copy_text(text) {
                Ok(()) => {
                    app.toast("chat copied to clipboard");
                }
                Err(err) => {
                    app.chat.push_error(format!("clipboard copy failed: {err}"));
                }
            },
            Effect::SaveLastModel(model) => {
                if let Err(err) = CliPrefs::remember_model(model) {
                    tracing::warn!(target: "prefs", "failed to save last model: {err}");
                }
            }
            _ => {}
        }
    }
    Ok(())
}

async fn run_auth_login() -> Result<()> {
    let (tx, mut rx) = mpsc::channel::<LoginEvent>(16);
    let cancel = CancellationToken::new();
    let mut login = Box::pin(tokio::spawn(login_copilot(tx, cancel)));
    let mut out = io::stdout();

    loop {
        tokio::select! {
            event = rx.recv() => {
                let Some(event) = event else {
                    continue;
                };
                match event {
                    LoginEvent::CodeReady { user_code, verification_uri, expires_in } => {
                        writeln!(
                            out,
                            "Open {verification_uri} and enter code {user_code} (expires in {expires_in}s)."
                        )?;
                        out.flush()?;
                    }
                    LoginEvent::Polling => {
                        writeln!(out, "Waiting for GitHub confirmation...")?;
                    }
                    LoginEvent::Verifying => {
                        writeln!(out, "Verifying Copilot access...")?;
                    }
                    LoginEvent::Succeeded => {
                        writeln!(out, "Signed in to GitHub Copilot.")?;
                    }
                    _ => {}
                }
            }
            result = &mut login => {
                let outcome = result.context("login task panicked")?;
                outcome.map_err(anyhow::Error::from)?;
                return Ok(());
            }
        }
    }
}

async fn bootstrap_session(
    hub: &mut EngineHub,
    kind: AgentKind,
    model: Option<ModelRef>,
) -> Result<(SessionController, EngineEventStream, SessionBootstrap)> {
    let service = hub.service(kind).await?;
    let hello = service.hello();
    let providers = service
        .provider_registry()
        .ids()
        .into_iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    let trace = hub.trace(kind).to_vec();
    let maybe_session = hub.last_session(kind).cloned();
    let (controller, events, transcript) = match maybe_session {
        Some(session) => {
            let (controller, events) = SessionController::resume(service, session).await?;
            let transcript = controller.transcript().await.ok();
            (controller, events, transcript)
        }
        None => {
            let params = NewSessionParams {
                cwd: Some(hub.cwd().to_path_buf()),
                model: model.clone(),
                ..NewSessionParams::default()
            };
            let (controller, events) = SessionController::open(service, params).await?;
            (controller, events, None)
        }
    };
    let session = controller.session_id().clone();
    hub.remember_session(kind, session.clone());
    let mcp_enabled = if kind == AgentKind::Native {
        McpStore::load().enabled_count()
    } else {
        0
    };
    let bootstrap = SessionBootstrap {
        kind,
        hello,
        session,
        providers,
        model,
        transcript,
        trace,
        permission_mode: None,
        mcp_enabled,
        session_restarted: false,
    };
    Ok((controller, events, bootstrap))
}

/// Pick the session model after bootstrap: honor `--model`, else restore a
/// saved preference when its provider is still registered.
fn resolve_startup_model(
    cli_model: Option<&ModelRef>,
    saved_model: Option<&str>,
    bootstrap: &SessionBootstrap,
) -> Option<ModelRef> {
    if let Some(model) = cli_model {
        return Some(model.clone());
    }
    let stored = saved_model?;
    resolve_stored_model(stored, &bootstrap.providers, None)
}

fn spawn_engine_forwarder(mut events: EngineEventStream, tx: mpsc::Sender<AppEvent>) {
    tokio::spawn(async move {
        while let Some(event) = events.next().await {
            if tx.send(AppEvent::Engine(Box::new(event))).await.is_err() {
                break;
            }
        }
    });
}

fn spawn_terminal_forwarder(tx: mpsc::Sender<AppEvent>) {
    tokio::spawn(async move {
        let mut events = EventStream::new();
        while let Some(event) = events.next().await {
            let Ok(event) = event else {
                continue;
            };
            if tx.send(AppEvent::Term(event)).await.is_err() {
                break;
            }
        }
    });
}

fn spawn_tick_forwarder(tx: mpsc::Sender<AppEvent>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(TICK_MS));
        loop {
            tick.tick().await;
            if tx.send(AppEvent::Tick).await.is_err() {
                break;
            }
        }
    });
}

fn spawn_interrupt_forwarder(tx: mpsc::Sender<AppEvent>) {
    tokio::spawn(async move {
        loop {
            if tokio::signal::ctrl_c().await.is_err() {
                break;
            }
            if tx.send(AppEvent::Interrupt).await.is_err() {
                break;
            }
        }
    });
}

/// Restore the terminal and exit on a terminate-class signal (SIGTERM, SIGHUP,
/// SIGQUIT). These bypass both `Drop` and the panic hook — a `kill`, a closed
/// terminal tab, or Ctrl+\ would otherwise leave raw mode and mouse tracking
/// on, so the shell echoes escape sequences on every scroll/click. We restore
/// directly to stdout (like the panic hook) and exit, which works even if the
/// event loop is wedged. Ctrl+C keeps its in-app meaning: raw mode delivers it
/// as a key, and SIGINT is handled by `spawn_interrupt_forwarder`.
#[cfg(unix)]
fn spawn_signal_restore() {
    use tokio::signal::unix::{SignalKind, signal};
    // Terminate-class: restore the terminal and exit cleanly.
    for kind in [
        SignalKind::terminate(),
        SignalKind::hangup(),
        SignalKind::quit(),
    ] {
        tokio::spawn(async move {
            let Ok(mut stream) = signal(kind) else {
                return;
            };
            if stream.recv().await.is_some() {
                restore_terminal(&mut io::stdout());
                std::process::exit(0);
            }
        });
    }
    // Swallow the job-control STOP signals. Just registering a handler
    // suppresses their default "stop the process" action — otherwise the app
    // freezes on its last frame while the shell takes back cooked-mode input,
    // yet raw mode and mouse tracking are still on, so the shell echoes escape
    // sequences on every scroll/click. Keeping the app alive avoids that:
    //   • SIGTSTP — Ctrl+Z.
    //   • SIGTTIN / SIGTTOU — the app briefly reads/writes the tty from the
    //     background, e.g. while an engine/MCP reload spawns a child that grabs
    //     the controlling terminal. This is the one that bites after a
    //     provider switch ("MCP servers reloaded" then garbage on scroll).
    // Quit with Ctrl+C / Ctrl+D / `/quit`, or `kill` (SIGTERM restores above).
    #[cfg(target_os = "macos")]
    const SIGTSTP_RAW: i32 = 18;
    #[cfg(not(target_os = "macos"))]
    const SIGTSTP_RAW: i32 = 20;
    const SIGTTIN_RAW: i32 = 21;
    const SIGTTOU_RAW: i32 = 22;
    for raw in [SIGTSTP_RAW, SIGTTIN_RAW, SIGTTOU_RAW] {
        tokio::spawn(async move {
            let Ok(mut stream) = signal(SignalKind::from_raw(raw)) else {
                return;
            };
            while stream.recv().await.is_some() {
                // Intentionally ignored — see above.
            }
        });
    }
}

#[cfg(not(unix))]
fn spawn_signal_restore() {}

/// Executes reducer effects on background tasks.
pub struct EffectExecutor {
    hub: Arc<Mutex<EngineHub>>,
    controller: Arc<Mutex<SessionController>>,
    current_kind: Arc<Mutex<AgentKind>>,
    tx: mpsc::Sender<AppEvent>,
    login_cancel: Arc<Mutex<Option<CancellationToken>>>,
    shell_cancel: Arc<Mutex<Option<CancellationToken>>>,
}

impl EffectExecutor {
    fn new(
        hub: EngineHub,
        controller: SessionController,
        kind: AgentKind,
        tx: mpsc::Sender<AppEvent>,
    ) -> Self {
        Self {
            hub: Arc::new(Mutex::new(hub)),
            controller: Arc::new(Mutex::new(controller)),
            current_kind: Arc::new(Mutex::new(kind)),
            tx,
            login_cancel: Arc::new(Mutex::new(None)),
            shell_cancel: Arc::new(Mutex::new(None)),
        }
    }

    fn execute_all(&self, effects: Vec<Effect>) {
        for effect in effects {
            self.execute(effect);
        }
    }

    fn execute(&self, effect: Effect) {
        match effect {
            Effect::SubmitPrompt { input, opts } => {
                let controller = self.controller.clone();
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let (service, session) = current_service_session(&controller).await;
                    let result = service
                        .prompt(&session, input, opts)
                        .await
                        .map_err(|err| err.to_string());
                    let _ = tx
                        .send(AppEvent::Task(TaskResult::TurnFinished(result)))
                        .await;
                });
            }
            Effect::CompactSession { opts } => {
                let controller = self.controller.clone();
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let controller = controller.lock().await;
                    let result = controller
                        .compact(opts)
                        .await
                        .map_err(|err| err.to_string());
                    let _ = tx
                        .send(AppEvent::Task(TaskResult::CompactFinished(result)))
                        .await;
                });
            }
            Effect::CancelTurn => {
                let controller = self.controller.clone();
                tokio::spawn(async move {
                    let (service, session) = current_service_session(&controller).await;
                    let _ = service.cancel(&session).await;
                });
            }
            Effect::RespondPermission {
                id,
                decision,
                session: target,
            } => {
                let controller = self.controller.clone();
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let (service, current) = current_service_session(&controller).await;
                    // Relayed subagent prompts carry the child session id;
                    // the agent-global pending map resolves it directly.
                    let session = target.unwrap_or(current);
                    let result = service
                        .respond_permission(&session, id.clone(), decision.clone())
                        .await;
                    if let Err(err) = result {
                        let _ = tx
                            .send(AppEvent::Task(TaskResult::PermissionRespondFailed {
                                message: err.to_string(),
                            }))
                            .await;
                    }
                });
            }
            Effect::SetTurnPermissionMode { mode } => {
                let controller = self.controller.clone();
                tokio::spawn(async move {
                    let (service, session) = current_service_session(&controller).await;
                    let _ = service.set_turn_permission_mode(&session, mode);
                });
            }
            Effect::RespondQuestion {
                id,
                answers,
                session: target,
            } => {
                let controller = self.controller.clone();
                tokio::spawn(async move {
                    let (service, current) = current_service_session(&controller).await;
                    let session = target.unwrap_or(current);
                    let _ = service.respond_question(&session, id, answers).await;
                });
            }
            Effect::ListModels => {
                let controller = self.controller.clone();
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let registry = {
                        let controller = controller.lock().await;
                        controller.service().provider_registry().clone()
                    };
                    let catalog = ModelCatalog::fetch(&registry).await;
                    for (provider, message) in &catalog.errors {
                        tracing::warn!(target: "catalog", provider = %provider, "{message}");
                    }
                    let _ = tx
                        .send(AppEvent::Task(TaskResult::Models(Ok(catalog.entries))))
                        .await;
                });
            }
            Effect::SwitchAgent { kind, invalidate } => {
                let hub = self.hub.clone();
                let controller = self.controller.clone();
                let current_kind = self.current_kind.clone();
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let result =
                        switch_agent(hub, controller, current_kind, kind, invalidate, tx.clone())
                            .await;
                    let _ = tx
                        .send(AppEvent::Task(TaskResult::EngineSwitched(Box::new(result))))
                        .await;
                });
            }
            Effect::NewSession => {
                let controller = self.controller.clone();
                let hub = self.hub.clone();
                let current_kind = self.current_kind.clone();
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let result = replace_with_fresh_session(
                        controller,
                        hub,
                        current_kind,
                        tx.clone(),
                        false,
                    )
                    .await;
                    let _ = tx
                        .send(AppEvent::Task(TaskResult::SessionReset(result)))
                        .await;
                });
            }
            Effect::ClearSession => {
                let controller = self.controller.clone();
                let hub = self.hub.clone();
                let current_kind = self.current_kind.clone();
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let result =
                        replace_with_fresh_session(controller, hub, current_kind, tx.clone(), true)
                            .await;
                    let _ = tx
                        .send(AppEvent::Task(TaskResult::SessionCleared(result)))
                        .await;
                });
            }
            Effect::StartLogin => {
                let tx = self.tx.clone();
                let cancel_slot = self.login_cancel.clone();
                tokio::spawn(async move {
                    let (progress_tx, mut progress_rx) = mpsc::channel(16);
                    let cancel = CancellationToken::new();
                    {
                        let mut slot = cancel_slot.lock().await;
                        *slot = Some(cancel.clone());
                    }
                    let progress_events = tx.clone();
                    tokio::spawn(async move {
                        while let Some(event) = progress_rx.recv().await {
                            if progress_events.send(AppEvent::Login(event)).await.is_err() {
                                break;
                            }
                        }
                    });
                    let result = login_copilot(progress_tx, cancel)
                        .await
                        .map_err(|err| err.to_string());
                    {
                        let mut slot = cancel_slot.lock().await;
                        *slot = None;
                    }
                    let _ = tx
                        .send(AppEvent::Task(TaskResult::LoginFinished(result)))
                        .await;
                });
            }
            Effect::CancelLogin => {
                let cancel_slot = self.login_cancel.clone();
                tokio::spawn(async move {
                    if let Some(cancel) = cancel_slot.lock().await.take() {
                        cancel.cancel();
                    }
                });
            }
            Effect::RunShellCommand { command } => {
                let hub = self.hub.clone();
                let tx = self.tx.clone();
                let cancel_slot = self.shell_cancel.clone();
                tokio::spawn(async move {
                    let cancel = CancellationToken::new();
                    {
                        let mut slot = cancel_slot.lock().await;
                        *slot = Some(cancel.clone());
                    }
                    let cwd = {
                        let hub = hub.lock().await;
                        hub.cwd().to_path_buf()
                    };
                    let outcome = run_shell_command(&command, &cwd, cancel).await;
                    {
                        let mut slot = cancel_slot.lock().await;
                        *slot = None;
                    }
                    let _ = tx
                        .send(AppEvent::Task(TaskResult::ShellCommand {
                            command,
                            outcome,
                        }))
                        .await;
                });
            }
            Effect::CancelShellCommand => {
                let cancel_slot = self.shell_cancel.clone();
                tokio::spawn(async move {
                    if let Some(cancel) = cancel_slot.lock().await.take() {
                        cancel.cancel();
                    }
                });
            }
            Effect::Resync { from_seq } => {
                let controller = self.controller.clone();
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let result = {
                        let controller = controller.lock().await;
                        controller.replay(from_seq).await
                    };
                    if result.is_ok() {
                        let transcript = {
                            let controller = controller.lock().await;
                            controller.transcript().await
                        }
                        .map_err(|err| err.to_string());
                        let _ = tx
                            .send(AppEvent::Task(TaskResult::Resynced(transcript)))
                            .await;
                    }
                });
            }
            Effect::OpenBrowser { url } => {
                tokio::spawn(async move {
                    let _ = open_url(&url);
                });
            }
            Effect::ReloadEngine { invalidate } => {
                let hub = self.hub.clone();
                let controller = self.controller.clone();
                let current_kind = self.current_kind.clone();
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let result =
                        reload_engine(hub, controller, current_kind, invalidate, tx.clone()).await;
                    let _ = tx
                        .send(AppEvent::Task(TaskResult::EngineReloaded(Box::new(result))))
                        .await;
                });
            }
            Effect::McpInstall {
                target,
                registry_id,
                import_path,
            } => {
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        run_mcp_install(target, registry_id, import_path)
                    })
                    .await
                    .map_err(|err| err.to_string())
                    .and_then(|inner| inner);
                    let _ = tx
                        .send(AppEvent::Task(TaskResult::McpInstallFinished(result)))
                        .await;
                });
            }
            Effect::McpListTools { server } => {
                let hub = self.hub.clone();
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let result = list_mcp_tools(hub, &server).await;
                    let _ = tx
                        .send(AppEvent::Task(TaskResult::McpToolsListed {
                            server,
                            result,
                        }))
                        .await;
                });
            }
            Effect::McpCallTool {
                server,
                tool,
                args_json,
            } => {
                let hub = self.hub.clone();
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let result = call_mcp_tool(hub, &server, &tool, &args_json).await;
                    let _ = tx
                        .send(AppEvent::Task(TaskResult::McpToolCalled {
                            server,
                            tool,
                            result,
                        }))
                        .await;
                });
            }
            Effect::ValidateProvider { id, config } => {
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let (config, result) =
                        match agentloop_cli_core::validate_provider(&id, &config).await {
                            Ok(models) => {
                                let mut config = config;
                                if config.models.is_empty() {
                                    config.models = models
                                        .iter()
                                        .map(|model| agentloop_cli_core::ModelEntry {
                                            id: model.id.clone(),
                                            name: model.display_name.clone(),
                                            context_window: model.context_window,
                                        })
                                        .collect();
                                }
                                let count = config.models.len();
                                (config, Ok(count))
                            }
                            Err(message) => (config, Err(message)),
                        };
                    let _ = tx
                        .send(AppEvent::Task(TaskResult::ProviderValidated {
                            id,
                            config,
                            result,
                        }))
                        .await;
                });
            }
            Effect::Quit
            | Effect::SetMouseCapture(_)
            | Effect::CopyToClipboard { .. }
            | Effect::SaveLastModel(_) => {}
        }
    }
}

async fn current_service_session(
    controller: &Arc<Mutex<SessionController>>,
) -> (agentloop_engine::EngineService, SessionId) {
    let controller = controller.lock().await;
    (
        controller.service().clone(),
        controller.session_id().clone(),
    )
}

async fn replace_with_fresh_session(
    controller: Arc<Mutex<SessionController>>,
    hub: Arc<Mutex<EngineHub>>,
    current_kind: Arc<Mutex<AgentKind>>,
    tx: mpsc::Sender<AppEvent>,
    cancel_first: bool,
) -> Result<SessionId, String> {
    if cancel_first {
        let (service, session) = current_service_session(&controller).await;
        let _ = service.cancel(&session).await;
    }
    let service = {
        let controller = controller.lock().await;
        controller.service().clone()
    };
    let (cwd, kind) = {
        let hub = hub.lock().await;
        (Some(hub.cwd().to_path_buf()), *current_kind.lock().await)
    };
    let params = NewSessionParams {
        cwd,
        ..NewSessionParams::default()
    };
    let (next, events) = SessionController::open(service, params)
        .await
        .map_err(|err| err.to_string())?;
    let id = next.session_id().clone();
    {
        let mut controller = controller.lock().await;
        *controller = next;
    }
    {
        let mut hub = hub.lock().await;
        hub.remember_session(kind, id.clone());
    }
    spawn_engine_forwarder(events, tx);
    Ok(id)
}

async fn switch_agent(
    hub: Arc<Mutex<EngineHub>>,
    controller: Arc<Mutex<SessionController>>,
    current_kind: Arc<Mutex<AgentKind>>,
    kind: AgentKind,
    invalidate: bool,
    tx: mpsc::Sender<AppEvent>,
) -> Result<SessionBootstrap, String> {
    let previous_session = {
        let controller = controller.lock().await;
        controller.session_id().clone()
    };
    let previous_kind = *current_kind.lock().await;
    let mut hub = hub.lock().await;
    hub.remember_session(previous_kind, previous_session);
    if invalidate {
        hub.invalidate(kind);
    }
    let (next, events, bootstrap) = bootstrap_session(&mut hub, kind, None)
        .await
        .map_err(|err| err.to_string())?;
    {
        let mut controller = controller.lock().await;
        *controller = next;
    }
    {
        let mut current_kind = current_kind.lock().await;
        *current_kind = kind;
    }
    spawn_engine_forwarder(events, tx);
    Ok(bootstrap)
}

async fn reload_engine(
    hub: Arc<Mutex<EngineHub>>,
    controller: Arc<Mutex<SessionController>>,
    current_kind: Arc<Mutex<AgentKind>>,
    invalidate: bool,
    tx: mpsc::Sender<AppEvent>,
) -> Result<SessionBootstrap, String> {
    let kind = *current_kind.lock().await;
    if kind != AgentKind::Native {
        return Err("MCP reload requires the native agent".to_owned());
    }
    let (session, transcript_fallback, cwd) = {
        let controller_guard = controller.lock().await;
        {
            let service = controller_guard.service();
            // Best-effort: stop any in-flight turn before swapping engine instances.
            let _ = service.cancel(controller_guard.session_id()).await;
        }
        let session = controller_guard.session_id().clone();
        let transcript_fallback = controller_guard.transcript().await.ok();
        let cwd = {
            let hub_guard = hub.lock().await;
            hub_guard.cwd().to_path_buf()
        };
        (session, transcript_fallback, cwd)
    };
    let mut hub_guard = hub.lock().await;
    hub_guard.remember_session(kind, session.clone());
    if invalidate {
        hub_guard.invalidate(AgentKind::Native);
    }
    let service = hub_guard
        .service(AgentKind::Native)
        .await
        .map_err(|err| err.to_string())?;
    let hello = service.hello();
    let providers = service
        .provider_registry()
        .ids()
        .into_iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    let trace = hub_guard.trace(AgentKind::Native).to_vec();
    let mcp_enabled = McpStore::load().enabled_count();
    drop(hub_guard);

    let resume_result = SessionController::resume(service.clone(), session.clone()).await;
    let (next, events, session_restarted) = match resume_result {
        Ok(pair) => (pair.0, pair.1, false),
        Err(_) => {
            let params = NewSessionParams {
                cwd: Some(cwd),
                ..NewSessionParams::default()
            };
            let (opened, events) = SessionController::open(service, params)
                .await
                .map_err(|err| err.to_string())?;
            (opened, events, true)
        }
    };
    let transcript = next.transcript().await.ok().or(transcript_fallback);
    let session_id = next.session_id().clone();
    {
        let mut controller = controller.lock().await;
        *controller = next;
    }
    {
        let mut hub_guard = hub.lock().await;
        hub_guard.remember_session(kind, session_id.clone());
    }
    spawn_engine_forwarder(events, tx);
    Ok(SessionBootstrap {
        kind: AgentKind::Native,
        hello,
        session: session_id,
        providers,
        model: None,
        transcript,
        trace,
        permission_mode: None,
        mcp_enabled,
        session_restarted,
    })
}

fn run_mcp_install(
    target: InstallTarget,
    registry_id: Option<String>,
    import_path: Option<std::path::PathBuf>,
) -> Result<String, String> {
    let mut store = McpStore::load();
    let name = if let Some(id) = registry_id {
        store.install_registry(&id).map_err(|err| err.to_string())?
    } else if let Some(path) = import_path {
        let added = store
            .import_from_file(&path)
            .map_err(|err| err.to_string())?;
        if added.is_empty() {
            return Err("no new servers imported (duplicates skipped)".to_owned());
        }
        added
            .into_iter()
            .next()
            .ok_or_else(|| "import produced no servers".to_owned())?
    } else {
        match target {
            InstallTarget::GitHub(repo) => {
                store.install_github(&repo).map_err(|err| err.to_string())?
            }
            InstallTarget::Npm(package) => store
                .install_npm(&package, None)
                .map_err(|err| err.to_string())?,
            InstallTarget::Unknown => {
                return Err("missing install target".to_owned());
            }
        }
    };
    store.save().map_err(|err| err.to_string())?;
    Ok(name)
}

async fn list_mcp_tools(
    hub: Arc<Mutex<EngineHub>>,
    server: &str,
) -> Result<Vec<agentloop_mcp::McpRemoteTool>, String> {
    let hub = hub.lock().await;
    let manager = hub
        .mcp_manager()
        .ok_or_else(|| "native engine has no MCP manager — enable servers and reload".to_owned())?;
    let cancel = CancellationToken::new();
    manager
        .list_server_tools(server, cancel)
        .await
        .map_err(|err| err.to_string())
}

async fn call_mcp_tool(
    hub: Arc<Mutex<EngineHub>>,
    server: &str,
    tool: &str,
    args_json: &str,
) -> Result<String, String> {
    let input: serde_json::Value = if args_json.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(args_json).map_err(|err| format!("invalid JSON args: {err}"))?
    };
    let hub = hub.lock().await;
    let manager = hub
        .mcp_manager()
        .ok_or_else(|| "native engine has no MCP manager".to_owned())?;
    let cancel = CancellationToken::new();
    let output = manager
        .call_server_tool(server, tool, input, cancel)
        .await
        .map_err(|err| err.to_string())?;
    Ok(output.render_text())
}

fn open_url(url: &str) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = ProcessCommand::new("open");
    #[cfg(target_os = "linux")]
    let mut command = ProcessCommand::new("xdg-open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = ProcessCommand::new("cmd");
        command.arg("/C").arg("start");
        command
    };
    command.arg(url).spawn().map(|_| ())
}

async fn run_shell_command(
    command: &str,
    cwd: &std::path::Path,
    cancel: CancellationToken,
) -> ShellCommandOutcome {
    #[cfg(windows)]
    let mut cmd = {
        let mut cmd = tokio::process::Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd
    };

    cmd.current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            return ShellCommandOutcome::Failed {
                message: err.to_string(),
            };
        }
    };

    let mut child = Some(child);
    tokio::select! {
        _ = cancel.cancelled() => {
            if let Some(mut running) = child.take() {
                let _ = running.start_kill();
                let _ = running.wait().await;
            }
            ShellCommandOutcome::Cancelled {
                partial_output: String::new(),
            }
        }
        result = async {
            let running = child.take().ok_or_else(|| {
                std::io::Error::other("shell command already taken")
            })?;
            running.wait_with_output().await
        } => match result {
            Ok(output) => {
                let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.is_empty() {
                    if !combined.is_empty() && !combined.ends_with('\n') {
                        combined.push('\n');
                    }
                    combined.push_str(&stderr);
                }
                ShellCommandOutcome::Completed {
                    output: combined,
                    exit_code: output.status.code(),
                }
            }
            Err(err) => ShellCommandOutcome::Failed {
                message: err.to_string(),
            },
        },
    }
}

/// XTerm "alternate scroll mode": while the alternate screen buffer is
/// active, the terminal translates trackpad/wheel scroll into events for the
/// foreground app (or arrow-key sequences) instead of applying its own
/// native scrollback/rubber-band to the window. `crossterm::EnableMouseCapture`
/// does NOT send this — it only sets modes 1000/1002/1003/1015/1006, which
/// report clicks and drags but say nothing about wheel-scroll routing. Without
/// it, terminals like Terminal.app/iTerm2 can still apply their own scroll
/// physics on top of (or instead of) forwarding wheel events to us, which
/// looks like blank scrollback rows bleeding into the alternate screen when
/// scrolling. `vim`, `tmux`, and `less` all send this alongside mouse capture
/// for the same reason.
const ENABLE_ALTERNATE_SCROLL: &str = "\x1b[?1007h";
const DISABLE_ALTERNATE_SCROLL: &str = "\x1b[?1007l";

/// Mouse tracking: button + wheel only (1000) with SGR extended coordinates
/// (1006). Deliberately NOT crossterm's `EnableMouseCapture`, which also sets
/// 1002 (button-event motion) and 1003 (ANY motion). We only handle wheel
/// scroll, so motion reporting is pure overhead — and worse, if the process
/// ever dies with tracking still on, `?1003h` makes every mouse *move* spew
/// escape sequences into the shell. 1000+1006 gives wheel events with zero
/// motion traffic; crossterm's event reader parses the SGR reports the same
/// way regardless of how the mode was turned on. The disable sequence clears
/// 1000/1002/1003/1006 so it also tears down any legacy full-capture state.
const ENABLE_MOUSE_TRACKING: &str = "\x1b[?1000h\x1b[?1006h";
const DISABLE_MOUSE_TRACKING: &str = "\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l";

/// Force raw mode back on, beating crossterm's `enable_raw_mode()` — which
/// no-ops once its internal flag says raw is on, so it can't undo a termios
/// reset done *outside* crossterm (an MCP child resetting the shared tty).
/// The disable→enable toggle clears that flag and re-applies raw; the saved
/// "restore on exit" mode is preserved because `disable_raw_mode` puts back the
/// original before `enable_raw_mode` re-samples it. Best-effort; errors ignored.
fn force_raw_mode() {
    let _ = disable_raw_mode();
    let _ = enable_raw_mode();
}

/// Best-effort terminal restore, safe to call from a panic hook or `Drop`.
/// Idempotent — running it twice does no harm.
fn restore_terminal(out: &mut impl Write) {
    let _ = out.write_all(DISABLE_MOUSE_TRACKING.as_bytes());
    let _ = out.write_all(DISABLE_ALTERNATE_SCROLL.as_bytes());
    let _ = out.flush();
    let _ = execute!(out, DisableBracketedPaste, LeaveAlternateScreen);
    let _ = disable_raw_mode();
}

/// Restore the terminal on a *fatal* panic before the default hook prints, so
/// the crash message lands on a clean screen. Only the main thread's panic is
/// fatal here (`#[tokio::main]` drives `run_tui` via `block_on` on the main
/// thread); a panicking tokio worker task is caught by the runtime and the
/// process keeps running, so restoring the terminal there would tear the live
/// UI out of the alternate screen mid-session. Skip those.
fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().name() == Some("main") {
            restore_terminal(&mut io::stdout());
        }
        original(info);
    }));
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalSession {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("enable raw mode")?;
        let mut stdout = io::stdout();
        // Enter the alternate screen and turn on bracketed paste so a pasted
        // block arrives as one `Event::Paste` instead of keystrokes — otherwise
        // a newline in the paste reads as Enter and submits the prompt. Mouse
        // capture is synced separately by the caller (default on). Alternate-
        // scroll (1007) is enabled as a fallback for the capture-off state on
        // terminals that honor it.
        execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)
            .context("enter alternate screen")?;
        stdout
            .write_all(ENABLE_ALTERNATE_SCROLL.as_bytes())
            .and_then(|()| stdout.flush())
            .context("enable alternate scroll mode")?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).context("create terminal")?;
        Ok(Self { terminal })
    }

    fn draw(&mut self, app: &mut App) -> Result<()> {
        self.terminal
            .draw(|frame| crate::ui::draw(frame, app))
            .context("draw frame")?;
        Ok(())
    }

    /// Toggle mouse tracking (button + wheel only — see [`ENABLE_MOUSE_TRACKING`]).
    /// Alternate-scroll mode stays on for the whole session (set in
    /// [`Self::enter`], cleared in `Drop`), so wheel scrolling keeps working in
    /// either state; this just decides whether the wheel drives the transcript
    /// (on) or the terminal does native selection/scroll (off).
    fn set_mouse_capture(&mut self, enabled: bool) -> Result<()> {
        let backend = self.terminal.backend_mut();
        let seq = if enabled {
            ENABLE_MOUSE_TRACKING
        } else {
            DISABLE_MOUSE_TRACKING
        };
        backend
            .write_all(seq.as_bytes())
            .and_then(|()| backend.flush())
            .context("set mouse tracking")?;
        Ok(())
    }

    /// Re-establish our terminal modes after something external may have
    /// disturbed them — e.g. an MCP server (a child process) restarted during
    /// an engine reload and reset the shared terminal's termios out of raw
    /// mode, which would otherwise leave the shell echoing mouse escape
    /// sequences on scroll while we keep running. Idempotent, so it's safe to
    /// call whenever a reload lands even if nothing was disturbed.
    fn reassert(&mut self, mouse_capture: bool) -> Result<()> {
        // Force raw back on (see `force_raw_mode`) — a plain enable_raw_mode()
        // would no-op if a child already reset termios while crossterm's flag
        // still reads "raw on".
        force_raw_mode();
        let backend = self.terminal.backend_mut();
        execute!(backend, EnterAlternateScreen, EnableBracketedPaste)
            .context("re-enter alternate screen")?;
        backend
            .write_all(ENABLE_ALTERNATE_SCROLL.as_bytes())
            .and_then(|()| backend.flush())
            .context("re-enable alternate scroll mode")?;
        self.set_mouse_capture(mouse_capture)?;
        self.terminal.clear().context("clear after reassert")?;
        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        restore_terminal(self.terminal.backend_mut());
        let _ = self.terminal.show_cursor();
    }
}

#[cfg(test)]
mod coalesce_tests {
    use agentloop_contracts::{AgentEvent, MessageId, Role, SessionEvent, SessionId, TurnId};

    use crate::events::AppEvent;

    use super::is_immediate_paint;

    fn engine_event(payload: AgentEvent) -> AppEvent {
        AppEvent::Engine(Box::new(SessionEvent {
            session_id: SessionId::from("sess"),
            turn_id: Some(TurnId::from("turn")),
            seq: 1,
            ts_ms: 0,
            payload,
        }))
    }

    #[test]
    fn markdown_delta_paints_immediately() {
        assert!(is_immediate_paint(&engine_event(
            AgentEvent::MarkdownDelta {
                message_id: MessageId::from("m1"),
                text: "hi".to_owned(),
            }
        )));
    }

    #[test]
    fn thinking_delta_paints_immediately() {
        assert!(is_immediate_paint(&engine_event(
            AgentEvent::ThinkingDelta {
                message_id: MessageId::from("m1"),
                text: "hmm".to_owned(),
            }
        )));
    }

    #[test]
    fn tick_is_not_immediate() {
        assert!(!is_immediate_paint(&AppEvent::Tick));
    }

    #[test]
    fn message_started_paints_immediately() {
        assert!(is_immediate_paint(&engine_event(
            AgentEvent::MessageStarted {
                message_id: MessageId::from("m1"),
                role: Role::Assistant,
            }
        )));
    }
}
