//! The progress window of a download (1.x `progress.ProgressDialog`, the Tauri build's Progress.svelte):
//! three tabs (status, the download's speed limit, options on completion), the percentage, the
//! per-connection bar and the connections table, refreshed with every frame from the manager.

use egui::{RichText, Ui};
use lider_core::fmt;
use lider_core::models::{Download, Status};

use super::kit::{self, Look};
use super::{Body, Cx, Dialog};
use crate::any_body;
use crate::app::App;

/// The window's label for a download.
pub fn label(id: i64) -> String {
    format!("progress-{id}")
}

/// "Options on completion" of a progress window (show Download complete, exit, turn off).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletionOptions {
    pub show_complete: bool,
    pub exit: bool,
    pub shutdown: bool,
}

pub struct Progress {
    pub id: i64,
    pub tab: usize,
    /// The segment bar and the connections table show.
    pub details: bool,
    pub limit_on: bool,
    pub limit_kbps: i64,
    /// Keep the limit on the download (applied at its next start too).
    pub remember: bool,
    pub options: CompletionOptions,
}

/// Open (or bring forward) the progress window of `id`, unless Options turn it off (`force` ignores that).
pub fn open(app: &mut App, id: i64, force: bool) {
    if !force && !app.manager.settings.show_progress_dialog {
        return;
    }
    let Some(d) = app.manager.downloads.get(&id) else {
        return;
    };
    let body = Progress {
        id,
        tab: 0,
        details: true,
        limit_on: d.speed_limit_kbps > 0,
        limit_kbps: if d.speed_limit_kbps > 0 {
            d.speed_limit_kbps
        } else {
            app.manager.settings.speed_limit_kbps
        },
        remember: true,
        options: CompletionOptions {
            show_complete: app.manager.settings.show_complete_dialog,
            exit: false,
            shutdown: false,
        },
    };
    let title = title(d);
    // 1.x keeps the progress window at least 600 px wide.
    app.open_dialog(Dialog::new(&label(id), &title, (640.0, 520.0), body).min_width(600.0));
}

/// What the open progress window of `id` asks for when the download completes (None without one).
pub fn completion_options(app: &mut App, id: i64) -> Option<CompletionOptions> {
    app.dialog_body::<Progress>(&label(id)).map(|p| p.options)
}

/// "42% name.iso", or the size so far while the total is unknown.
fn title(d: &Download) -> String {
    let done = if d.size > 0 {
        format!("{:.0}%", d.progress() * 100.0)
    } else {
        fmt::size(d.downloaded)
    };
    format!("{done} {}", d.filename)
}

/// The status pill: text and tone (`progress_info` of the Tauri build).
fn status(app: &App, d: &Download) -> (String, &'static str) {
    let m = &app.manager;
    let text = m.task(d.id).map(|t| t.status_text()).unwrap_or_default();
    if let Some(v) = m.verify_progress(d.id) {
        (format!("Verifying {:.0}%", v * 100.0), "accent")
    } else if !text.is_empty() {
        (text, "accent")
    } else {
        match d.status {
            Status::Error => (format!("Error: {}", d.error), "bad"),
            Status::Paused => ("Paused".into(), "muted"),
            Status::Downloading => ("⤓ Receiving data…".into(), "accent"),
            Status::Assembling => ("Assembling…".into(), "accent"),
            Status::Complete => ("✓ Complete".into(), "ok"),
            _ => ("Connecting…".into(), "accent"),
        }
    }
}

impl Progress {
    /// Apply the speed limit at once; keep it on the download when `remember`.
    fn apply_limit(&mut self, app: &mut App) {
        self.limit_kbps = self.limit_kbps.clamp(1, 10_000_000);
        let kbps = if self.limit_on { self.limit_kbps } else { 0 };
        let m = &mut app.manager;
        if let Some(t) = m.task(self.id) {
            t.set_speed_limit(kbps);
        }
        if self.remember {
            m.edit(self.id, |d| d.speed_limit_kbps = kbps);
        }
    }
}

impl Body for Progress {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let Some(d) = app.manager.get(self.id) else {
            cx.close = true;
            return;
        };
        cx.title = Some(title(&d));
        let m = &app.manager;
        let running = m.is_running(self.id);
        let snap = m.task(self.id).map(|t| t.snapshot());
        let rate = if running {
            fmt::rate(snap.as_ref().map_or(0.0, |s| s.rate))
        } else {
            String::new()
        };
        let left = snap.as_ref().map_or(-1, |s| s.time_left);
        let (status_text, status_tone) = status(app, &d);
        let quality = d
            .media
            .as_ref()
            .and_then(|md| md.get("variant"))
            .map(|v| {
                format!(
                    "   ({} {})",
                    v["quality"].as_str().unwrap_or(""),
                    v["ext"].as_str().unwrap_or("").to_uppercase()
                )
            })
            .unwrap_or_default();
        let mut action: Option<&'static str> = None;
        let mut limit_changed = false;
        kit::body(ui, |ui| {
            kit::tab_strip(
                ui,
                &p,
                &["Download status", "Speed Limiter", "Options on completion"],
                &mut self.tab,
            );
            let w = 130.0;
            let value = |ui: &mut Ui, by: egui::Id, text: &str| {
                ui.label(RichText::new(text).monospace()).labelled_by(by);
            };
            match self.tab {
                0 => {
                    ui.add(
                        egui::Label::new(RichText::new(format!("{}{quality}", d.url)).monospace())
                            .wrap(),
                    );
                    kit::row(ui, "Status", w, |ui, _| {
                        kit::pill(ui, &p, &status_text, status_tone);
                    });
                    let size = if d.size >= 0 {
                        fmt::size(d.size)
                    } else {
                        "(unknown)".into()
                    };
                    kit::row(ui, "File size", w, |ui, by| value(ui, by, &size));
                    let done = if d.size > 0 {
                        format!(
                            "{} ({:.1} %)",
                            fmt::size(d.downloaded),
                            d.progress() * 100.0
                        )
                    } else {
                        fmt::size(d.downloaded)
                    };
                    kit::row(ui, "Downloaded", w, |ui, by| value(ui, by, &done));
                    kit::row(ui, "Transfer rate", w, |ui, by| value(ui, by, &rate));
                    let time = if running && left >= 0 {
                        fmt::duration(left)
                    } else {
                        "(unknown)".into()
                    };
                    kit::row(ui, "Time left", w, |ui, by| value(ui, by, &time));
                    let (resume, tone) = match d.resumable {
                        Some(true) => ("✓ Yes", "ok"),
                        Some(false) => ("No", "muted"),
                        None => ("(unknown)", "muted"),
                    };
                    kit::row(ui, "Resume capability", w, |ui, _| {
                        kit::pill(ui, &p, resume, tone);
                    });
                }
                1 => {
                    kit::row(ui, "Transfer rate", w, |ui, by| value(ui, by, &rate));
                    limit_changed |= ui
                        .checkbox(&mut self.limit_on, "Use Speed Limiter")
                        .changed();
                    kit::row(ui, "Maximum speed", w, |ui, by| {
                        limit_changed |= ui
                            .add(
                                egui::DragValue::new(&mut self.limit_kbps)
                                    .range(1..=10_000_000)
                                    .speed(5.0),
                            )
                            .labelled_by(by)
                            .changed();
                        ui.label("KBytes/sec");
                    });
                    limit_changed |= ui
                        .checkbox(
                            &mut self.remember,
                            "Remember Speed Limiter settings for this file on download start",
                        )
                        .changed();
                }
                _ => {
                    ui.checkbox(
                        &mut self.options.show_complete,
                        "Show download complete dialog",
                    );
                    kit::row(ui, "Save To", 70.0, |ui, by| {
                        kit::readonly(ui, &d.path(), by)
                    });
                    kit::caption(ui, &p, "Power options");
                    ui.checkbox(&mut self.options.exit, "Exit LID when done");
                    ui.checkbox(&mut self.options.shutdown, "Turn off computer when done");
                }
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let percent = if d.size > 0 {
                    format!("{:.1}%", d.progress() * 100.0)
                } else {
                    fmt::size(d.downloaded)
                };
                ui.label(RichText::new(percent).monospace().size(22.0).strong());
                ui.vertical(|ui| {
                    ui.add_space(8.0);
                    crate::main_window::bar(ui, &p, d.progress() as f32, 10.0);
                });
            });
            ui.horizontal(|ui| {
                let toggle = if self.details {
                    "<< Hide details"
                } else {
                    "Show details >>"
                };
                if ui.add(egui::Button::new(toggle).frame(false)).clicked() {
                    self.details = !self.details;
                }
                kit::right(ui, |ui| {
                    if kit::plain(ui, &p, "Cancel") {
                        action = Some("cancel");
                    }
                    let (label, act, icon) = if d.status == Status::Verifying {
                        ("Skip check", "skip", "")
                    } else if running {
                        ("Pause", "pause", "stop")
                    } else {
                        ("Resume", "resume", "resume")
                    };
                    let complete = d.status == Status::Complete;
                    let clicked = if icon.is_empty() {
                        kit::button(ui, &p, label, Look::Normal, !complete)
                    } else {
                        ui.add_enabled_ui(!complete, |ui| {
                            kit::icon_button(ui, &p, icon, label, Look::Normal)
                        })
                        .inner
                    };
                    if clicked {
                        action = Some(act);
                    }
                });
            });
            if self.details {
                kit::faint(ui, &p, lider_core::view::SEGMENTS);
                let segs = snap
                    .as_ref()
                    .map(|s| s.segments.clone())
                    .unwrap_or_else(|| d.segments.clone());
                crate::main_window::segments(ui, &p, d.size, &segs);
                kit::panel(ui, &p, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(160.0)
                        .min_scrolled_height(110.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            egui::Grid::new("progress-connections")
                                .num_columns(4)
                                .min_col_width(50.0)
                                .spacing([18.0, 4.0])
                                .show(ui, |ui| {
                                    for h in ["N.", "DOWNLOADED", "SPEED", "INFO"] {
                                        ui.label(RichText::new(h).small().strong().color(p.mu));
                                    }
                                    ui.end_row();
                                    for c in snap.iter().flat_map(|s| &s.connections) {
                                        ui.label(c.index.to_string());
                                        ui.label(RichText::new(fmt::size(c.received)).monospace());
                                        let speed = if c.alive && running {
                                            fmt::rate(c.rate)
                                        } else {
                                            String::new()
                                        };
                                        ui.label(RichText::new(speed).monospace());
                                        let info = if c.alive || running {
                                            c.status.as_str()
                                        } else {
                                            ""
                                        };
                                        if info == "Finished" {
                                            kit::pill(ui, &p, "Finished", "ok");
                                        } else {
                                            ui.label(info);
                                        }
                                        ui.end_row();
                                    }
                                });
                        });
                });
            }
        });
        if limit_changed {
            self.apply_limit(app);
        }
        if cx.escape(ui) {
            cx.close = true;
        }
        match action {
            Some("skip") => app.manager.skip_check(self.id),
            Some("pause") => app.manager.stop(self.id),
            Some("resume") => {
                app.manager.start(self.id);
            }
            Some("cancel") => {
                app.manager.stop(self.id);
                cx.close = true;
            }
            _ => return,
        }
        app.rebuild();
    }
    any_body!();
}
