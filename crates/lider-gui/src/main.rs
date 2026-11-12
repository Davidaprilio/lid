//! LID's window in egui (docs/egui/PLAN.md). Like the Tauri binary, the process mode comes from argv
//! before any window exists: the native messaging host and the browser CLI never start the GUI.

mod app;
mod fonts;
mod icons;
mod main_window;
mod theme;

use std::process::ExitCode;

use lider_core::manager::DownloadManager;
use lider_core::paths::{Dirs, migrate_legacy_dirs};
use lider_core::store::Store;
use lider_sys::platform::prelude::*;

struct Window {
    app: app::App,
}

impl eframe::App for Window {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.tick();
        ctx.request_repaint_after(app::TICK);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        main_window::show(&mut self.app, ui);
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if lider_sys::native_host::started_by_browser(&args) {
        return ExitCode::from(lider_sys::native_host::main().clamp(0, 255) as u8);
    }
    if args
        .iter()
        .any(|a| a == "--install-browser-integration" || a == "--uninstall-browser-integration")
    {
        return ExitCode::from(lider_sys::browser::main(&args, os()).clamp(0, 255) as u8);
    }
    let dirs = Dirs::from_env();
    migrate_legacy_dirs(&dirs);
    let settings = lider_core::settings::load(&dirs);
    let rt = match tokio::runtime::Builder::new_multi_thread()
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
    let manager = match Store::open(&dirs.database_file())
        .map_err(|e| e.to_string())
        .and_then(|s| {
            DownloadManager::new(settings, s, rt.handle().clone()).map_err(|e| e.to_string())
        }) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("lider: {e}");
            return ExitCode::FAILURE;
        }
    };
    let app = app::App::new(rt, manager, dirs);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!(
                "{} {}",
                lider_core::APP_LONG_NAME,
                lider_core::VERSION
            ))
            .with_app_id("lider")
            .with_inner_size([1000.0, 560.0]),
        ..Default::default()
    };
    let run = eframe::run_native(
        "lider",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            fonts::install(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx, app.theme);
            Ok(Box::new(Window { app }))
        }),
    );
    match run {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lider: {e}");
            ExitCode::FAILURE
        }
    }
}
