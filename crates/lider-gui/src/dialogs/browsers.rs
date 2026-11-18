//! "Browser integration" (1.x `browsers.BrowserIntegrationDialog`): register the helper for the browsers,
//! then load the extension. Also the status bar's "Browser: …" text.

use std::collections::BTreeSet;

use egui::{RichText, Ui};
use lider_sys::browser::{self as sysb, Family};
use lider_sys::platform::BrowserState;
use lider_sys::platform::prelude::*;

use super::kit::{self, Badge, Head};
use super::{Body, Cx, Dialog};
use crate::any_body;
use crate::app::App;
use crate::boxes::MsgBox;

pub struct Browsers {
    pub rows: Vec<BrowserState>,
    pub checked: BTreeSet<String>,
    pub chromium_folder: String,
    pub firefox_folder: String,
}

pub fn open(app: &mut App) {
    let mut b = Browsers {
        rows: vec![],
        checked: BTreeSet::new(),
        chromium_folder: String::new(),
        firefox_folder: String::new(),
    };
    b.load();
    b.checked = b
        .rows
        .iter()
        .filter(|r| r.detected)
        .map(|r| r.name.to_string())
        .collect();
    app.open_dialog(Dialog::new(
        "browsers",
        "Browser integration",
        (680.0, 600.0),
        b,
    ));
}

/// The status bar's text: whether any detected browser has the helper (checked once, then cached in
/// `app.browser_ok` until Options or this window change it).
pub fn status_text(app: &mut App) -> String {
    let ok = *app
        .browser_ok
        .get_or_insert_with(|| os().browsers().iter().any(|b| b.detected && b.installed));
    format!("Browser: {}", if ok { "connected" } else { "not set up" })
}

impl Browsers {
    fn load(&mut self) {
        let sys = os();
        self.rows = sys.browsers();
        let folder = |f: Family| {
            sys.exported_extension(f)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        self.chromium_folder = folder(Family::Chromium);
        self.firefox_folder = folder(Family::Firefox);
    }

    /// Install or remove the helper for the checked browsers; installing exports the extension folders.
    fn apply(&mut self, install: bool) -> Result<(), String> {
        let sys = os();
        let chosen: Vec<&BrowserState> = self
            .rows
            .iter()
            .filter(|b| self.checked.contains(b.name))
            .collect();
        let names: Vec<&str> = chosen.iter().map(|b| b.name).collect();
        if install {
            sys.install(&names).map_err(|e| e.to_string())?;
            let mut families: Vec<Family> = chosen.iter().map(|b| b.family).collect();
            families.dedup();
            for f in families {
                sys.extension_folder(f).map_err(|e| e.to_string())?;
            }
        } else {
            sys.uninstall(&names);
        }
        Ok(())
    }

    fn folder_row(ui: &mut Ui, p: &crate::theme::Palette, folder: &str, by: egui::Id) {
        ui.horizontal(|ui| {
            let w = ui.available_width() - 190.0;
            let mut t = folder.to_string();
            ui.add(
                egui::TextEdit::singleline(&mut t)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("Install for a browser first")
                    .desired_width(w.max(100.0)),
            )
            .labelled_by(by);
            let any = !folder.is_empty();
            if kit::button(ui, p, "Copy", kit::Look::Normal, any) {
                ui.ctx().copy_text(folder.to_string());
            }
            if kit::button(ui, p, "Open folder", kit::Look::Normal, any) {
                let _ = os().open_path(folder);
            }
        });
    }
}

impl Body for Browsers {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let mut install = None;
        let mut done = false;
        kit::body(ui, |ui| {
            kit::header(
                ui,
                &p,
                &Head::new(
                    "connection",
                    "Browser integration",
                    "LID talks to its extension through native messaging — no network port is opened.",
                    Badge::None,
                ),
                |_| {},
            );
            kit::panel(ui, &p, |ui| {
                let enabled: Vec<String> = self
                    .rows
                    .iter()
                    .filter(|r| r.detected)
                    .map(|r| r.name.to_string())
                    .collect();
                let all_on =
                    !enabled.is_empty() && enabled.iter().all(|n| self.checked.contains(n));
                egui::Grid::new("browsers")
                    .num_columns(2)
                    .min_col_width(280.0)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        let mut all = all_on;
                        let head = RichText::new("BROWSER").small().strong().color(p.mu);
                        if ui.checkbox(&mut all, head).changed() {
                            self.checked = if all_on {
                                BTreeSet::new()
                            } else {
                                enabled.iter().cloned().collect()
                            };
                        }
                        ui.label(RichText::new("LID HELPER").small().strong().color(p.mu));
                        ui.end_row();
                        for r in &self.rows {
                            let mut on = self.checked.contains(r.name);
                            let colour = if r.detected { p.tx } else { p.fa };
                            let c = ui.add_enabled(
                                r.detected,
                                egui::Checkbox::new(&mut on, RichText::new(r.name).color(colour)),
                            );
                            if c.changed() {
                                if on {
                                    self.checked.insert(r.name.to_string());
                                } else {
                                    self.checked.remove(r.name);
                                }
                            }
                            if r.installed {
                                kit::pill(ui, &p, "Installed", "ok");
                            } else if r.detected {
                                kit::pill(ui, &p, "Not installed", "muted");
                            } else {
                                ui.label(RichText::new("browser not found").color(p.mu));
                            }
                            ui.end_row();
                        }
                    });
            });
            ui.horizontal(|ui| {
                if kit::primary(ui, &p, "Install for checked browsers") {
                    install = Some(true);
                }
                if kit::plain(ui, &p, "Remove") {
                    install = Some(false);
                }
            });
            kit::panel(ui, &p, |ui| {
                kit::caption(ui, &p, "Then add the LID extension to the browser");
                let by = ui
                    .add(
                        egui::Label::new(
                            "Chrome, Chromium, Brave, Edge, Vivaldi, Opera: open chrome://extensions, turn on Developer mode, click Load unpacked and choose the folder below.",
                        )
                        .wrap(),
                    )
                    .id;
                Self::folder_row(ui, &p, &self.chromium_folder, by);
                let by = ui
                    .add(
                        egui::Label::new(
                            "Firefox: open about:debugging#/runtime/this-firefox, click Load Temporary Add-on and choose manifest.json in the folder below. Then allow it to access all websites in the add-on's permissions.",
                        )
                        .wrap(),
                    )
                    .id;
                Self::folder_row(ui, &p, &self.firefox_folder, by);
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Get the extension:");
                let mut links: Vec<(&str, &str)> = vec![];
                if !sysb::CHROME_WEB_STORE_URL.is_empty() {
                    links.push(("Chrome Web Store", sysb::CHROME_WEB_STORE_URL));
                }
                links.push(if !sysb::FIREFOX_ADDONS_URL.is_empty() {
                    ("Firefox Add-ons", sysb::FIREFOX_ADDONS_URL)
                } else if sysb::FIREFOX_SIGNED_XPI {
                    ("Firefox (.xpi)", sysb::FIREFOX_XPI_URL)
                } else {
                    ("Firefox (.zip)", sysb::FIREFOX_ZIP_URL)
                });
                links.push(("All downloads", sysb::RELEASES_URL));
                for (label, url) in links {
                    if ui.link(label).on_hover_text(url).clicked() {
                        let _ = os().open_path(url);
                    }
                }
            });
        });
        kit::footer(ui, &p, |ui| {
            kit::right(ui, |ui| {
                done = kit::primary(ui, &p, "Close");
            });
        });
        if let Some(i) = install {
            if let Err(e) = self.apply(i) {
                *cx.boxx = Some(MsgBox::info("Browser integration", &e));
            }
            self.load();
            app.browser_ok = None;
        }
        if done || cx.enter(ui) || cx.escape(ui) {
            app.browser_ok = None;
            cx.close = true;
        }
    }
    any_body!();
}
