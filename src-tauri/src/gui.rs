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
use tauri::{Emitter, Manager};

use crate::cli::Cli;

/// State shared by the commands.
pub struct AppState {
    pub manager: Mutex<DownloadManager>,
    /// Lines from other starts and the browser, kept until the window takes them.
    pub pending: Mutex<Vec<String>>,
    /// The first-start "add to the application menu?" question is due.
    pub offer_menu: bool,
    pub start_hidden: bool,
    _engine: tokio::runtime::Runtime,
    _server: Option<InstanceServer>,
}

#[derive(serde::Serialize)]
struct AppInfo {
    name: &'static str,
    long_name: &'static str,
    version: &'static str,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: lider_core::APP_NAME,
        long_name: lider_core::APP_LONG_NAME,
        version: lider_core::VERSION,
    }
}

#[derive(serde::Serialize)]
struct StartupInfo {
    offer_menu: bool,
    start_hidden: bool,
    downloads: usize,
}

/// What the window needs to know when it opens.
#[tauri::command]
fn startup_info(state: tauri::State<'_, AppState>) -> StartupInfo {
    StartupInfo {
        offer_menu: state.offer_menu,
        start_hidden: state.start_hidden,
        downloads: state
            .manager
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .downloads
            .len(),
    }
}

/// URLs, `--show` and browser requests received so far (the window drains them once it is ready, and
/// listens for `lid://instance-line` afterwards).
#[tauri::command]
fn take_pending_lines(state: tauri::State<'_, AppState>) -> Vec<String> {
    std::mem::take(&mut *state.pending.lock().unwrap_or_else(|e| e.into_inner()))
}

pub fn run(cli: Cli, socket: String) -> ExitCode {
    let dirs = Dirs::from_env();
    migrate_legacy_dirs(&dirs);
    let settings = lider_core::settings::load(&dirs);
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
    let manager = match DownloadManager::new(settings, store, engine.handle().clone()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("lider: {e}");
            return ExitCode::FAILURE;
        }
    };

    let result = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            app_info,
            startup_info,
            take_pending_lines
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let server = InstanceServer::bind(&socket, move |line| {
                if let Some(state) = handle.try_state::<AppState>() {
                    state
                        .pending
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push(line.clone());
                }
                let _ = handle.emit("lid://instance-line", line);
            });
            let server = match server {
                Ok(s) => Some(s),
                // Another LID started at the same moment and owns the socket.
                Err(_) if lider_sys::ipc::send_lines(&cli.urls, &socket) => std::process::exit(0),
                Err(_) => None,
            };
            app.manage(AppState {
                manager: Mutex::new(manager),
                pending: Mutex::new(cli.urls.clone()),
                offer_menu,
                start_hidden,
                _engine: engine,
                _server: server,
            });
            if let Some(w) = app.get_webview_window("main") {
                if start_hidden {
                    let _ = w.hide();
                } else {
                    let _ = w.show();
                }
            }
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
