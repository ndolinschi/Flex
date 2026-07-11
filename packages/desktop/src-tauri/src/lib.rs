mod browser;
mod commands;
mod compose;
mod config;
mod error;
mod secrets;
mod state;
mod terminal;

use tauri::{LogicalSize, Manager};

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

/// Trace/log to stdout so engine and provider errors are visible in the
/// `tauri dev` console. `RUST_LOG` wins if set (standard `EnvFilter` syntax,
/// e.g. `RUST_LOG=debug`); otherwise falls back to a useful default that
/// surfaces provider-level detail (Bedrock request/stream failures) without
/// drowning in framework noise. Init once, before building the engine
/// service, so failures during `build_service` on launch are also captured.
fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new(
            "info,agentloop_loop=debug,agentloop_engine=debug,\
             agentloop_providers=debug,agentloop_provider_bedrock=debug",
        )
    });
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stdout)
        .try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_os::init())
        .setup(|app| {
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
            commands::list_models,
            commands::list_providers,
            commands::create_session,
            commands::list_sessions,
            commands::session_meta,
            commands::resume_session,
            commands::update_session,
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
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                let state = window.state::<AppState>();
                crate::terminal::kill_all_terminals(&state);
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
