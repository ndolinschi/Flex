//! Tokio + terminal runtime: wire reducer effects to engine operations.

use std::io::{self, Write};
use std::process::Command as ProcessCommand;
use std::sync::Arc;
use std::time::Duration;

use std::time::Instant;

use anyhow::{Context, Result};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, EventStream};
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
    AgentKind, CliPrefs, EngineHub, LoginEvent, ModelCatalog, SessionController, login_copilot,
    resolve_stored_model,
};
use agentloop_contracts::{ModelRef, NewSessionParams, SessionId};
use agentloop_core::EventStream as EngineEventStream;

use crate::app::App;
use crate::args::{Args, Command};
use crate::events::{AppEvent, Effect, SessionBootstrap, ShellCommandOutcome, TaskResult};
use crate::files::FileIndex;

const EVENT_QUEUE: usize = 1024;
const TICK_MS: u64 = 100;
const FRAME_BUDGET: Duration = Duration::from_millis(16);

/// Run either the TUI or the headless auth subcommand.
pub async fn run(args: Args) -> Result<()> {
    match args.command {
        Command::Tui => run_tui(args).await,
        Command::AuthLogin => run_auth_login().await,
    }
}

async fn run_tui(args: Args) -> Result<()> {
    let (tx, mut rx) = mpsc::channel(EVENT_QUEUE);
    let mut hub = EngineHub::new(
        args.workdir.clone(),
        args.provider.clone(),
        args.model.clone(),
    );
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
    let executor = EffectExecutor::new(hub, controller, args.agent, tx.clone());
    let mut terminal = TerminalSession::enter()?;
    terminal.draw(&mut app)?;

    while let Some(first) = rx.recv().await {
        let batch = coalesce_events(&mut rx, first).await;
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
        if app.is_dirty() {
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
    let bootstrap = SessionBootstrap {
        kind,
        hello,
        session,
        providers,
        model,
        transcript,
        trace,
        permission_mode: None,
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
            Effect::RespondPermission { id, decision } => {
                let controller = self.controller.clone();
                tokio::spawn(async move {
                    let (service, session) = current_service_session(&controller).await;
                    let _ = service.respond_permission(&session, id, decision).await;
                });
            }
            Effect::RespondQuestion { id, answers } => {
                let controller = self.controller.clone();
                tokio::spawn(async move {
                    let (service, session) = current_service_session(&controller).await;
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
                    let result = async {
                        let (next, events) = SessionController::open(service, params).await?;
                        let id = next.session_id().clone();
                        {
                            let mut controller = controller.lock().await;
                            *controller = next;
                        }
                        {
                            let mut hub = hub.lock().await;
                            hub.remember_session(kind, id.clone());
                        }
                        spawn_engine_forwarder(events, tx.clone());
                        Ok::<SessionId, agentloop_engine::EngineServiceError>(id)
                    }
                    .await
                    .map_err(|err| err.to_string());
                    let _ = tx
                        .send(AppEvent::Task(TaskResult::SessionReset(result)))
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

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalSession {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("enable raw mode")?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).context("enter alternate screen")?;
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

    fn set_mouse_capture(&mut self, enabled: bool) -> Result<()> {
        if enabled {
            execute!(self.terminal.backend_mut(), EnableMouseCapture)
                .context("enable mouse capture")?;
        } else {
            execute!(self.terminal.backend_mut(), DisableMouseCapture)
                .context("disable mouse capture")?;
        }
        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
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
