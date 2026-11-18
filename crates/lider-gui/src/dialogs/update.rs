//! Updating LID (lider_core::update) and getting yt-dlp when YouTube needs it (lider_core::tools): the
//! offer of a newer version, the "Updating LID" window, and "Download yt-dlp?".

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use egui::{RichText, Ui};
use lider_core::tools::{self, Progress};
use lider_core::update::{self, Found};

use super::kit::{self, Badge, Head};
use super::{Body, Cx, Dialog};
use crate::any_body;
use crate::app::{App, Effect, Request};
use crate::boxes::{Answer, Button, MsgBox, Then};

/// A result that a background task puts here and the window takes.
type Slot<T> = Arc<Mutex<Option<T>>>;

fn take<T>(slot: &Slot<T>) -> Option<T> {
    slot.lock().unwrap_or_else(|e| e.into_inner()).take()
}

/// A running "is there a newer LID?".
pub struct Check {
    slot: Slot<Result<Option<Found>, String>>,
    /// At start: only a newer, not skipped version shows; nothing is said otherwise.
    quiet: bool,
}

impl App {
    /// Look for a newer version in the background (`quiet` at start). A build in a source checkout never
    /// looks at start unless LIDER_UPDATE_CHECK=1.
    /// LIDER_UPDATE_API points at another releases list (a test release).
    pub fn check_for_update(&mut self, quiet: bool) {
        let prefs = update::load(&self.dirs);
        if quiet {
            let forced = std::env::var("LIDER_UPDATE_CHECK").is_ok_and(|v| v == "1");
            if !prefs.check_at_startup || (update::is_dev_build() && !forced) {
                return;
            }
        }
        let api = std::env::var("LIDER_UPDATE_API").unwrap_or_else(|_| update::RELEASES_API.into());
        self.check_for_update_at(&api, quiet);
    }

    pub fn check_for_update_at(&mut self, api: &str, quiet: bool) {
        let prefs = update::load(&self.dirs);
        let client = match self.manager.client() {
            Ok(c) => c,
            Err(e) => {
                if !quiet {
                    self.info(
                        "LID",
                        &format!("Cannot check for a new version:\n{}", e.message()),
                    );
                }
                return;
            }
        };
        let slot: Slot<_> = Default::default();
        let s = slot.clone();
        let api = api.to_string();
        let can_install = update::self_updatable().is_some();
        self.rt.spawn(async move {
            if quiet {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            let r = update::find(&client, &api, &prefs.channel, can_install).await;
            *s.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
        });
        self.update_check = Some(Check { slot, quiet });
    }

    /// The check ended (called each tick): offer what it found.
    pub fn update_checked(&mut self) {
        // Another question is open: the offer waits for it.
        if self.boxx.is_some() {
            return;
        }
        let Some(c) = &self.update_check else { return };
        let Some(r) = take(&c.slot) else { return };
        let quiet = c.quiet;
        self.update_check = None;
        if let (true, Err(e)) = (quiet, &r) {
            eprintln!("lider: checking for a new version: {e}");
        }
        self.update_note = match &r {
            Ok(Some(f)) => format!("LID {} is available.", f.version),
            Ok(None) => "LID is up to date.".into(),
            Err(e) => format!("Cannot check: {e}"),
        };
        match r {
            Ok(Some(f)) if !quiet || f.version != update::load(&self.dirs).skipped => {
                self.offer_update(f)
            }
            Ok(_) | Err(_) => {}
        }
    }

    /// "LID x is available": Skip this version, Later, Update now (or the release page).
    pub fn offer_update(&mut self, f: Found) {
        self.effects.push(Effect::Show);
        let notes = f.notes.trim();
        let notes = if notes.is_empty() {
            String::new()
        } else {
            let short: String = notes.chars().take(600).collect();
            let more = if notes.chars().count() > 600 {
                "…"
            } else {
                ""
            };
            format!("\n\n{short}{more}")
        };
        let text = format!("You have LID {}.{notes}", lider_core::VERSION);
        let main = if f.can_install {
            Button {
                id: "install",
                label: "Update now",
                primary: true,
                ..Default::default()
            }
        } else {
            Button {
                id: "page",
                label: "Open release page",
                primary: true,
                ..Default::default()
            }
        };
        let note = if f.can_install {
            "LID downloads it, checks its signature, installs it and starts again."
        } else {
            "Update LID with the package manager it came from (apt, pacman or your AUR helper), or get it from the release page."
        };
        let title = format!("LID {} is available", f.version);
        self.boxx = Some(
            MsgBox::question(&title, &text, Then::Update(Box::new(f)))
                .with_icon("down-arrow", false)
                .with_note(note)
                .with_buttons(vec![
                    Button {
                        id: "skip",
                        label: "Skip this version",
                        left: true,
                        ..Default::default()
                    },
                    Button {
                        id: "later",
                        label: "Later",
                        ..Default::default()
                    },
                    main,
                ]),
        );
    }

    /// The answer to the offer.
    pub fn update_answer(&mut self, button: &str, f: Found) {
        match button {
            "skip" => {
                let mut p = update::load(&self.dirs);
                p.skipped = f.version;
                if let Err(e) = update::save(&self.dirs, &p) {
                    self.info("LID", &e.to_string());
                }
            }
            "page" => {
                use lider_sys::platform::prelude::*;
                let _ = os().open_path(&f.page);
            }
            "install" => {
                if let Some(url) = f.latest_json.clone() {
                    install(self, &f.version, &url);
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------- Updating LID

pub struct Updating {
    pub version: String,
    progress: Arc<Progress>,
    slot: Slot<Result<(String, Vec<u8>), String>>,
}

/// Download the update, check it, then install it and start again. LIDER_UPDATE_KEY replaces LID's
/// key (a test release signed with another key).
pub fn install(app: &mut App, version: &str, latest_json: &str) {
    let key = std::env::var("LIDER_UPDATE_KEY").unwrap_or_else(|_| update::PUBLIC_KEY.into());
    install_with_key(app, version, latest_json, &key);
}

pub fn install_with_key(app: &mut App, version: &str, latest_json: &str, key: &str) {
    let client = match app.manager.client() {
        Ok(c) => c,
        Err(e) => return app.info("LID was not updated", &e.message()),
    };
    let progress: Arc<Progress> = Arc::new(Progress {
        total: (-1).into(),
        ..Default::default()
    });
    let slot: Slot<_> = Default::default();
    let (p, s, url) = (progress.clone(), slot.clone(), latest_json.to_string());
    let key = key.to_string();
    app.rt.spawn(async move {
        let r = update::download(&client, &url, &key, &p).await;
        *s.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
    });
    app.open_dialog(Dialog::new(
        "update",
        "Updating LID",
        (520.0, 200.0),
        Updating {
            version: version.into(),
            progress,
            slot,
        },
    ));
}

/// Put the checked file in place; Ok when LID should now close (and start again).
fn put_in_place(app: &mut App, version: &str, data: &[u8]) -> Result<(), String> {
    if cfg!(windows) {
        // The installer replaces LID once it has closed, then starts it again.
        return update::run_installer(data, version);
    }
    let target = update::self_updatable().ok_or("This copy of LID is not an AppImage.")?;
    update::install_appimage(&target, data)?;
    *app.restart.lock().unwrap_or_else(|e| e.into_inner()) = Some(target);
    Ok(())
}

impl Body for Updating {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        if let Some(r) = take(&self.slot) {
            match r.and_then(|(v, data)| {
                let v = if v.is_empty() {
                    self.version.clone()
                } else {
                    v
                };
                put_in_place(app, &v, &data)
            }) {
                Ok(()) => {
                    cx.close = true;
                    app.stop_and_exit();
                    return;
                }
                Err(e) if e == "cancelled" => {
                    cx.close = true;
                    return;
                }
                Err(e) => {
                    cx.close = true;
                    app.boxx =
                        Some(MsgBox::info("LID was not updated", &e).with_icon("info", true));
                    return;
                }
            }
        }
        let done = self.progress.done.load(Ordering::Relaxed);
        let total = self.progress.total.load(Ordering::Relaxed);
        kit::body(ui, |ui| {
            let text = if total > 0 {
                format!(
                    "Downloading LID {}… {}%",
                    self.version,
                    (done as f64 / total as f64 * 100.0).round()
                )
            } else {
                format!("Downloading LID {}…", self.version)
            };
            kit::header(
                ui,
                &p,
                &Head::new("down-arrow", "Updating LID", &text, Badge::Accent),
                |_| {},
            );
            bar(ui, &p, done, total);
        });
        kit::footer(ui, &p, |ui| {
            kit::right(ui, |ui| {
                if kit::plain(ui, &p, "Cancel") {
                    self.progress.cancel.store(true, Ordering::Relaxed);
                }
            });
        });
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(200));
    }

    fn closed(&mut self, _app: &mut App) {
        self.progress.cancel.store(true, Ordering::Relaxed);
    }
    any_body!();
}

/// A progress bar with "1.2 MB of 3.4 MB" under it.
fn bar(ui: &mut Ui, p: &crate::theme::Palette, done: i64, total: i64) {
    let fraction = if total > 0 {
        (done as f32 / total as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    crate::main_window::bar(ui, p, fraction, 8.0);
    let size = lider_core::fmt::size(done);
    let text = if total > 0 {
        format!("{size} of {}", lider_core::fmt::size(total))
    } else {
        size
    };
    ui.label(RichText::new(text).monospace().color(p.mu));
}

// ---------------------------------------------------------------------- Download yt-dlp?

pub struct YtDlp {
    /// The address that needs it: added again once yt-dlp is there.
    request: Request,
    asset: &'static str,
    progress: Option<Arc<Progress>>,
    slot: Slot<Result<std::path::PathBuf, String>>,
}

/// YouTube needs yt-dlp and it is not here: offer to download it, then go on with `request`.
pub fn offer_yt_dlp(app: &mut App, request: Request) {
    let Some(asset) = tools::yt_dlp_asset() else {
        app.boxx = Some(
            MsgBox::info(
                "LID",
                "Downloading from YouTube needs yt-dlp, which has no program for this system.\n\nInstall it with: pip install yt-dlp",
            )
            .with_icon("info", true),
        );
        return;
    };
    app.open_dialog(Dialog::new(
        "ytdlp",
        "Download yt-dlp?",
        (560.0, 280.0),
        YtDlp {
            request,
            asset,
            progress: None,
            slot: Default::default(),
        },
    ));
}

impl YtDlp {
    fn start(&mut self, app: &mut App) -> Result<(), String> {
        let client = app.manager.client().map_err(|e| e.message())?;
        let progress: Arc<Progress> = Arc::new(Progress {
            total: (-1).into(),
            ..Default::default()
        });
        let (p, s, dirs) = (progress.clone(), self.slot.clone(), app.dirs.clone());
        app.rt.spawn(async move {
            let r = tools::install_yt_dlp(&client, &dirs, &p).await;
            *s.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
        });
        self.progress = Some(progress);
        Ok(())
    }
}

impl Body for YtDlp {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        if let Some(r) = take(&self.slot) {
            self.progress = None;
            match r {
                Ok(_) => {
                    app.requests.push(self.request.clone());
                    app.take_requests();
                    cx.close = true;
                    return;
                }
                Err(e) if e == "cancelled" => {
                    cx.close = true;
                    return;
                }
                Err(e) => {
                    *cx.boxx = Some(
                        MsgBox::info("LID", &format!("Cannot download yt-dlp:\n{e}"))
                            .with_icon("info", true),
                    );
                    return;
                }
            }
        }
        let mut get = false;
        let mut cancel = false;
        match &self.progress {
            None => {
                kit::body(ui, |ui| {
                    kit::header(
                        ui,
                        &p,
                        &Head::new("category-video", "Download yt-dlp?", "", Badge::Accent),
                        |_| {},
                    );
                    ui.add(
                        egui::Label::new(format!(
                            "LID reads YouTube pages with yt-dlp, which is not on this computer yet.\n\nLID can download it now ({}) from its official releases:\n{}\n\nThe file is checked against the release's checksums and kept in LID's own folder.",
                            self.asset,
                            tools::PROJECT_URL
                        ))
                        .wrap(),
                    );
                });
                kit::footer(ui, &p, |ui| {
                    kit::right(ui, |ui| {
                        get |= kit::primary(ui, &p, "Download");
                        cancel |= kit::plain(ui, &p, "Cancel");
                    });
                });
                get |= cx.enter(ui);
            }
            Some(progress) => {
                cx.title = Some("Downloading yt-dlp".into());
                let done = progress.done.load(Ordering::Relaxed);
                let total = progress.total.load(Ordering::Relaxed);
                kit::body(ui, |ui| {
                    kit::header(
                        ui,
                        &p,
                        &Head::new(
                            "category-video",
                            "Downloading yt-dlp",
                            "From the official yt-dlp releases on GitHub",
                            Badge::Accent,
                        ),
                        |_| {},
                    );
                    bar(ui, &p, done, total);
                });
                kit::footer(ui, &p, |ui| {
                    kit::right(ui, |ui| {
                        if kit::plain(ui, &p, "Cancel") {
                            progress.cancel.store(true, Ordering::Relaxed);
                        }
                    });
                });
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
        if cancel || cx.escape(ui) {
            cx.close = true;
        } else if get && let Err(e) = self.start(app) {
            *cx.boxx = Some(MsgBox::info(
                "LID",
                &format!("Cannot download yt-dlp:\n{e}"),
            ));
        }
    }

    fn answer(&mut self, _app: &mut App, _a: Answer, cx: &mut Cx) {
        cx.close = true;
    }

    fn closed(&mut self, _app: &mut App) {
        if let Some(p) = &self.progress {
            p.cancel.store(true, Ordering::Relaxed);
        }
    }
    any_body!();
}
