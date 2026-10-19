mod commands;
mod jobs;
mod macos_printing;
mod models;
mod parser;
mod printer_setup;
mod printing;
mod storage;
mod test_sheet;
mod tray;
mod watcher;

use jobs::JobQueueState;
use std::sync::Arc;
use storage::StorageState;
use tauri::window::Color;
use tauri::Manager;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};
use watcher::WatcherState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Logs go to the app log dir (on Windows
        // %LOCALAPPDATA%\studio.hellomagnets.hello-print\logs) so testers can
        // send them; Settings has a button that opens the folder.
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                ])
                .level(log::LevelFilter::Info)
                .timezone_strategy(TimezoneStrategy::UseLocal)
                .max_file_size(2_000_000)
                .rotation_strategy(RotationStrategy::KeepSome(5))
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            log::info!(
                "Hello Print {} starting on {} {}",
                app.package_info().version,
                std::env::consts::OS,
                std::env::consts::ARCH
            );

            let data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");

            let storage = Arc::new(StorageState::new(data_dir));
            let watcher_state = Arc::new(WatcherState::new());
            let job_queue = Arc::new(JobQueueState::new());

            // Set up system tray
            let _ = tray::setup_tray(app.handle());

            // Handle close-to-tray
            let storage_for_close = storage.clone();
            let main_window = app.get_webview_window("main").unwrap();
            let window_for_close = main_window.clone();

            // Paint the window and webview in the theme's background colour
            // (--background in App.css) until the page loads, instead of the
            // webview's default white.
            let background = match main_window.theme() {
                Ok(tauri::Theme::Dark) => Color(10, 10, 10, 255),
                _ => Color(255, 255, 255, 255),
            };
            let _ = main_window.set_background_color(Some(background));
            main_window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    let minimize = storage_for_close
                        .config
                        .lock()
                        .map(|c| c.minimize_to_tray)
                        .unwrap_or(false);
                    if minimize {
                        api.prevent_close();
                        let _ = window_for_close.hide();
                    }
                }
            });

            // Auto-start watcher if a watch folder is configured
            let watch_folder = {
                let config = storage.config.lock().unwrap();
                config.watch_folder.clone()
            };

            if let Some(folder) = watch_folder {
                let app_handle = app.handle().clone();
                let ws = watcher_state.clone();
                let ss = storage.clone();
                let jq = job_queue.clone();
                std::thread::spawn(move || {
                    if let Err(e) = watcher::start_watcher(app_handle, folder, ws, ss, jq) {
                        log::error!("Failed to auto-start watcher: {}", e);
                    }
                });
            }

            app.manage(storage);
            app.manage(watcher_state);
            app.manage(job_queue);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::list_presets,
            commands::create_preset,
            commands::update_preset,
            commands::delete_preset,
            commands::list_printers,
            commands::get_printer_capabilities,
            commands::start_watcher,
            commands::stop_watcher,
            commands::get_watcher_status,
            commands::list_jobs,
            commands::cancel_job,
            commands::retry_job,
            commands::reprint_job,
            commands::get_platform,
            commands::open_log_folder,
            commands::list_paper_sizes,
            commands::save_test_sheet,
            commands::open_printer_dialog,
            commands::check_expansion,
            commands::print_test_sheet,
            commands::configure_macos_printer,
            commands::get_borderless_scale_factor,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
