//! "Download complete" and "Checksum mismatch" (1.x `progress.CompleteDialog`, `MismatchDialog`).

use egui::{RichText, Ui};
use lider_core::checksum;
use lider_core::models::Download;
use lider_core::view;
use lider_sys::platform::prelude::*;

use super::kit::{self, Badge, Head, Look};
use super::{Body, Cx, Dialog};
use crate::any_body;
use crate::app::App;
use crate::boxes::{Answer, MsgBox, Then};
use crate::icons::icon;

/// The saved path with the category icon before it.
fn saved_as(ui: &mut Ui, cx: &Cx, d: &Download) {
    let p = cx.p;
    let by = kit::caption(ui, &p, "The file saved as");
    ui.horizontal(|ui| {
        ui.add(icon(&view::category_icon(&d.category), 16.0, &p, p.mu));
        kit::readonly(ui, &d.path(), by);
    });
}

// ---------------------------------------------------------------------- Download complete

pub struct Complete {
    pub id: i64,
    pub dont_show: bool,
}

pub fn label(id: i64) -> String {
    format!("complete-{id}")
}

pub fn complete(app: &mut App, id: i64) {
    app.effects.push(crate::app::Effect::Show);
    app.open_dialog(Dialog::new(
        &label(id),
        "Download complete",
        (640.0, 360.0),
        Complete {
            id,
            dont_show: false,
        },
    ));
}

impl Body for Complete {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let Some(d) = app.manager.downloads.get(&self.id).cloned() else {
            cx.close = true;
            return;
        };
        let verified = d.checksum_result == "verified";
        let kind = checksum::title(&d.checksum);
        let mut action = None;
        kit::body(ui, |ui| {
            let downloaded = format!(
                "Downloaded {}",
                kit::size_with_bytes(d.size.max(d.downloaded))
            );
            let badge = if verified { "shield-check" } else { "finished" };
            kit::header(
                ui,
                &p,
                &Head::new(badge, "Download complete", &downloaded, Badge::Ok),
                |ui| {
                    if verified {
                        kit::pill(ui, &p, &format!("{kind} verified"), "ok");
                    }
                },
            );
            let by = kit::caption(ui, &p, "Address");
            kit::readonly(ui, &d.url, by);
            saved_as(ui, cx, &d);
            if verified {
                let by = kit::caption(ui, &p, &format!("Checksum ({kind}) · matches"));
                ui.horizontal(|ui| {
                    let w = ui.available_width() - 24.0;
                    ui.add_sized(
                        [w, 24.0],
                        egui::TextEdit::singleline(&mut d.checksum_actual.clone())
                            .font(egui::TextStyle::Monospace),
                    )
                    .labelled_by(by);
                    ui.label(RichText::new("✓").family(crate::fonts::bold()).color(p.ok));
                });
            }
        });
        kit::footer(ui, &p, |ui| {
            ui.checkbox(&mut self.dont_show, "Don't show this dialog again");
            kit::right(ui, |ui| {
                if kit::plain(ui, &p, "Close") {
                    action = Some("close");
                }
                if kit::plain(ui, &p, "Open folder") {
                    action = Some("folder");
                }
                if kit::plain(ui, &p, "Open with...") {
                    action = Some("with");
                }
                if kit::primary(ui, &p, "Open") {
                    action = Some("open");
                }
            });
        });
        if action.is_none() && (cx.enter(ui) || cx.escape(ui)) {
            action = Some("close");
        }
        match action {
            Some("open") => {
                let _ = os().open_path(&d.path());
                cx.close = true;
            }
            Some("folder") => {
                app.open_folder(self.id);
                cx.close = true;
            }
            Some("with") => {
                *cx.boxx = Some(MsgBox::input(
                    "Open with",
                    "Command (the file path is appended):",
                    Then::OpenWith(self.id),
                ));
            }
            Some(_) => cx.close = true,
            None => {}
        }
    }

    fn answer(&mut self, app: &mut App, a: Answer, cx: &mut Cx) {
        if a.button == "ok" && !a.value.trim().is_empty() {
            app.answer(a);
            cx.close = true;
        }
    }

    /// "Don't show this dialog again" applies when the window closes, whichever way (1.x `_complete_closed`).
    fn closed(&mut self, app: &mut App) {
        if self.dont_show {
            app.manager.settings.show_complete_dialog = false;
            app.save_settings();
            app.rebuild();
        }
    }
    any_body!();
}

// ---------------------------------------------------------------------- Checksum mismatch

pub struct Mismatch {
    pub id: i64,
}

pub fn mismatch(app: &mut App, id: i64) {
    app.effects.push(crate::app::Effect::Show);
    app.open_dialog(Dialog::new(
        &format!("mismatch-{id}"),
        "Checksum mismatch",
        (640.0, 420.0),
        Mismatch { id },
    ));
}

/// Download again, delete the file (to the Trash) and the entry, or keep it.
pub fn mismatch_choice(app: &mut App, id: i64, choice: &str) -> Result<(), String> {
    match choice {
        "again" => {
            app.manager.redownload(id, true, true);
            super::progress::open(app, id, false);
        }
        "delete" => {
            let path = app.manager.downloads.get(&id).map(|d| d.path());
            if let Some(p) = path
                && std::path::Path::new(&p).exists()
            {
                trash::delete(&p).map_err(|e| e.to_string())?;
            }
            app.manager.remove(&[id], false);
            app.checked.remove(&id);
        }
        _ => {}
    }
    app.rebuild();
    Ok(())
}

impl Body for Mismatch {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let Some(d) = app.manager.downloads.get(&self.id).cloned() else {
            cx.close = true;
            return;
        };
        let mut choice = None;
        kit::body(ui, |ui| {
            let subtitle = format!(
                "{} · {}. It may be corrupt or changed on the server.",
                d.filename,
                lider_core::fmt::size(d.size)
            );
            kit::header(
                ui,
                &p,
                &Head::new(
                    "shield-alert",
                    "The file doesn't match its checksum",
                    &subtitle,
                    Badge::Bad,
                ),
                |_| {},
            );
            kit::panel(ui, &p, |ui| {
                egui::Grid::new("mismatch-pair")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        kit::caption(ui, &p, "Expected");
                        ui.add(
                            egui::Label::new(
                                RichText::new(checksum::split(&d.checksum).1).monospace(),
                            )
                            .wrap(),
                        );
                        ui.end_row();
                        kit::caption(ui, &p, "Actual");
                        ui.add(
                            egui::Label::new(
                                RichText::new(&d.checksum_actual).monospace().color(p.bad),
                            )
                            .wrap(),
                        );
                        ui.end_row();
                    });
                kit::faint(
                    ui,
                    &p,
                    &format!(
                        "{}. Expected is the value you entered; Actual is computed from the downloaded file after its parts were merged.",
                        checksum::title(&d.checksum)
                    ),
                );
            });
            saved_as(ui, cx, &d);
        });
        kit::footer(ui, &p, |ui| {
            if kit::icon_button(ui, &p, "delete", "Delete file", Look::Danger) {
                choice = Some("delete");
            }
            kit::right(ui, |ui| {
                if kit::plain(ui, &p, "Keep file") {
                    choice = Some("keep");
                }
                if kit::primary(ui, &p, "Download again") {
                    choice = Some("again");
                }
            });
        });
        if choice.is_none() {
            if cx.enter(ui) {
                choice = Some("again");
            } else if cx.escape(ui) {
                choice = Some("keep");
            }
        }
        if let Some(c) = choice {
            match mismatch_choice(app, self.id, c) {
                Ok(()) => cx.close = true,
                Err(e) => *cx.boxx = Some(MsgBox::info("Checksum mismatch", &e)),
            }
        }
    }
    any_body!();
}
