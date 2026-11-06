//! The Tauri application: start-up in the order of `app.py` (1.x), then the window.
//!
//! 1. Move folders of earlier builds.
//! 2. Load settings.
//! 3. Point desktop entries at this copy.
//! 4. Open the list and the manager.
//! 5. Take the instance socket.
//! 6. Show the window unless starting minimized into the tray.
//! 7. Hand the command-line URLs to the window.

use std::process::ExitCode;
use std::sync::Mutex;

use lider_core::manager::DownloadManager;
use lider_core::paths::{Dirs, migrate_legacy_dirs};
use lider_core::store::Store;
use lider_sys::env::SysEnv;
use lider_sys::ipc::InstanceServer;
use tauri::{Emitter, Manager, WindowEvent};

use crate::app::{self, AppState};
use crate::cli::Cli;

pub fn run(cli: Cli, socket: String) -> ExitCode {
    let dirs = Dirs::from_env();
    migrate_legacy_dirs(&dirs);
    let mut settings = lider_core::settings::load(&dirs);
    let sys = SysEnv::from_process();
    let _ = lider_sys::desktop::refresh(&sys);
    let engine = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("lider-engine")
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("lider: cannot start the download engine: {e}");
            return ExitCode::FAILURE;
        }
    };
    let store = match Store::open(&dirs.database_file()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("lider: {e}");
            return ExitCode::FAILURE;
        }
    };
    let offer_menu = lider_sys::desktop::should_offer(&sys, settings.menu_offered);
    let start_hidden = (cli.minimized || settings.start_minimized) && settings.tray_icon;
    if settings.speed_limiter_on_startup {
        settings.speed_limiter_on = true;
    }
    let ui = app::new_ui(&settings);
    let manager = match DownloadManager::new(settings, store, engine.handle().clone()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("lider: {e}");
            return ExitCode::FAILURE;
        }
    };

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            crate::commands::app_info,
            crate::commands::ready,
            crate::commands::view,
            crate::commands::menu_action,
            crate::commands::ui_filter,
            crate::commands::ui_select,
            crate::commands::ui_check,
            crate::commands::ui_toggle_all,
            crate::commands::ui_clear_checks,
            crate::commands::ui_header_click,
            crate::commands::ui_column_width,
            crate::commands::ui_pane_sizes,
            crate::commands::ui_search,
            crate::commands::ui_search_close,
            crate::commands::ui_search_flags,
            crate::commands::ui_find_next,
            crate::commands::targets_info,
            crate::commands::act_remove,
            crate::commands::act_delete_completed,
            crate::commands::act_activate,
            crate::commands::act_details_button,
            crate::commands::act_open_window,
            crate::commands::act_download_limit,
            crate::commands::act_open,
            crate::commands::act_open_folder,
            crate::commands::act_open_with,
            crate::commands::act_set_queue,
            crate::commands::act_queue,
            crate::commands::act_add_many,
            crate::commands::act_start_main_queue,
            crate::commands::copy_text,
            crate::commands::clipboard_text,
            crate::commands::export_list,
            crate::commands::import_list,
            crate::commands::set_list_font,
            crate::commands::system_fonts,
            crate::commands::close_choice,
            crate::commands::shutdown_now,
            crate::commands::menu_offer_answer,
            crate::commands::recheck_browser,
            crate::commands::save_settings,
            crate::commands::browser_request,
            crate::commands::set_completion_options,
            crate::commands::open_dialog,
            crate::commands::download_url,
            crate::commands::delete_category,
            crate::commands::e2e_script,
            crate::commands::e2e_report,
            crate::dialogs::dialog_theme,
            crate::dialogs::settings_get,
            crate::dialogs::settings_patch,
            crate::dialogs::checksum_parse,
            crate::dialogs::limiter_apply,
            crate::dialogs::columns_apply,
            crate::dialogs::category_save,
            crate::dialogs::url_history,
            crate::dialogs::remember_url,
            crate::dialogs::add_check,
            crate::dialogs::add_duplicate_existing,
            crate::dialogs::probe_url,
            crate::dialogs::probe_media,
            crate::dialogs::add_prepare,
            crate::dialogs::add_download,
            crate::dialogs::batch_preview,
            crate::dialogs::grab_page,
            crate::dialogs::links_rows,
            crate::dialogs::add_choices,
            crate::dialogs::progress_info,
            crate::dialogs::progress_limit,
            crate::dialogs::progress_button,
            crate::dialogs::complete_info,
            crate::dialogs::complete_dont_show,
            crate::dialogs::mismatch_choice,
            crate::dialogs::checkhash_info,
            crate::dialogs::checkhash_start,
            crate::dialogs::checkhash_poll,
            crate::dialogs::checkhash_stop,
            crate::dialogs::checkhash_save,
            crate::dialogs::checkhash_later,
            crate::dialogs::properties_info,
            crate::dialogs::properties_verify,
            crate::dialogs::properties_apply,
            crate::dialogs::desktop_state,
            crate::dialogs::options_apply,
            crate::dialogs::scheduler_data,
            crate::dialogs::scheduler_apply,
            crate::dialogs::next_run,
            crate::dialogs::queue_new,
            crate::dialogs::queue_delete,
            crate::dialogs::queue_move,
            crate::dialogs::browsers_list,
            crate::dialogs::browsers_apply,
            crate::dialogs::open_url,
            crate::dialogs::download_get,
            crate::dialogs::can_check_hash,
            crate::dialogs::name_choice,
            crate::dialogs::media_default,
            crate::dialogs::media_variant,
            crate::dialogs::home_dir,
            crate::dialogs::appimage_name,
        ])
        .on_menu_event(|app, e| crate::commands::run_menu(app, e.id().as_ref()))
        .on_window_event(|window, e| {
            if let WindowEvent::Destroyed = e {
                crate::dialogs::window_closed(window.app_handle(), window.label());
            }
            if window.label() == "main"
                && let WindowEvent::CloseRequested { api, .. } = e
            {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                if !app::close_requested(app, state.inner()) {
                    api.prevent_close();
                }
            }
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            let server = InstanceServer::bind(&socket, move |line| {
                let state = handle.state::<AppState>();
                if *app::lock(&state.ready) {
                    app::on_line(&handle, &line);
                } else {
                    app::lock(&state.pending).push(line);
                }
            });
            let server = match server {
                Ok(s) => Some(s),
                // Another LID started at the same moment and owns the socket.
                Err(_) if lider_sys::ipc::send_lines(&cli.urls, &socket) => std::process::exit(0),
                Err(_) => None,
            };
            let window_state = manager.settings.window_state;
            app.manage(AppState {
                manager: Mutex::new(manager),
                ui: Mutex::new(ui),
                pending: Mutex::new(cli.urls.clone()),
                ready: Mutex::new(false),
                offer_menu: Mutex::new(offer_menu),
                start_hidden,
                dirs,
                sys,
                _engine: engine,
                _server: Mutex::new(server),
            });
            let state = app.state::<AppState>();
            {
                let dark = crate::system::prefers_dark().unwrap_or(false);
                let mut ui = state.ui();
                ui.dark_system = dark;
                ui.global_menu = crate::system::global_menu_available();
            }
            crate::tray::ensure(app.handle(), state.inner());
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_title(&format!(
                    "{} {}",
                    lider_core::APP_LONG_NAME,
                    lider_core::VERSION
                ));
                if let Some(ws) = window_state {
                    let _ = w.set_size(tauri::LogicalSize::new(ws.width as f64, ws.height as f64));
                    let _ = w.set_position(tauri::LogicalPosition::new(ws.x as f64, ws.y as f64));
                    if ws.maximized {
                        let _ = w.maximize();
                    }
                }
                let hidden = start_hidden && crate::tray::exists(app.handle());
                if hidden {
                    let _ = w.hide();
                } else {
                    let _ = w.show();
                }
            }
            crate::pump::apply_menubar(app.handle(), state.inner());
            crate::pump::start(app.handle().clone());
            let _ = app.emit("lid://started", ());
            Ok(())
        })
        .run(tauri::generate_context!());
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lider: {e}");
            ExitCode::FAILURE
        }
    }
}
