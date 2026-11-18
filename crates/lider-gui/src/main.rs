//! LID's window in egui (docs/egui/PLAN.md). Like the Tauri binary, the process mode comes from argv
//! before any window exists: the native messaging host and the browser CLI never start the GUI.
//!
//! GUI start-up in the order of `app.py` (1.x): move folders of earlier builds, load the settings, point
//! the desktop entries at this copy, open the list and the manager, take the instance socket, show the
//! window unless it starts in the tray, hand the command-line URLs to it.

// No console window on Windows (release builds); the browser still gets stdin/stdout for the host.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod boxes;
mod dialogs;
mod fonts;
mod icons;
mod main_window;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_add;
#[cfg(test)]
mod tests_dialogs;
#[cfg(test)]
mod tests_progress;
#[cfg(test)]
mod tests_settings;
#[cfg(test)]
mod tests_update;
mod theme;
mod tray;

use std::process::ExitCode;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, OnceLock};

use lider_core::cli;
use lider_core::manager::DownloadManager;
use lider_core::paths::{Dirs, migrate_legacy_dirs};
use lider_core::settings::WindowState;
use lider_core::store::Store;
use lider_sys::platform::prelude::*;

use app::Effect;
use tray::TrayEvent;

struct Window {
    app: app::App,
    tray: Option<tray::Tray>,
    tray_tx: std::sync::mpsc::Sender<TrayEvent>,
    tray_rx: Receiver<TrayEvent>,
    ctx: egui::Context,
    clipboard: Option<arboard::Clipboard>,
    /// Kept for the lifetime of the window: other starts and the browser reach LID through it.
    _instance: Option<lider_sys::platform::InstanceGuard>,
    ticks: u64,
    /// Started hidden in the tray: shown on the first frame when no tray icon came up.
    start_hidden: bool,
    first_frame: bool,
    /// The list's font family that the fonts were last built with.
    list_family: String,
}

fn wake(ctx: &egui::Context) -> tray::Wake {
    let ctx = ctx.clone();
    Arc::new(move || ctx.request_repaint())
}

impl Window {
    /// The tray icon follows `settings.tray_icon`; its limiter checks and tip follow the manager.
    fn sync_tray(&mut self) {
        let s = &self.app.manager.settings;
        if s.tray_icon && self.tray.is_none() {
            self.tray = tray::start(self.tray_tx.clone(), wake(&self.ctx), s.speed_limiter_on);
        } else if !s.tray_icon && self.tray.is_some() {
            self.tray = None;
        }
        if let Some(t) = &self.tray {
            t.set_limiter(s.speed_limiter_on);
            t.set_tooltip(&lider_core::view::tray_tooltip(&self.app.manager));
        }
        self.app.tray_shown = self.tray.is_some();
    }

    fn show(&self) {
        let ctx = &self.ctx;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn effects(&mut self) {
        for e in std::mem::take(&mut self.app.effects) {
            match e {
                Effect::Notify(title, text) => notify(title, text),
                Effect::Show => self.show(),
                // Without a tray icon (a desktop without one) hiding would lose the window: it
                // minimises instead (1.x `_to_background`).
                Effect::Hide if self.tray.is_none() => self
                    .ctx
                    .send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
                Effect::Hide => self
                    .ctx
                    .send_viewport_cmd(egui::ViewportCommand::Visible(false)),
                Effect::Close => self.ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Effect::PowerOff => {
                    let _ = os().power_off();
                    self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    /// Where the window is and how big, while it is shown (saved with the settings).
    fn track_geometry(&mut self, ctx: &egui::Context) {
        let g = ctx.input(|i| {
            let v = i.viewport();
            if v.minimized == Some(true) {
                return None;
            }
            let outer = v.outer_rect?;
            let inner = v.inner_rect?;
            Some(WindowState {
                x: outer.min.x as i32,
                y: outer.min.y as i32,
                width: inner.width() as u32,
                height: inner.height() as u32,
                maximized: v.maximized.unwrap_or(false),
            })
        });
        if let Some(mut g) = g {
            // Maximised keeps the size and place to return to.
            if g.maximized
                && let Some(old) = self.app.geometry
            {
                g = WindowState {
                    maximized: true,
                    ..old
                };
            }
            self.app.geometry = Some(g);
        }
    }
}

/// A desktop notification, sent off the window's thread (D-Bus may be slow).
fn notify(title: String, text: String) {
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new()
            .appname("LID")
            .summary(&title)
            .body(&text)
            .icon("lider")
            .show();
    });
}

impl eframe::App for Window {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if std::mem::take(&mut self.first_frame) {
            self.sync_tray();
            if self.start_hidden && self.tray.is_none() {
                self.show();
            }
        }
        while let Ok(e) = self.tray_rx.try_recv() {
            match e {
                TrayEvent::Open => self.show(),
                TrayEvent::Run(id) => {
                    if id == "add" {
                        self.show();
                    }
                    self.app.run(&id);
                }
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.app.close_requested() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if self.app.tick() {
            self.ticks += 1;
            if self.ticks % 4 == 1 {
                self.app.dark_system = os().prefers_dark().unwrap_or(false);
                self.app.apply_theme();
            }
            if let Some(c) = &mut self.clipboard
                && let Ok(text) = c.get_text()
            {
                self.app.clipboard_text(&text);
            }
            self.sync_tray();
        }
        self.track_geometry(ctx);
        // View → Font: the list's family changed (fonts are rebuilt only then).
        let family = self
            .app
            .manager
            .settings
            .list_font
            .split(',')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if family != self.list_family {
            fonts::install_with_list(ctx, &family);
            self.list_family = family;
        }
        if std::mem::take(&mut self.app.restyle) {
            theme::apply(ctx, self.app.theme);
        }
        self.effects();
        // A dialog opened while the window is hidden (in the tray): the hidden window needs a full pass
        // to make the dialog's own window.
        if self.app.dialogs.iter().any(|d| !d.shown) {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
            ctx.request_repaint();
        }
        ctx.request_repaint_after(app::TICK);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        main_window::show(&mut self.app, ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Closed by the desktop (log-out) without the close request: still save and stop.
        if !self.app.quit {
            self.app.stop_and_exit();
        }
        self.app.close_all_dialogs();
        self.tray = None;
    }
}

/// The window's icon: the app icon (1.x `app`).
fn window_icon() -> egui::IconData {
    image::load_from_memory(include_bytes!("../../../src-tauri/icons/64x64.png"))
        .map(|i| {
            let rgba = i.to_rgba8();
            egui::IconData {
                width: rgba.width(),
                height: rgba.height(),
                rgba: rgba.into_raw(),
            }
        })
        .unwrap_or_default()
}

fn code(n: i32) -> ExitCode {
    ExitCode::from(n.clamp(0, 255) as u8)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if lider_sys::native_host::started_by_browser(&args) {
        return code(lider_sys::native_host::main());
    }
    if args
        .iter()
        .any(|a| a == "--install-browser-integration" || a == "--uninstall-browser-integration")
    {
        return code(lider_sys::browser::main(&args, os()));
    }
    let cli = cli::parse(&args);
    if cli.help {
        print!("{}", cli::USAGE);
        return ExitCode::SUCCESS;
    }
    let sys = os();
    let socket = sys.instance_name();
    // A running LID takes the URLs (or comes forward); this process then ends.
    match cli::lines_for_running(&cli) {
        None if sys.is_running(&socket) => return ExitCode::SUCCESS,
        Some(lines) if sys.send_lines(&lines, &socket) => return ExitCode::SUCCESS,
        _ => {}
    }
    gui(cli, socket)
}

fn gui(cli: cli::Cli, socket: String) -> ExitCode {
    let sys = os();
    let dirs = Dirs::from_env();
    migrate_legacy_dirs(&dirs);
    let mut settings = lider_core::settings::load(&dirs);
    let _ = sys.refresh();
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
    let offer_menu = sys.should_offer(settings.menu_offered);
    let start_hidden = (cli.minimized || settings.start_minimized) && settings.tray_icon;
    if settings.speed_limiter_on_startup {
        settings.speed_limiter_on = true;
    }
    let window_state = settings.window_state;
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
    let mut app = app::App::new(rt, manager, dirs);
    app.geometry = window_state;
    // Lines from other starts and the browser wait in the inbox until the next tick; the window wakes.
    let ctx_cell: Arc<OnceLock<egui::Context>> = Arc::default();
    let inbox = app.inbox.clone();
    let cell = ctx_cell.clone();
    let instance = match sys.bind(
        &socket,
        Box::new(move |line| {
            inbox.lock().unwrap_or_else(|e| e.into_inner()).push(line);
            if let Some(ctx) = cell.get() {
                ctx.request_repaint();
            }
        }),
    ) {
        Ok(g) => Some(g),
        // Another LID started at the same moment and owns the socket.
        Err(_) if sys.send_lines(&cli.urls, &socket) => return ExitCode::SUCCESS,
        Err(_) => None,
    };
    {
        let mut inbox = app.inbox.lock().unwrap_or_else(|e| e.into_inner());
        inbox.extend(cli.urls.iter().cloned());
    }
    // Debug builds: LIDER_RUN="action;action" runs menu actions at start (screenshots of dialogs).
    #[cfg(debug_assertions)]
    if let Ok(actions) = std::env::var("LIDER_RUN") {
        app.tick();
        for a in actions.split(';').filter(|a| !a.is_empty()) {
            // "select:NAME" selects a download by its file name (for its context-menu windows).
            if let Some(name) = a.strip_prefix("select:") {
                let id = app.manager.downloads.values().find(|d| d.filename == name);
                app.selected = id.map(|d| d.id).into_iter().collect();
                continue;
            }
            app.run(a);
        }
    }
    if offer_menu && !start_hidden {
        let name = sys.self_contained_name().unwrap_or_else(|| "LID".into());
        app.offer_menu(&name);
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(format!(
            "{} {}",
            lider_core::APP_LONG_NAME,
            lider_core::VERSION
        ))
        .with_app_id("lider")
        .with_inner_size([1000.0, 560.0])
        .with_min_inner_size([640.0, 360.0])
        .with_icon(window_icon())
        .with_visible(!start_hidden);
    if let Some(ws) = window_state {
        viewport = viewport
            .with_inner_size([ws.width as f32, ws.height as f32])
            .with_position([ws.x as f32, ws.y as f32])
            .with_maximized(ws.maximized);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    // After an update: the new program starts once this one has closed (and let go of the socket).
    let restart = app.restart.clone();
    app.check_for_update(true);
    let run = eframe::run_native(
        "lider",
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            let _ = ctx_cell.set(ctx.clone());
            egui_extras::install_image_loaders(&ctx);
            fonts::install(&ctx);
            app.dark_system = os().prefers_dark().unwrap_or(false);
            app.apply_theme();
            app.restyle = false;
            theme::apply(&ctx, app.theme);
            let (tray_tx, tray_rx) = channel();
            Ok(Box::new(Window {
                app,
                tray: None,
                tray_tx,
                tray_rx,
                ctx,
                clipboard: arboard::Clipboard::new().ok(),
                _instance: instance,
                ticks: 0,
                start_hidden,
                first_frame: true,
                list_family: String::new(),
            }))
        }),
    );
    if let Some(p) = restart.lock().unwrap_or_else(|e| e.into_inner()).take()
        && let Err(e) = std::process::Command::new(&p).spawn()
    {
        eprintln!("lider: cannot start {}: {e}", p.display());
    }
    match run {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lider: {e}");
            ExitCode::FAILURE
        }
    }
}
