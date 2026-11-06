//! The Tauri application. Everything that needs a webview lives behind `run()`.

use std::process::ExitCode;

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

pub fn run() -> ExitCode {
    let result = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![app_info])
        .run(tauri::generate_context!());
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lider: {e}");
            ExitCode::FAILURE
        }
    }
}
