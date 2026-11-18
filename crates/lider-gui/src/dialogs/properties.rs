//! "File Properties" (1.x `properties.PropertiesDialog`, the Tauri build's Properties.svelte): rename or
//! move a download, its address, description, checksum, referer and login.

use egui::{RichText, Ui};
use lider_core::checksum;
use lider_core::fmt;
use lider_core::models::{Download, Status};
use lider_core::view;
use lider_sys::platform::prelude::*;

use super::kit::{self, Look};
use super::{Body, Cx, Dialog};
use crate::any_body;
use crate::app::App;
use crate::boxes::MsgBox;

pub struct Properties {
    pub id: i64,
    pub filename: String,
    pub save_dir: String,
    pub url: String,
    pub description: String,
    pub referer: String,
    pub username: String,
    pub password: String,
    pub checksum: kit::ChecksumEdit,
}

pub fn label(id: i64) -> String {
    format!("properties-{id}")
}

pub fn open(app: &mut App, id: i64) {
    let Some(d) = app.manager.get(id) else {
        return;
    };
    let body = Properties {
        id,
        filename: d.filename.clone(),
        save_dir: d.save_dir.clone(),
        url: d.url.clone(),
        description: d.description.clone(),
        referer: d.referer.clone(),
        username: d.username.clone(),
        password: d.password.clone(),
        checksum: kit::ChecksumEdit {
            hint: String::new(),
            ..kit::ChecksumEdit::with_value(&d.checksum)
        },
    };
    app.open_dialog(Dialog::new(
        &label(id),
        "File Properties",
        (680.0, 600.0),
        body,
    ));
}

/// What the edited fields change.
#[derive(Debug, Clone, Default)]
pub struct Patch {
    pub filename: String,
    pub save_dir: String,
    pub url: String,
    pub description: String,
    pub referer: String,
    pub username: String,
    pub password: String,
    pub checksum: String,
}

/// OK in File Properties: rename or move (a finished file is moved on disk; refused while it runs),
/// then the other fields; a changed checksum forgets the last check.
pub fn apply(app: &mut App, id: i64, patch: Patch) -> Result<(), String> {
    let d = app.manager.get(id).ok_or("no such download")?;
    let name = if patch.filename.trim().is_empty() {
        d.filename.clone()
    } else {
        patch.filename.trim().to_string()
    };
    let folder = if patch.save_dir.trim().is_empty() {
        d.save_dir.clone()
    } else {
        patch.save_dir.trim().to_string()
    };
    if name != d.filename || folder != d.save_dir {
        if app.manager.is_running(id) {
            return Err("Stop the download before moving or renaming it.".into());
        }
        if d.status == Status::Complete && std::path::Path::new(&d.path()).exists() {
            let target = std::path::Path::new(&folder).join(&name);
            std::fs::create_dir_all(&folder)
                .and_then(|_| match std::fs::rename(d.path(), &target) {
                    // Another file system: copy, then remove.
                    Err(e) if e.raw_os_error() == Some(18) => std::fs::copy(d.path(), &target)
                        .and_then(|_| std::fs::remove_file(d.path())),
                    r => r,
                })
                .map_err(|e| format!("Cannot move the file:\n{e}"))?;
        }
    }
    let changed_checksum = patch.checksum != d.checksum;
    app.manager.edit(id, |x| {
        x.filename = name.clone();
        x.save_dir = folder.clone();
        if !patch.url.trim().is_empty() {
            x.url = patch.url.trim().to_string();
        }
        x.description = patch.description.clone();
        x.referer = patch.referer.clone();
        x.username = patch.username.clone();
        x.password = patch.password.clone();
        if changed_checksum {
            x.checksum = patch.checksum.clone();
            x.checksum_result.clear();
            x.checksum_actual.clear();
            x.checksum_retried = false;
        }
    });
    app.rebuild();
    Ok(())
}

/// The status pill (text, tone).
fn status(app: &App, d: &Download) -> (String, &'static str) {
    match d.status {
        Status::Error if !d.error.is_empty() => (format!("! Error: {}", d.error), "bad"),
        Status::Error => ("! Error".into(), "bad"),
        Status::Complete => ("✓ Complete".into(), "ok"),
        _ => {
            let t = fmt::status(d);
            let tone = if app.manager.is_running(d.id) {
                "accent"
            } else {
                "muted"
            };
            (if t.is_empty() { "Queued".into() } else { t }, tone)
        }
    }
}

/// The checksum pill, the result line and its tone (`okText`, `badText`, `muted`).
fn checksum_state(app: &App, d: &Download) -> ((String, &'static str), String, &'static str) {
    let when = || {
        chrono::DateTime::from_timestamp(d.checksum_checked as i64, 0)
            .map(|t| {
                t.with_timezone(&chrono::Local)
                    .format("%b %d, %H:%M")
                    .to_string()
            })
            .unwrap_or_default()
    };
    if let Some(v) = app.manager.verify_progress(d.id) {
        return (
            ("Verifying".into(), "accent"),
            format!("Checking… {:.0}%", v * 100.0),
            "muted",
        );
    }
    match d.checksum_result.as_str() {
        "verified" => (
            ("✓ Verified".into(), "ok"),
            format!(
                "Matches · checked {} in {:.1} s",
                when(),
                d.checksum_seconds
            ),
            "okText",
        ),
        "mismatch" => (
            ("Checksum mismatch".into(), "bad"),
            format!(
                "Doesn't match · checked {} in {:.1} s",
                when(),
                d.checksum_seconds
            ),
            "badText",
        ),
        "skipped" => (
            ("Not checked".into(), "muted"),
            "Skipped — the file was not checked".into(),
            "muted",
        ),
        _ if !d.checksum.is_empty() => (
            ("Not checked".into(), "muted"),
            "Not checked yet".into(),
            "muted",
        ),
        _ => (
            ("Not set".into(), "muted"),
            "No checksum entered".into(),
            "muted",
        ),
    }
}

/// "Last try: … · Result: …".
fn footer_text(d: &Download) -> String {
    let last = if d.last_try > 0.0 {
        fmt::date(d.last_try)
    } else {
        "never".into()
    };
    let result = if d.status == Status::Error {
        format!("Error: {}", d.error)
    } else {
        fmt::status(d)
    };
    let tail = match d.checksum_result.as_str() {
        r @ ("verified" | "mismatch") => format!(" · {} {r}", checksum::title(&d.checksum)),
        _ => String::new(),
    };
    format!("Last try: {last} · Result: {result}{tail}")
}

impl Properties {
    fn patch(&self) -> Patch {
        Patch {
            filename: self.filename.clone(),
            save_dir: self.save_dir.clone(),
            url: self.url.clone(),
            description: self.description.clone(),
            referer: self.referer.trim().to_string(),
            username: self.username.clone(),
            password: self.password.clone(),
            checksum: self.checksum.value().unwrap_or_default(),
        }
    }
}

impl Body for Properties {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let Some(d) = app.manager.get(self.id) else {
            cx.close = true;
            return;
        };
        let (status_text, status_tone) = status(app, &d);
        let ((cpill, ctone), ctext, ctext_tone) = checksum_state(app, &d);
        let verifying = app.manager.verify_progress(self.id).is_some();
        let complete = d.status == Status::Complete;
        let youtube = d.media.is_some();
        let value = self.checksum.value();
        let can_verify = matches!(&value, Ok(v) if !v.is_empty()) && !verifying && complete;
        let mut action: Option<&'static str> = None;
        kit::body(ui, |ui| {
            ui.horizontal(|ui| {
                kit::badge_icon(ui, &p, &view::category_icon(&d.category), kit::Badge::Tile);
                ui.vertical(|ui| {
                    let by = kit::caption(ui, &p, "File name");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filename)
                            .font(egui::FontId::proportional(15.0))
                            .desired_width(f32::INFINITY),
                    )
                    .labelled_by(by);
                    ui.horizontal(|ui| {
                        kit::pill(ui, &p, &status_text, status_tone);
                        kit::pill(ui, &p, &d.category, "muted");
                    });
                });
            });
            let w = 100.0;
            kit::panel(ui, &p, |ui| {
                let ext = lider_core::categories::extension(&d.filename);
                let kind = match (
                    mime_guess::from_path(&d.filename).first_raw(),
                    ext.is_empty(),
                ) {
                    (Some(mt), _) => mt.to_string(),
                    (None, false) => ext.to_uppercase(),
                    (None, true) => "Unknown".into(),
                };
                kit::row(ui, "Type", w, |ui, _| {
                    ui.label(format!("{kind} file ({})", d.category));
                });
                let size = if d.size < 0 {
                    "(unknown)".into()
                } else {
                    kit::size_with_bytes(d.size)
                };
                kit::row(ui, "Size", w, |ui, _| {
                    ui.label(RichText::new(size).monospace());
                });
                let variant = d
                    .media
                    .as_ref()
                    .and_then(|md| md.get("variant"))
                    .and_then(|v| v.get("label"))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                if let Some(v) = variant {
                    kit::row(ui, "YouTube", w, |ui, _| {
                        ui.label(v);
                    });
                }
                kit::row(ui, "Save To", w, |ui, by| {
                    kit::folder_field_with(ui, &p, &mut self.save_dir, by, "&Browse...", "Save To");
                });
                kit::row(ui, "Address", w, |ui, by| {
                    ui.add_enabled(
                        !youtube,
                        egui::TextEdit::singleline(&mut self.url)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY),
                    )
                    .labelled_by(by);
                });
                kit::row(ui, "Description", w, |ui, by| {
                    kit::field(ui, &mut self.description, by);
                });
            });
            kit::panel(ui, &p, |ui| {
                ui.horizontal(|ui| {
                    kit::caption(ui, &p, "Checksum");
                    kit::pill(ui, &p, &cpill, ctone);
                });
                kit::row(ui, "Expected", w, |ui, by| {
                    ui.vertical(|ui| {
                        self.checksum.ui(ui, &p, true, by);
                    });
                });
                kit::row(ui, "Result", w, |ui, _| {
                    let colour = match ctext_tone {
                        "okText" => p.ok,
                        "badText" => p.bad,
                        _ => p.mu,
                    };
                    ui.label(RichText::new(&ctext).color(colour));
                    kit::right(ui, |ui| {
                        if kit::button(ui, &p, "Verify now", Look::Normal, can_verify) {
                            action = Some("verify");
                        }
                    });
                });
            });
            kit::panel(ui, &p, |ui| {
                kit::caption(ui, &p, "Web page and authorization");
                kit::row(ui, "Parent page", w, |ui, _| {
                    if d.parent_page.is_empty() {
                        ui.label(RichText::new("Unknown").monospace().color(p.mu));
                    } else {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&d.parent_page).monospace().color(p.actx),
                            )
                            .truncate(),
                        );
                    }
                });
                kit::row(ui, "Referer", w, |ui, by| {
                    kit::mono_field(ui, &mut self.referer, by);
                });
                ui.horizontal(|ui| {
                    let half = (ui.available_width() - 10.0) / 2.0;
                    ui.vertical(|ui| {
                        ui.set_width(half);
                        kit::row(ui, "Login", w, |ui, by| {
                            kit::field(ui, &mut self.username, by);
                        });
                    });
                    ui.vertical(|ui| {
                        kit::row(ui, "Password", 70.0, |ui, by| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.password)
                                    .password(true)
                                    .desired_width(f32::INFINITY),
                            )
                            .labelled_by(by);
                        });
                    });
                });
            });
        });
        kit::footer(ui, &p, |ui| {
            let text = footer_text(&d);
            ui.add(egui::Label::new(RichText::new(text).small().color(p.fa)).truncate());
            kit::right(ui, |ui| {
                if kit::primary(ui, &p, "OK") {
                    action = Some("ok");
                }
                if kit::button(ui, &p, "Open", Look::Normal, complete) {
                    action = Some("open");
                }
            });
        });
        if action.is_none() {
            if cx.escape(ui) {
                cx.close = true;
            } else if cx.enter(ui) {
                action = Some("ok");
            }
        }
        match action {
            Some("verify") => {
                if let Ok(cs) = value {
                    let m = &mut app.manager;
                    if m.downloads.get(&self.id).is_some_and(|d| d.checksum != cs) {
                        m.edit(self.id, |d| {
                            d.checksum = cs.clone();
                            d.checksum_result.clear();
                            d.checksum_actual.clear();
                        });
                    }
                    m.verify(self.id, true);
                    app.rebuild();
                }
            }
            Some("open") => {
                let _ = os().open_path(&d.path());
            }
            Some("ok") => {
                if let Err(e) = &value {
                    *cx.boxx = Some(MsgBox::info("File Properties", e).with_icon("info", true));
                    return;
                }
                match apply(app, self.id, self.patch()) {
                    Ok(()) => cx.close = true,
                    Err(e) => {
                        *cx.boxx = Some(MsgBox::info("File Properties", &e).with_icon("info", true))
                    }
                }
            }
            _ => {}
        }
    }
    any_body!();
}
