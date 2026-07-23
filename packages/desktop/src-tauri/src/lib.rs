mod artifacts_plugin;
mod browser;
mod commands;
mod components_plugin;
mod compose;
mod config;
mod db_plugin;
mod debug;
mod error;
mod event_coalesce;
#[cfg(target_os = "macos")]
mod macos_window;
mod path_resolve;
mod plugins;
mod remote;
mod screen_capture;
mod secrets;
mod state;
mod terminal;
mod win_console;

use tauri::{Emitter, LogicalSize, Manager};
use tracing_subscriber::prelude::*;

use crate::compose::{build_service, open_session_store};
use crate::config::load_config;
use crate::state::AppState;

const MAIN_WINDOW_MIN_WIDTH: f64 = 900.0;
const MAIN_WINDOW_MIN_HEIGHT: f64 = 600.0;

fn init_tracing(app: &tauri::App) {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    let debug_mode = debug::is_debug_mode_enabled(&app_data_dir);

    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if debug_mode {
            tracing_subscriber::EnvFilter::new("debug,tauri_plugin_updater=off,tantivy=warn")
        } else {
            tracing_subscriber::EnvFilter::new(
                "info,agentloop_loop=debug,agentloop_engine=debug,\
                 agentloop_providers=debug,agentloop_provider_bedrock=debug,\
                 tauri_plugin_updater=off,tantivy=warn",
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

fn chrono_today_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
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
    win_console::ensure_hidden_parent_console();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        & !tauri_plugin_window_state::StateFlags::DECORATIONS,
                )
                .build(),
        )
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            init_tracing(app);

            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_decorations(false);
                let min_size = LogicalSize::new(MAIN_WINDOW_MIN_WIDTH, MAIN_WINDOW_MIN_HEIGHT);
                let _ = window.set_min_size(Some(min_size));

                #[cfg(target_os = "macos")]
                macos_window::apply_macos_chrome(&window);

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
                match build_service(&config, store.clone(), app.handle().clone()) {
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

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<AppState>();
                let remote = state.remote.lock().await.clone();
                if let Some(server) = remote {
                    let cfg = server.snapshot_config().await;
                    if cfg.enabled {
                        if let Err(err) = server.start(handle.clone()).await {
                            tracing::warn!(error = %err, "failed to start remote access on launch");
                        }
                    }
                }
            });

            if std::env::var("FLEX_BROWSER_QA").is_ok() {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                    let _ = handle.emit("qa-open-browser", "https://www.google.com");
                });
            }

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
            commands::chatgpt_auth_status,
            commands::chatgpt_auth_start,
            commands::chatgpt_auth_wait,
            commands::chatgpt_auth_cancel,
            commands::list_models,
            commands::list_providers,
            commands::create_session,
            commands::list_sessions,
            commands::session_meta,
            commands::resume_session,
            commands::update_session,
            commands::suggest_session_title,
            commands::get_inline_completion_prefs,
            commands::save_inline_completion_prefs,
            commands::complete_prompt_inline,
            commands::check_inline_completion_connection,
            commands::review_prompt,
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
            commands::set_turn_permission_mode,
            commands::respond_question,
            commands::respond_mode_switch,
            commands::is_configured,
            commands::git_is_repo,
            commands::git_has_remote,
            commands::git_branch,
            commands::git_list_branches,
            commands::git_checkout,
            commands::git_status,
            commands::git_status_since_baseline,
            commands::git_diff,
            commands::git_commit,
            commands::git_push,
            commands::git_commit_paths,
            commands::git_commit_and_push,
            commands::git_create_branch_and_commit,
            commands::git_create_pr,
            commands::git_pr_status,
            commands::git_pr_diff,
            commands::git_create_pr_for_branch,
            commands::git_pr_draft,
            commands::suggest_commit_message,
            commands::review_undo_file,
            commands::review_keep_file,
            commands::review_apply_patch,
            commands::review_file_diff,
            commands::list_files,
            commands::list_dir_children,
            commands::invalidate_workspace_path_cache,
            commands::resolve_workspace_cwd,
            commands::list_commands,
            db_plugin::db_list_connections,
            db_plugin::db_upsert_connection,
            db_plugin::db_remove_connection,
            db_plugin::db_connect,
            db_plugin::db_disconnect,
            db_plugin::db_active_connection,
            db_plugin::db_list_schemas,
            db_plugin::db_list_tables,
            db_plugin::db_preview_table,
            db_plugin::db_query,
            db_plugin::db_mention_tables,
            artifacts_plugin::artifacts_list,
            artifacts_plugin::artifacts_register,
            artifacts_plugin::artifacts_remove,
            artifacts_plugin::artifacts_preview_csv,
            artifacts_plugin::artifacts_open_external,
            components_plugin::components_detect,
            components_plugin::components_list,
            components_plugin::components_detail,
            commands::is_isolated,
            commands::workspace_status,
            commands::list_workspaces,
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
            remote::remote_access_get,
            remote::remote_access_save,
            remote::remote_access_rotate_token,
            remote::remote_access_restart,
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
            commands::read_text_file,
            commands::create_text_file,
            commands::rename_path,
            commands::delete_path,
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
            browser::browser_apply_style_overrides,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                let state = window.state::<AppState>();
                crate::terminal::kill_all_terminals(&state);
                tauri::async_runtime::block_on(async {
                    if let Some(remote) = state.remote.lock().await.as_ref() {
                        let _ = remote.stop().await;
                    }
                    if let Some(service) = state.service.lock().await.as_ref() {
                        service.shutdown().await;
                    }
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
