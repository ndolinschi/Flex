mod browser;
mod commands;
mod compose;
mod config;
mod debug;
mod error;
mod secrets;
mod state;
mod terminal;
mod win_console;

use tauri::{LogicalSize, Manager};
use tracing_subscriber::prelude::*;

use crate::compose::{build_service, open_session_store};
use crate::config::load_config;
use crate::state::AppState;

/// Must match `app.windows[0].minWidth`/`minHeight` in `tauri.conf.json`.
///
/// tauri-plugin-window-state's `on_window_ready` restore hook calls
/// `set_size` directly from whatever was last persisted to
/// `.window-state.json` — it does not consult the window's configured
/// min-size, and a programmatic `set_size` is not clamped by the OS/Tauri
/// the way an interactive user drag-resize is. So a size saved *before*
/// this minimum existed (or saved on a platform where the min wasn't
/// enforced) gets replayed verbatim on every future launch, silently
/// bypassing the constraint below. Re-assert and clamp here, in `setup`,
/// which runs after the plugin's window-ready restore.
const MAIN_WINDOW_MIN_WIDTH: f64 = 900.0;
const MAIN_WINDOW_MIN_HEIGHT: f64 = 600.0;

/// Trace/log to stdout (visible in the `tauri dev` console) AND to a rolling
/// daily file under the app's log dir (visible in packaged builds, where
/// stdout is discarded — see the module doc on why this matters). `RUST_LOG`
/// wins if set (standard `EnvFilter` syntax, e.g. `RUST_LOG=debug`);
/// otherwise the default filter is picked by whether "Debug logging" is
/// currently on in the frontend's Settings (read directly from its
/// persisted UI store — see `debug::is_debug_mode_enabled`): debug mode ON
/// raises the whole app to `debug`; OFF uses the previous useful default
/// (`info` + explicit `debug` for the engine/provider crates, which surface
/// request/stream failures without drowning in framework noise).
///
/// Runs inside `.setup()` (not before `tauri::Builder`) because it needs an
/// `AppHandle` to resolve the app's log dir via Tauri's path API — it still
/// runs before `build_service`, so failures there are captured same as
/// before. The file writer's `WorkerGuard` (must stay alive for the process
/// lifetime, or the background flush task stops — see
/// `tracing_appender::non_blocking`'s docs) is deliberately leaked rather
/// than threaded through `AppState`: this runs exactly once per process and
/// needs to outlive every other subsystem, including teardown-time logging.
fn init_tracing(app: &tauri::App) {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    let debug_mode = debug::is_debug_mode_enabled(&app_data_dir);

    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if debug_mode {
            tracing_subscriber::EnvFilter::new("debug")
        } else {
            tracing_subscriber::EnvFilter::new(
                "info,agentloop_loop=debug,agentloop_engine=debug,\
                 agentloop_providers=debug,agentloop_provider_bedrock=debug",
            )
        }
    });

    let stdout_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stdout);

    let log_dir = app
        .path()
        .app_log_dir()
        .unwrap_or_else(|_| app_data_dir.join("logs"));
    let _ = std::fs::create_dir_all(&log_dir);
    let file_appender = tracing_appender::rolling::daily(&log_dir, "flex-desktop.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    // Leaked deliberately: this must outlive every other subsystem (so late
    // shutdown logging still flushes) and there is exactly one per process.
    Box::leak(Box::new(guard));
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false);

    debug::set_log_file_path(log_dir.join(format!("flex-desktop.log.{}", chrono_today_suffix())));

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(stdout_layer)
        .with(file_layer)
        .try_init();

    tracing::info!(debug_mode, log_dir = %log_dir.display(), "tracing initialized");
}

/// `tracing_appender::rolling::daily` names files `<prefix>.<YYYY-MM-DD>`
/// but doesn't expose the exact path it's writing to — reconstruct today's
/// suffix (local date) purely for the Settings "copy log path" affordance;
/// this is cosmetic (worst case: the displayed path is a day stale right at
/// midnight), not load-bearing for logging itself.
fn chrono_today_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    // Civil-from-days (Howard Hinnant's algorithm) — avoids a chrono
    // dependency for this one cosmetic label.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_os::init())
        // Auto-update + relaunch. Signing key / Apple notarization still
        // gated on secrets (see release.yml TODOs) — the plugin is safe to
        // init unsigned; checks soft-fail until a signed latest.json exists.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // Needs `app`'s path resolver (Tauri's path API, not the raw
            // `dirs` crate) for the log dir, so this can't run before
            // `tauri::Builder` exists — see `init_tracing`'s doc comment.
            // Still runs first thing in `setup`, so failures during
            // `build_service` below are captured same as before.
            init_tracing(app);

            // Belt-and-suspenders against tauri-plugin-window-state replaying a
            // stale sub-minimum size from `.window-state.json` (see constants
            // above). Re-apply the min constraint and, if the size restored by
            // the plugin already violates it, snap back up to the minimum.
            if let Some(window) = app.get_webview_window("main") {
                let min_size = LogicalSize::new(MAIN_WINDOW_MIN_WIDTH, MAIN_WINDOW_MIN_HEIGHT);
                let _ = window.set_min_size(Some(min_size));

                if let Ok(scale) = window.scale_factor() {
                    if let Ok(current) = window.inner_size() {
                        let current_logical = current.to_logical::<f64>(scale);
                        if current_logical.width < MAIN_WINDOW_MIN_WIDTH
                            || current_logical.height < MAIN_WINDOW_MIN_HEIGHT
                        {
                            let clamped = LogicalSize::new(
                                current_logical.width.max(MAIN_WINDOW_MIN_WIDTH),
                                current_logical.height.max(MAIN_WINDOW_MIN_HEIGHT),
                            );
                            let _ = window.set_size(clamped);
                        }
                    }
                }
            }

            let store = open_session_store().map_err(|e| e.to_string())?;
            let config = load_config().unwrap_or_default();
            let service = if config.is_ready() {
                match build_service(&config, store.clone()) {
                    Ok(s) => Some(s),
                    Err(err) => {
                        tracing::warn!(error = %err, "failed to build engine on launch");
                        None
                    }
                }
            } else {
                None
            };
            app.manage(AppState::new(store, config, service));

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<AppState>();
                commands::respawn_cron_loop(&state).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::hello,
            commands::get_provider_config,
            commands::set_secret_storage,
            commands::list_builtin_providers,
            commands::validate_provider,
            commands::save_provider_config,
            commands::profiles_list,
            commands::profile_upsert,
            commands::profile_remove,
            commands::profile_activate,
            commands::validate_profile,
            commands::copilot_auth_status,
            commands::copilot_auth_start,
            commands::copilot_auth_wait,
            commands::copilot_auth_cancel,
            commands::list_models,
            commands::list_providers,
            commands::create_session,
            commands::list_sessions,
            commands::session_meta,
            commands::resume_session,
            commands::update_session,
            commands::suggest_session_title,
            commands::delete_session,
            commands::replay,
            commands::subscribe_session,
            commands::unsubscribe_session,
            commands::prompt,
            commands::cancel,
            commands::background_list,
            commands::background_kill,
            commands::background_demote,
            commands::respond_permission,
            commands::respond_question,
            commands::is_configured,
            commands::git_is_repo,
            commands::git_branch,
            commands::git_list_branches,
            commands::git_checkout,
            commands::git_status,
            commands::git_status_since_baseline,
            commands::git_diff,
            commands::git_commit,
            commands::git_push,
            // Commit center: selective staging + commit/push/branch/PR flow.
            commands::git_commit_paths,
            commands::git_commit_and_push,
            commands::git_create_branch_and_commit,
            commands::git_create_pr,
            commands::suggest_commit_message,
            // Review flow: per-file keep/undo + hunk-patch apply.
            commands::review_undo_file,
            commands::review_keep_file,
            commands::review_apply_patch,
            commands::review_file_diff,
            commands::list_files,
            commands::list_commands,
            commands::is_isolated,
            commands::workspace_status,
            commands::integrate_session,
            commands::discard_session,
            commands::revert,
            commands::routines_list,
            commands::routines_upsert,
            commands::routines_remove,
            commands::routines_run,
            commands::routines_history,
            commands::mcp_list,
            commands::mcp_upsert,
            commands::mcp_remove,
            commands::mcp_test,
            commands::memory_list,
            commands::memory_get,
            commands::memory_remove,
            commands::memory_set_expiry,
            commands::project_memory_list,
            commands::project_memory_get,
            commands::project_memory_remove,
            commands::project_memory_set_expiry,
            commands::user_identity,
            commands::save_text_file,
            commands::export_diagnostics_bundle,
            commands::write_temp_blob,
            commands::debug_log_path,
            commands::app_version,
            commands::index_status,
            commands::index_rebuild,
            terminal::terminal_create,
            terminal::terminal_write,
            terminal::terminal_resize,
            terminal::terminal_kill,
            terminal::terminal_list,
            browser::browser_open,
            browser::browser_navigate,
            browser::browser_back,
            browser::browser_forward,
            browser::browser_reload,
            browser::browser_set_bounds,
            browser::browser_set_visible,
            browser::browser_close,
            browser::browser_open_devtools,
            browser::browser_hard_reload,
            browser::browser_clear_data,
            browser::browser_screenshot,
            browser::browser_set_design_mode,
        ])
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { .. } => {
                let state = window.state::<AppState>();
                crate::terminal::kill_all_terminals(&state);
                // Reap every background bash process any session started
                // (`run_in_background`/demoted foreground calls) — otherwise
                // a dev server or long-running script left running by an
                // agent outlives the app entirely, since its detached
                // reader/wait tasks have no `Drop` impl that can kill them
                // (see `EngineService::shutdown`'s doc comment). Blocking is
                // deliberate: killing is best-effort and typically
                // near-instant, and doing it before the process actually
                // exits is the only way to guarantee it happens at all.
                tauri::async_runtime::block_on(async {
                    if let Some(service) = state.service.lock().await.as_ref() {
                        service.shutdown().await;
                    }
                });
            }
            // macOS anchors child webviews to the window bottom (non-flipped
            // NSView coords); re-assert the frontend-requested bounds so the
            // browser page never slides over the React toolbar mid-resize.
            tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged { .. } => {
                let state = window.state::<AppState>();
                crate::browser::reapply_browser_bounds(&state);
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
