mod browser;
mod commands;
mod compose;
mod config;
mod error;
mod state;
mod terminal;

use tauri::Manager;

use crate::compose::{build_service, open_session_store};
use crate::config::load_config;
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
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
            commands::list_builtin_providers,
            commands::validate_provider,
            commands::save_provider_config,
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
            commands::respond_permission,
            commands::respond_question,
            commands::is_configured,
            commands::git_branch,
            commands::git_list_branches,
            commands::git_checkout,
            commands::git_status,
            commands::git_diff,
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
