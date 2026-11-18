//! "Check hash" (1.x `checkhash.CheckHashDialog`, the Tauri build's CheckHash.svelte): compare a finished
//! file with a pasted value, or only compute its hash; for an unfinished download, check it as soon as
//! it finishes. The file is read on a thread of its own; closing the window stops it.

use std::sync::Arc;

use egui::{RichText, Ui};
use lider_core::checksum::{self, CheckResult, Outcome, Verifier};
use lider_core::fmt;
use lider_core::models::Status;

use super::kit::{self, Badge, Head, Look};
use super::{Body, Cx, Dialog};
use crate::any_body;
use crate::app::App;
use crate::boxes::MsgBox;
use crate::icons::icon;

const READY: &str = "Pre-filled with the checksum stored for this download. Paste the value published by the site, or leave it empty to only compute the file's hash.";
const RUNNING: &str = "Paste the value published by the site. Computing a hash without a value is available once the file is complete.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// A finished file: Check now / Compute hash.
    Ready,
    Reading,
    Match,
    Mismatch,
    Computed,
    /// Still downloading: Check when finished.
    Running,
}

pub struct CheckHash {
    pub id: i64,
    pub state: State,
    pub edit: kit::ChecksumEdit,
    pub save: bool,
    /// The check reading the file, and the value it compares with.
    verifier: Option<(Arc<Verifier>, String)>,
    /// The last finished check.
    pub outcome: Option<Outcome>,
    pub algorithm: String,
    pub read_error: String,
    pub saved: bool,
    last_text: String,
}

pub fn open(app: &mut App, id: i64) {
    let Some(d) = app.manager.get(id) else {
        return;
    };
    let finished = matches!(d.status, Status::Complete | Status::Verifying);
    let edit = kit::ChecksumEdit::with_value(&d.checksum);
    let body = CheckHash {
        id,
        state: if finished {
            State::Ready
        } else {
            State::Running
        },
        last_text: edit.text.clone(),
        edit,
        save: true,
        verifier: None,
        outcome: None,
        algorithm: String::new(),
        read_error: String::new(),
        saved: false,
    };
    app.open_dialog(Dialog::new(
        &format!("checkhash-{id}"),
        "Check hash",
        (640.0, 480.0),
        body,
    ));
}

/// "name · 12.3 MB", or "name · 12.3 of 40 MB" while it downloads.
fn subtitle(d: &lider_core::models::Download) -> String {
    let finished = matches!(d.status, Status::Complete | Status::Verifying);
    if finished || d.size <= 0 {
        let n = if d.size > 0 { d.size } else { d.downloaded };
        format!("{} · {}", d.filename, fmt::size(n))
    } else {
        let done = fmt::size(d.downloaded);
        let number = done
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned();
        format!("{} · {} of {}", d.filename, number, fmt::size(d.size))
    }
}

impl CheckHash {
    /// Start reading the file: compare with the value, or compute (`algo:`).
    fn run(&mut self, path: String) {
        let value = self.edit.value().unwrap_or_default();
        let algo = if !value.is_empty() {
            checksum::split(&value).0.to_string()
        } else if self.edit.algorithm == "auto" {
            "sha256".into()
        } else {
            self.edit.algorithm.clone()
        };
        let target = if value.is_empty() {
            format!("{algo}:")
        } else {
            value.clone()
        };
        let v = Arc::new(Verifier::new(path, &target, true, self.id));
        self.algorithm = checksum::algorithm_title(&v.algorithm).to_string();
        let worker = v.clone();
        let started = std::thread::Builder::new()
            .name(format!("checkhash-{}", self.id))
            .spawn(move || {
                worker.run();
            });
        self.read_error.clear();
        if let Err(e) = started {
            self.read_error = format!("Could not read the file: {e}");
            return;
        }
        self.verifier = Some((v, value));
        self.state = State::Reading;
    }

    fn stop(&mut self) {
        if let Some((v, _)) = self.verifier.take() {
            v.skip();
        }
    }

    /// The check ended: show it, save it when asked.
    fn finished(&mut self, app: &mut App, o: Outcome, value: String) {
        match o.result {
            CheckResult::Error => {
                self.read_error = format!("Could not read the file: {}", o.error);
                self.state = State::Ready;
                return;
            }
            CheckResult::Skipped => {
                self.state = State::Ready;
                return;
            }
            _ => {}
        }
        self.saved = false;
        let judged = matches!(o.result, CheckResult::Verified | CheckResult::Mismatch);
        if self.save && !value.is_empty() && judged {
            app.manager
                .record_check(self.id, &value, o.result.as_str(), &o.actual, o.seconds);
            app.rebuild();
            self.saved = true;
        }
        self.state = match o.result {
            CheckResult::Verified => State::Match,
            CheckResult::Mismatch => State::Mismatch,
            _ => State::Computed,
        };
        self.outcome = Some(o);
    }

    fn hash_field(ui: &mut Ui, p: &crate::theme::Palette, caption: &str, value: &str) {
        let by = kit::caption(ui, p, caption);
        ui.horizontal(|ui| {
            let w = ui.available_width() - 70.0;
            ui.add_sized(
                [w, 24.0],
                egui::TextEdit::singleline(&mut value.to_string()).font(egui::TextStyle::Monospace),
            )
            .labelled_by(by);
            if kit::plain(ui, p, "Copy") {
                ui.ctx().copy_text(value.to_string());
            }
        });
    }
}

impl Body for CheckHash {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let Some(d) = app.manager.get(self.id) else {
            cx.close = true;
            return;
        };
        // The check reading the file.
        let done = self.verifier.as_ref().map(|(v, _)| v.outcome());
        match done {
            Some(Some(o)) => {
                if let Some((_, value)) = self.verifier.take() {
                    self.finished(app, o, value);
                }
            }
            Some(None) => ui
                .ctx()
                .request_repaint_after(std::time::Duration::from_millis(100)),
            None => {}
        }
        let path = d.path();
        let exists = std::path::Path::new(&path).is_file();
        let finished = matches!(d.status, Status::Complete | Status::Verifying);
        if self.state == State::Running && finished {
            self.state = State::Ready;
        }
        let valid = self.edit.valid();
        let value = self.edit.value().unwrap_or_default();
        let can_run = valid && self.state == State::Ready && exists;
        let can_save =
            valid && !value.is_empty() && matches!(self.state, State::Ready | State::Reading);
        let kind = if self.algorithm.is_empty() {
            "SHA-256".to_string()
        } else {
            self.algorithm.clone()
        };
        let mut action: Option<&'static str> = None;
        kit::body(ui, |ui| {
            kit::header(
                ui,
                &p,
                &Head::new("checksum", "Check hash", &subtitle(&d), Badge::Accent),
                |_| {},
            );
            let by = kit::caption(ui, &p, "Expected checksum");
            self.edit.hint = if self.state == State::Running {
                RUNNING
            } else {
                READY
            }
            .into();
            self.edit.ui(ui, &p, self.state != State::Reading, by);
            if self.edit.text != self.last_text {
                self.last_text = self.edit.text.clone();
                if matches!(self.state, State::Match | State::Mismatch | State::Computed) {
                    self.state = State::Ready;
                }
            }
            if self.state != State::Running {
                ui.add_enabled(
                    can_save,
                    egui::Checkbox::new(&mut self.save, "Save this checksum to the download"),
                );
            }
            match self.state {
                State::Running => {
                    let percent = if d.size > 0 {
                        format!(" ({:.0}%)", d.progress() * 100.0)
                    } else {
                        String::new()
                    };
                    kit::accent_note(
                        ui,
                        &p,
                        "clock",
                        format!(
                            "The file is still downloading{percent}. LID checks it as soon as the download finishes."
                        ),
                    );
                }
                State::Ready => {
                    if finished && !exists {
                        kit::error_text(ui, &p, &format!("File not found at {path}"));
                    }
                    if !self.read_error.is_empty() {
                        kit::error_text(ui, &p, &self.read_error);
                    }
                }
                State::Reading => {
                    if let Some((v, _)) = &self.verifier {
                        let progress = v.progress();
                        let total = std::fs::metadata(&v.path)
                            .map(|m| m.len() as i64)
                            .unwrap_or(0);
                        kit::panel(ui, &p, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "Reading the file… {:.0}%",
                                        progress * 100.0
                                    ))
                                    .strong(),
                                );
                                kit::right(ui, |ui| {
                                    if ui.button("Stop").clicked() {
                                        action = Some("stop");
                                    }
                                });
                            });
                            ui.label(
                                RichText::new(format!(
                                    "{} of {}",
                                    fmt::size((total as f64 * progress) as i64),
                                    fmt::size(total)
                                ))
                                .monospace()
                                .color(p.mu),
                            );
                            crate::main_window::bar(ui, &p, progress as f32, 8.0);
                            kit::faint(
                                ui,
                                &p,
                                &format!("{kind} · runs in the background; the list stays usable."),
                            );
                        });
                    }
                }
                State::Match => {
                    if let Some(o) = &self.outcome {
                        kit::panel(ui, &p, |ui| {
                            ui.horizontal(|ui| {
                                ui.add(icon("shield-check", 18.0, &p, p.ok));
                                ui.label(
                                    RichText::new("Matches — the file is intact")
                                        .strong()
                                        .color(p.ok),
                                );
                                kit::right(ui, |ui| {
                                    kit::muted(ui, &p, &format!("{kind} · {:.1} s", o.seconds));
                                });
                            });
                            Self::hash_field(ui, &p, &format!("Computed {kind}"), &o.actual);
                        });
                        kit::faint(
                            ui,
                            &p,
                            if self.saved {
                                "Saved to the download — the list shows ✓ Verified."
                            } else {
                                "Not saved — the download is unchanged."
                            },
                        );
                    }
                }
                State::Mismatch => {
                    if let Some(o) = &self.outcome {
                        kit::panel(ui, &p, |ui| {
                            egui::Grid::new("checkhash-pair")
                                .num_columns(2)
                                .spacing([12.0, 6.0])
                                .show(ui, |ui| {
                                    kit::caption(ui, &p, "Expected");
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(checksum::split(&value).1).monospace(),
                                        )
                                        .wrap(),
                                    );
                                    ui.end_row();
                                    kit::caption(ui, &p, "Actual");
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&o.actual).monospace().color(p.bad),
                                        )
                                        .wrap(),
                                    );
                                    ui.end_row();
                                });
                            let saved = if self.saved {
                                " Saved to the download — the list shows Checksum mismatch."
                            } else {
                                ""
                            };
                            kit::faint(
                                ui,
                                &p,
                                &format!(
                                    "{kind}. Expected is the value above; Actual is computed from the file on disk.{saved}"
                                ),
                            );
                        });
                    }
                }
                State::Computed => {
                    if let Some(o) = &self.outcome {
                        kit::panel(ui, &p, |ui| {
                            Self::hash_field(ui, &p, &format!("{kind} of the file"), &o.actual);
                        });
                        kit::faint(
                            ui,
                            &p,
                            "No expected value, so there is no verdict and nothing is saved. Pick MD5, SHA-1 or SHA-512 above to compute another type.",
                        );
                    }
                }
            }
        });
        let state = self.state;
        let run_label = if self.edit.text.trim().is_empty() {
            "Compute hash"
        } else {
            "Check now"
        };
        kit::footer(ui, &p, |ui| match state {
            State::Mismatch => {
                if kit::icon_button(ui, &p, "delete", "Delete file", Look::Danger) {
                    action = Some("delete");
                }
                kit::right(ui, |ui| {
                    if kit::plain(ui, &p, "Keep file") {
                        action = Some("keep");
                    }
                    if kit::primary(ui, &p, "Download again") {
                        action = Some("again");
                    }
                });
            }
            State::Match | State::Computed => {
                kit::right(ui, |ui| {
                    if kit::primary(ui, &p, "Close") {
                        action = Some("close");
                    }
                    if kit::plain(ui, &p, "Check again") {
                        action = Some("check_again");
                    }
                });
            }
            State::Running => {
                kit::right(ui, |ui| {
                    if kit::button(
                        ui,
                        &p,
                        "Check when finished",
                        Look::Primary,
                        valid && !value.is_empty(),
                    ) {
                        action = Some("later");
                    }
                    if kit::plain(ui, &p, "Close") {
                        action = Some("close");
                    }
                });
            }
            State::Ready | State::Reading => {
                kit::right(ui, |ui| {
                    if kit::button(ui, &p, run_label, Look::Primary, can_run) {
                        action = Some("run");
                    }
                    if kit::plain(ui, &p, "Close") {
                        action = Some("close");
                    }
                });
            }
        });
        if action.is_none() {
            if cx.escape(ui) {
                action = Some("close");
            } else if cx.enter(ui) {
                // Enter presses the default button; a disabled one does nothing (1.x QDialog).
                action = match state {
                    State::Ready if can_run => Some("run"),
                    State::Running if valid && !value.is_empty() => Some("later"),
                    State::Mismatch => Some("again"),
                    State::Match | State::Computed => Some("close"),
                    _ => None,
                };
            }
        }
        match action {
            Some("run") => self.run(path),
            Some("stop") => {
                self.stop();
                self.state = State::Ready;
            }
            Some("check_again") => self.state = State::Ready,
            Some("later") => {
                app.manager.check_when_finished(self.id, &value);
                app.rebuild();
                cx.close = true;
            }
            Some(c @ ("again" | "delete" | "keep")) => {
                match super::complete::mismatch_choice(app, self.id, c) {
                    Ok(()) => cx.close = true,
                    Err(e) => *cx.boxx = Some(MsgBox::info("Check hash", &e)),
                }
            }
            Some("close") => cx.close = true,
            _ => {}
        }
    }

    /// Closing the window (any way) stops what it was reading; nothing is saved (1.x `done()`).
    fn closed(&mut self, _app: &mut App) {
        self.stop();
    }
    any_body!();
}
