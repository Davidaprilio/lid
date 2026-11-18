//! "LID options" with its seven tabs (1.x `options.OptionsDialog`): a working copy of the settings; OK
//! writes the keys this window edits, then the desktop entries (autostart, the application menu).

use egui::{RichText, Ui};
use lider_core::settings::{CONNECTION_CHOICES, Category, DEFAULT_FILE_TYPES, Settings};
use lider_sys::platform::prelude::*;

use super::kit::{self, TagInput};
use super::small::{self, CategoryForm};
use super::{Body, Cx, Dialog};
use crate::any_body;
use crate::app::App;
use crate::icons::icon;
use crate::theme::{Palette, ThemeName};

pub const TABS: [&str; 7] = [
    "General",
    "File types",
    "Save to",
    "Downloads",
    "Connection",
    "Proxy / Socks",
    "Appearance",
];

const HEAD: [(&str, &str); 7] = [
    ("general", "Desktop Integration"),
    ("filetypes", "Downloaded file types"),
    ("saveto", "Categories, file types, folders"),
    ("folder-download", "Default download settings"),
    ("connection", "Connections and Limits"),
    ("proxy", "Proxy"),
    ("general", "Appearance"),
];

/// The desktop as the window found it when it opened.
pub struct DesktopState {
    pub autostart: bool,
    pub in_menu: bool,
    pub system_installed: bool,
    pub browsers_connected: usize,
    pub global_menu: bool,
}

/// The "Max. connections number for a server" editor: which row (None = new), pattern, connections.
pub struct LimitEdit {
    pub index: Option<usize>,
    pub pattern: String,
    pub n: String,
}

pub struct Options {
    pub tab: usize,
    /// The working copy; OK writes the keys this window edits.
    pub s: Settings,
    pub desk: DesktopState,
    pub launch: bool,
    pub in_menu: bool,
    pub file_types: TagInput,
    pub no_sites: TagInput,
    pub cats: Vec<Category>,
    pub cat_index: usize,
    pub cat_exts: TagInput,
    pub cat_dir: String,
    pub limits: Vec<(String, i64)>,
    pub limit_sel: Option<usize>,
    pub use_proxy: bool,
    pub proxy: String,
    /// The category form over the window (None: New, Some(i): Edit).
    pub edit_cat: Option<(CategoryForm, Option<usize>)>,
    pub edit_limit: Option<LimitEdit>,
    /// Updates (lider_core::update): kept in updates.json, written on OK like the rest.
    pub updates: lider_core::update::Prefs,
}

/// Open Options on a tab ("" for the first, "Appearance" from View → Appearance...).
pub fn open(app: &mut App, tab: &str) {
    if app.dialog_open("options") {
        if let Some(o) = app.dialog_body::<Options>("options")
            && let Some(i) = TABS.iter().position(|t| *t == tab)
        {
            o.tab = i;
        }
        app.open_dialog(Dialog::new(
            "options",
            "LID options",
            (720.0, 712.0),
            Closed,
        ));
        return;
    }
    let sys = os();
    let desk = DesktopState {
        autostart: sys.autostart_enabled(),
        in_menu: sys.in_menu(),
        system_installed: sys.system_installed(),
        browsers_connected: sys.browsers().iter().filter(|b| b.installed).count(),
        global_menu: sys.global_menu_available(),
    };
    let mut body = Options::new(&app.manager.settings, desk, tab);
    body.updates = lider_core::update::load(&app.dirs);
    app.open_dialog(Dialog::new("options", "LID options", (720.0, 712.0), body));
}

/// A placeholder body: only used to bring an open window forward.
struct Closed;
impl Body for Closed {
    fn ui(&mut self, _app: &mut App, _ui: &mut Ui, cx: &mut Cx) {
        cx.close = true;
    }
    any_body!();
}

fn words(s: &str, lower: bool) -> Vec<String> {
    s.split_whitespace()
        .map(|x| {
            if lower {
                x.to_lowercase()
            } else {
                x.to_string()
            }
        })
        .collect()
}

fn cat_icon(name: &str) -> String {
    let l = name.to_lowercase();
    if ["compressed", "documents", "music", "programs", "video"].contains(&l.as_str()) {
        format!("category-{l}")
    } else {
        "folder".into()
    }
}

impl Options {
    pub fn new(s: &Settings, desk: DesktopState, tab: &str) -> Options {
        let mut limits: Vec<(String, i64)> = s
            .server_limits
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        limits.sort_by(|a, b| a.0.cmp(&b.0));
        let cats = s.categories.clone();
        let mut o = Options {
            tab: TABS.iter().position(|t| *t == tab).unwrap_or(0),
            s: s.clone(),
            launch: desk.autostart,
            in_menu: desk.system_installed || desk.in_menu,
            desk,
            file_types: TagInput::new(words(&s.auto_capture_types, true), true),
            no_sites: TagInput::new(words(&s.no_auto_sites, false), false),
            cat_exts: TagInput::new(vec![], true),
            cat_dir: String::new(),
            cats,
            cat_index: 0,
            limits,
            limit_sel: None,
            use_proxy: !s.proxy.is_empty(),
            proxy: if s.proxy.is_empty() {
                "http://127.0.0.1:8080".into()
            } else {
                s.proxy.clone()
            },
            edit_cat: None,
            edit_limit: None,
            updates: lider_core::update::Prefs::default(),
        };
        o.select_cat(0);
        o
    }

    /// Save to: switching category keeps the edits of the previous one.
    fn store_cat(&mut self) {
        self.cat_exts.commit();
        if let Some(c) = self.cats.get_mut(self.cat_index) {
            c.extensions = self.cat_exts.values.clone();
            if !self.cat_dir.trim().is_empty() {
                c.save_dir = self.cat_dir.trim().to_string();
            }
        }
    }

    fn select_cat(&mut self, i: usize) {
        self.store_cat();
        self.cat_index = i;
        if let Some(c) = self.cats.get(i) {
            self.cat_exts = TagInput::new(c.extensions.clone(), true);
            self.cat_dir = c.save_dir.clone();
        }
    }

    fn saved_cat(&mut self, c: Category, index: Option<usize>) {
        match index {
            None => {
                if self.cats.iter().any(|x| x.name == c.name) {
                    return;
                }
                self.cats.push(c);
                let last = self.cats.len() - 1;
                self.select_cat(last);
            }
            Some(i) => {
                self.cat_exts = TagInput::new(c.extensions.clone(), true);
                self.cat_dir = c.save_dir.clone();
                self.cats[i] = c;
            }
        }
    }

    fn save_limit(&mut self) {
        let Some(e) = self.edit_limit.take() else {
            return;
        };
        if let Some(i) = e.index {
            self.limits.remove(i);
        }
        if !e.pattern.trim().is_empty() {
            let n = e.n.parse().unwrap_or(2);
            self.limits.push((e.pattern.trim().to_string(), n));
        }
        self.limits.sort_by(|a, b| a.0.cmp(&b.0));
        self.limit_sel = None;
    }

    /// The settings as OK writes them: only the keys this window edits change (sorting or column
    /// widths may have changed meanwhile).
    pub fn settings(&mut self, current: &Settings) -> Settings {
        self.store_cat();
        self.file_types.commit();
        self.no_sites.commit();
        let w = &self.s;
        let mut n = current.clone();
        n.launch_on_startup = self.launch;
        n.monitor_clipboard = w.monitor_clipboard;
        n.browser_capture = w.browser_capture;
        n.auto_capture_types = self.file_types.values.join(" ");
        n.no_auto_sites = self.no_sites.values.join(" ");
        n.categories = self.cats.clone();
        if !w.temp_dir.trim().is_empty() {
            n.temp_dir = w.temp_dir.trim().to_string();
        }
        n.show_progress_dialog = w.show_progress_dialog;
        n.show_start_dialog = w.show_start_dialog;
        n.only_add_to_queue = w.only_add_to_queue;
        n.show_complete_dialog = w.show_complete_dialog;
        n.duplicate_action = w.duplicate_action.clone();
        if !w.user_agent.trim().is_empty() {
            n.user_agent = w.user_agent.clone();
        }
        n.verify_checksum = w.verify_checksum;
        n.checksum_from_clipboard = w.checksum_from_clipboard;
        n.checksum_mismatch = w.checksum_mismatch.clone();
        n.max_connections = w.max_connections;
        n.server_limits = self.limits.iter().cloned().collect();
        n.retries = w.retries.clamp(0, 100);
        n.proxy = if self.use_proxy {
            self.proxy.trim().to_string()
        } else {
            String::new()
        };
        n.theme = w.theme.clone();
        n.follow_system = w.follow_system;
        n.dark_theme = w.dark_theme.clone();
        n.density = w.density.clone();
        n.toolbar_labels = w.toolbar_labels;
        n.menubar = w.menubar.clone();
        n.list_font = w.list_font.clone();
        n.show_categories = w.show_categories;
        n.show_details = w.show_details;
        n.show_statusbar = w.show_statusbar;
        n.tray_icon = w.tray_icon;
        n.close_to_tray = w.close_to_tray;
        n.start_minimized = w.start_minimized;
        n
    }

    fn font_label(&self) -> String {
        match self.s.list_font_family_and_size() {
            Some((f, size)) => format!("{f} · {size} pt"),
            None => match self
                .s
                .list_font
                .split(',')
                .next()
                .filter(|f| !f.trim().is_empty())
            {
                Some(f) => format!("{} · 10 pt", f.trim()),
                None => "System default · 10 pt".into(),
            },
        }
    }
}

/// OK: write the settings and apply them everywhere, then the desktop entries.
pub fn apply(app: &mut App, o: &mut Options) {
    let next = o.settings(&app.manager.settings);
    app.manager.settings = next;
    let on = app.manager.settings.speed_limiter_on;
    app.manager.set_speed_limiter(on, None);
    app.save_settings();
    if let Err(e) = lider_core::update::save(&app.dirs, &o.updates) {
        app.info("LID", &e.to_string());
    }
    app.apply_theme();
    app.browser_ok = None;
    // The test harness never writes desktop entries into the real home.
    if !cfg!(test) {
        let sys = os();
        if o.launch != sys.autostart_enabled() {
            // 1.x ignores an error here: OK still closes the window.
            let _ = sys.set_autostart(o.launch);
        }
        if !o.desk.system_installed {
            let in_menu = sys.in_menu();
            if o.in_menu && !in_menu {
                let _ = sys.add_to_menu();
            } else if !o.in_menu && in_menu {
                sys.remove_from_menu();
            }
        }
    }
    app.rebuild();
}

impl Body for Options {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        if let Some(f) = app.font_chosen.take() {
            self.s.list_font = f;
        }
        // The category form and the server editor float over the window; its own keys wait meanwhile.
        let over = self.edit_cat.is_some() || self.edit_limit.is_some();
        let blocked = cx.blocked;
        cx.blocked |= over;
        let mut ok = false;
        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 18,
                right: 18,
                top: 10,
                bottom: 0,
            })
            .show(ui, |ui| {
                kit::tab_strip(ui, &p, &TABS, &mut self.tab);
            });
        kit::body(ui, |ui| {
            let (ic, title) = HEAD[self.tab];
            ui.horizontal(|ui| {
                ui.add(icon(ic, 22.0, &p, p.tx));
                ui.label(RichText::new(title).size(15.0).strong());
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .min_scrolled_height(520.0)
                .max_height(520.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.set_min_height(520.0);
                    match self.tab {
                        0 => self.general(app, ui, &p),
                        1 => self.file_types(ui, &p),
                        2 => self.save_to(ui, &p),
                        3 => self.downloads(ui, &p),
                        4 => self.connection(ui, &p),
                        5 => self.proxy_tab(ui, &p),
                        _ => self.appearance(app, ui, &p),
                    }
                });
        });
        kit::footer(ui, &p, |ui| {
            kit::right(ui, |ui| {
                ok |= kit::primary(ui, &p, "OK");
                if kit::plain(ui, &p, "Cancel") {
                    cx.close = true;
                }
            });
        });
        if ok && !over {
            apply(app, self);
            cx.close = true;
        }
        if cx.escape(ui) {
            cx.close = true;
        }
        cx.blocked = blocked;
        if let Some((form, index)) = &mut self.edit_cat {
            let index = *index;
            match small::form_over(ui, cx, form) {
                Some(Some(c)) => {
                    self.edit_cat = None;
                    self.saved_cat(c, index);
                }
                Some(None) => self.edit_cat = None,
                None => {}
            }
        }
        if self.edit_limit.is_some() {
            let p = cx.p;
            egui::Modal::new(ui.scope_id().with("server-limit"))
                .frame(
                    egui::Frame::new()
                        .fill(p.win)
                        .stroke(egui::Stroke::new(1.0, p.line2))
                        .corner_radius(10),
                )
                .show(ui.ctx(), |ui| {
                    ui.set_width(480.0);
                    self.limit_editor(ui, cx);
                });
        }
    }
    any_body!();
}

impl Options {
    fn general(&mut self, app: &mut App, ui: &mut Ui, p: &Palette) {
        ui.checkbox(&mut self.launch, "Launch LID on startup");
        let menu = ui.add_enabled(
            !self.desk.system_installed,
            egui::Checkbox::new(&mut self.in_menu, "Show LID in the application menu"),
        );
        if self.desk.system_installed {
            menu.on_disabled_hover_text("Added by the installed LID package");
        }
        ui.checkbox(
            &mut self.s.monitor_clipboard,
            "Automatically start downloading of URLs placed to clipboard",
        );
        kit::panel(ui, p, |ui| {
            kit::caption(ui, p, "Browser integration");
            ui.checkbox(
                &mut self.s.browser_capture,
                "Capture downloads from browsers (needs the LID extension)",
            );
            kit::faint(
                ui,
                p,
                "The extension takes over downloads of the file types on the File types tab, and adds “Download with LID”, “Download all links with LID” and a YouTube button.",
            );
            ui.horizontal(|ui| {
                if kit::plain(ui, p, "Set up browsers...") {
                    super::browsers::open(app);
                }
                let n = self.desk.browsers_connected;
                if n > 0 {
                    let s = if n == 1 { "" } else { "s" };
                    kit::pill(ui, p, &format!("✓ {n} browser{s} connected"), "ok");
                } else {
                    kit::pill(ui, p, "Not set up yet", "muted");
                }
            });
        });
        kit::panel(ui, p, |ui| {
            kit::caption(ui, p, "Updates");
            ui.checkbox(
                &mut self.updates.check_at_startup,
                "Check for a new version when LID starts",
            );
            kit::row(ui, "Versions", 80.0, |ui, by| {
                let choices = [
                    ("stable".to_string(), "Stable releases".to_string()),
                    (
                        "prerelease".to_string(),
                        "Pre-releases too (beta, alpha)".to_string(),
                    ),
                ];
                kit::combo_by(
                    ui,
                    "update-channel",
                    240.0,
                    &mut self.updates.channel,
                    &choices,
                    by,
                );
                if kit::plain(ui, p, "Check now") {
                    let _ = lider_core::update::save(&app.dirs, &self.updates);
                    app.update_note = "Checking…".into();
                    app.check_for_update(false);
                }
            });
            if !app.update_note.is_empty() {
                kit::faint(ui, p, &app.update_note);
            }
        });
    }

    fn file_types(&mut self, ui: &mut Ui, p: &Palette) {
        let by = ui
            .label("Automatically start downloading the following file types:")
            .id;
        self.file_types
            .ui(ui, p, "Add a file type…", "types", true, by);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if ui
                .button("Default")
                .on_hover_text("Back to the default file types")
                .clicked()
            {
                self.file_types.values = words(DEFAULT_FILE_TYPES, true);
            }
        });
        let by = ui
            .label("Don't start downloading automatically from the following sites:")
            .id;
        self.no_sites
            .ui(ui, p, "Add a site, e.g. *.example.com", "sites", true, by);
    }

    fn save_to(&mut self, ui: &mut Ui, p: &Palette) {
        let mut pick = None;
        let mut new = false;
        let mut edit = false;
        kit::panel(ui, p, |ui| {
            kit::caption(ui, p, "Save to...");
            kit::row(ui, "Category", 80.0, |ui, by| {
                let name = self
                    .cats
                    .get(self.cat_index)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                egui::ComboBox::from_id_salt("options-category")
                    .width(200.0)
                    .selected_text(name)
                    .show_ui(ui, |ui| {
                        for (i, c) in self.cats.iter().enumerate() {
                            let b = egui::Button::selectable(
                                i == self.cat_index,
                                (icon(&cat_icon(&c.name), 16.0, p, p.tx), c.name.as_str()),
                            );
                            if ui.add(b).clicked() {
                                pick = Some(i);
                            }
                        }
                    })
                    .response
                    .labelled_by(by);
                new = kit::plain(ui, p, "New");
                edit = kit::plain(ui, p, "Edit...");
            });
            let name = self
                .cats
                .get(self.cat_index)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            let by = ui
                .label(format!(
                    "Automatically put in “{name}” the following file types:"
                ))
                .id;
            self.cat_exts.ui(ui, p, "Add a type…", "types", true, by);
            let by = ui
                .label(format!("Default download directory for “{name}”"))
                .id;
            kit::folder_field(ui, p, &mut self.cat_dir, by);
        });
        kit::panel(ui, p, |ui| {
            let by = kit::caption(ui, p, "Temporary directory");
            kit::folder_field(ui, p, &mut self.s.temp_dir, by);
            kit::faint(
                ui,
                p,
                "Parts are stored here while downloading. The finished file moves to its category folder.",
            );
        });
        if let Some(i) = pick {
            self.select_cat(i);
        }
        if new {
            self.store_cat();
            self.edit_cat = Some((CategoryForm::new(None), None));
        } else if edit {
            self.store_cat();
            let i = self.cat_index;
            self.edit_cat = Some((CategoryForm::new(self.cats.get(i)), Some(i)));
        }
    }

    fn downloads(&mut self, ui: &mut Ui, p: &Palette) {
        let s = &mut self.s;
        ui.checkbox(
            &mut s.show_progress_dialog,
            "Show \"Download progress\" dialog",
        );
        ui.checkbox(&mut s.show_start_dialog, "Show start download dialog");
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.add_enabled(
                s.show_start_dialog,
                egui::Checkbox::new(
                    &mut s.only_add_to_queue,
                    "Do not start downloading, only add files to the queue",
                ),
            );
        });
        ui.checkbox(&mut s.show_complete_dialog, "Show download complete dialog");
        kit::row(
            ui,
            "If a duplicate download link is added",
            250.0,
            |ui, by| {
                let choices: Vec<(String, String)> = [
                ("ask", "Show the dialog and ask me"),
                ("number", "Add the duplicate with a numbered file name"),
                ("overwrite", "Add the duplicate and overwrite the existing file"),
                (
                    "complete",
                    "If existing file is complete, show download complete dialog; otherwise resume it",
                ),
            ]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
                kit::combo_by(
                    ui,
                    "options-duplicate",
                    380.0,
                    &mut s.duplicate_action,
                    &choices,
                    by,
                );
            },
        );
        kit::row(ui, "User-Agent for manual downloads", 250.0, |ui, by| {
            kit::mono_field(ui, &mut s.user_agent, by);
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add(icon("checksum", 18.0, p, p.tx));
            ui.label(RichText::new("Checksum verification").strong());
        });
        ui.separator();
        ui.checkbox(
            &mut s.verify_checksum,
            "Verify the checksum when a download finishes (when one was entered)",
        );
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            kit::faint(
                ui,
                p,
                "Reads the whole file once, after its parts are merged. Large files take a few seconds; the list shows “Verifying”.",
            );
        });
        ui.checkbox(
            &mut s.checksum_from_clipboard,
            "Fill in the checksum from the clipboard when it holds a hash",
        );
        kit::row(ui, "If the checksum doesn't match", 250.0, |ui, by| {
            let choices: Vec<(String, String)> = [
                ("ask", "Ask me — show the mismatch dialog"),
                ("keep", "Keep the file, mark it “Checksum mismatch”"),
                ("redownload", "Delete the file and download it again"),
            ]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
            kit::combo_by(
                ui,
                "options-mismatch",
                300.0,
                &mut s.checksum_mismatch,
                &choices,
                by,
            );
        });
    }

    fn connection(&mut self, ui: &mut Ui, p: &Palette) {
        let choices: Vec<(String, String)> = CONNECTION_CHOICES
            .iter()
            .map(|c| (c.to_string(), c.to_string()))
            .collect();
        let mut new = false;
        let mut delete = false;
        let mut edit = false;
        kit::panel(ui, p, |ui| {
            kit::caption(ui, p, "Max. connections number");
            kit::row(ui, "Default max. conn. number", 190.0, |ui, by| {
                let mut v = self.s.max_connections.to_string();
                if kit::combo_by(ui, "options-maxconn", 90.0, &mut v, &choices, by) {
                    self.s.max_connections = v.parse().unwrap_or(8);
                }
            });
            ui.label("Exceptions");
            ui.horizontal_top(|ui| {
                let w = ui.available_width() - 100.0;
                ui.vertical(|ui| {
                    ui.set_width(w);
                    egui::Frame::new()
                        .fill(p.pane)
                        .stroke(egui::Stroke::new(1.0, p.line))
                        .corner_radius(6)
                        .inner_margin(6)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.set_min_height(150.0);
                            egui::Grid::new("server-limits")
                                .num_columns(2)
                                .min_col_width(200.0)
                                .show(ui, |ui| {
                                    ui.label(RichText::new("SERVER").small().color(p.mu));
                                    ui.label(RichText::new("MAX. CONNECTIONS").small().color(p.mu));
                                    ui.end_row();
                                    for (i, (pat, n)) in self.limits.iter().enumerate() {
                                        let sel = self.limit_sel == Some(i);
                                        if ui
                                            .selectable_label(sel, RichText::new(pat).monospace())
                                            .clicked()
                                        {
                                            self.limit_sel = Some(i);
                                        }
                                        ui.label(RichText::new(n.to_string()).monospace());
                                        ui.end_row();
                                    }
                                });
                        });
                });
                ui.vertical(|ui| {
                    new = kit::plain(ui, p, "New");
                    let any = self.limit_sel.is_some();
                    delete = kit::button(ui, p, "Delete", kit::Look::Normal, any);
                    edit = kit::button(ui, p, "Edit", kit::Look::Normal, any);
                });
            });
        });
        kit::row(
            ui,
            "Reconnection attempts for each connection",
            280.0,
            |ui, by| {
                ui.add(egui::DragValue::new(&mut self.s.retries).range(0..=100))
                    .labelled_by(by);
            },
        );
        if new {
            self.edit_limit = Some(LimitEdit {
                index: None,
                pattern: String::new(),
                n: "2".into(),
            });
        } else if delete && let Some(i) = self.limit_sel.take() {
            self.limits.remove(i);
        } else if edit && let Some(i) = self.limit_sel {
            let (pat, n) = self.limits[i].clone();
            self.edit_limit = Some(LimitEdit {
                index: Some(i),
                pattern: pat,
                n: n.to_string(),
            });
        }
    }

    fn limit_editor(&mut self, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let Some(e) = &mut self.edit_limit else {
            return;
        };
        let mut ok = false;
        let mut cancel = false;
        kit::body(ui, |ui| {
            ui.label(
                RichText::new("Max. connections number for a server")
                    .size(15.0)
                    .strong(),
            );
            kit::row(ui, "Server", 170.0, |ui, by| {
                kit::mono_field(ui, &mut e.pattern, by);
            });
            ui.horizontal(|ui| {
                ui.add_space(178.0);
                kit::faint(ui, &p, "You may use an asterisk as a wildcard.");
            });
            kit::row(ui, "Max. connections number", 170.0, |ui, by| {
                let choices: Vec<(String, String)> = CONNECTION_CHOICES
                    .iter()
                    .map(|c| (c.to_string(), c.to_string()))
                    .collect();
                kit::combo_by(ui, "limit-n", 90.0, &mut e.n, &choices, by);
            });
        });
        kit::footer(ui, &p, |ui| {
            kit::right(ui, |ui| {
                ok = kit::primary(ui, &p, "OK");
                cancel = kit::plain(ui, &p, "Cancel");
            });
        });
        if ok || cx.enter(ui) {
            self.save_limit();
        } else if cancel || cx.escape(ui) {
            self.edit_limit = None;
        }
    }

    fn proxy_tab(&mut self, ui: &mut Ui, p: &Palette) {
        ui.checkbox(&mut self.use_proxy, "Use proxy");
        kit::row(ui, "Proxy address", 110.0, |ui, by| {
            ui.add_enabled(
                self.use_proxy,
                egui::TextEdit::singleline(&mut self.proxy)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY),
            )
            .labelled_by(by);
        });
        kit::faint(
            ui,
            p,
            "Format: http://[user:password@]host:port — used for HTTP and HTTPS.",
        );
    }

    fn appearance(&mut self, app: &mut App, ui: &mut Ui, p: &Palette) {
        kit::caption(ui, p, "Theme");
        ui.horizontal(|ui| {
            for (t, title) in [
                ("ember", "Ember"),
                ("ember-dark", "Ember Dark"),
                ("dark", "Dark"),
            ] {
                ui.vertical(|ui| {
                    let c = Palette::of(ThemeName::parse(t));
                    let selected = self.s.theme == t;
                    let (r, resp) =
                        ui.allocate_exact_size(egui::vec2(190.0, 78.0), egui::Sense::click());
                    theme_preview(ui, r, &c, selected, p);
                    if resp.clicked() {
                        self.s.theme = t.into();
                    }
                    ui.radio_value(&mut self.s.theme, t.to_string(), title);
                });
            }
        });
        ui.horizontal(|ui| {
            ui.checkbox(
                &mut self.s.follow_system,
                "Follow the system light / dark setting",
            );
            let by = ui.label(RichText::new("When dark, use").color(p.mu)).id;
            ui.add_enabled_ui(self.s.follow_system, |ui| {
                let choices = vec![
                    ("ember-dark".to_string(), "Ember Dark".to_string()),
                    ("dark".to_string(), "Dark".to_string()),
                ];
                kit::combo_by(
                    ui,
                    "options-dark",
                    120.0,
                    &mut self.s.dark_theme,
                    &choices,
                    by,
                );
            });
        });
        kit::row(ui, "List density", 110.0, |ui, _| {
            kit::segmented(
                ui,
                p,
                &mut self.s.density,
                &[
                    ("comfortable".to_string(), "Comfortable"),
                    ("compact".to_string(), "Compact"),
                ],
            );
        });
        kit::row(ui, "Toolbar", 110.0, |ui, _| {
            kit::segmented(
                ui,
                p,
                &mut self.s.toolbar_labels,
                &[(true, "Icons and labels"), (false, "Icons only")],
            );
        });
        kit::row(ui, "Menu bar", 110.0, |ui, _| {
            kit::segmented(
                ui,
                p,
                &mut self.s.menubar,
                &[
                    ("native".to_string(), "Native"),
                    ("custom".to_string(), "In window"),
                    ("both".to_string(), "Both"),
                ],
            );
            kit::faint(
                ui,
                p,
                if self.desk.global_menu {
                    "Native = desktop global menu"
                } else {
                    "No global menu here: Native shows it in the window"
                },
            );
        });
        let label = self.font_label();
        kit::row(ui, "List font", 110.0, |ui, by| {
            let mut t = label.clone();
            ui.add(egui::TextEdit::singleline(&mut t).desired_width(220.0))
                .labelled_by(by);
            if kit::plain(ui, p, "Change...") {
                small::font(app, Some(self.s.list_font.clone()));
            }
            if ui.button("Reset").clicked() {
                self.s.list_font.clear();
            }
        });
        egui::Grid::new("options-view")
            .num_columns(2)
            .spacing([18.0, 8.0])
            .show(ui, |ui| {
                let s = &mut self.s;
                ui.checkbox(&mut s.show_categories, "Show Categories panel");
                ui.checkbox(&mut s.show_details, "Show details pane (F9)");
                ui.end_row();
                ui.checkbox(&mut s.show_statusbar, "Show status bar");
                ui.checkbox(&mut s.tray_icon, "Show icon in system tray");
                ui.end_row();
                ui.checkbox(&mut s.close_to_tray, "Close button hides to tray");
                ui.checkbox(&mut s.start_minimized, "Start minimized to tray");
                ui.end_row();
            });
    }
}

/// A small picture of a theme: toolbar, sidebar and list rows in its colours.
fn theme_preview(ui: &mut Ui, r: egui::Rect, c: &Palette, selected: bool, p: &Palette) {
    let painter = ui.painter();
    painter.rect_filled(r, 7.0, c.win);
    let stroke = if selected {
        egui::Stroke::new(2.0, p.ac)
    } else {
        egui::Stroke::new(1.0, c.line)
    };
    let bar = egui::Rect::from_min_size(r.min, egui::vec2(r.width(), 16.0));
    painter.hline(bar.x_range(), bar.bottom(), egui::Stroke::new(1.0, c.line));
    painter.rect_filled(
        egui::Rect::from_min_size(bar.min + egui::vec2(6.0, 4.0), egui::vec2(22.0, 8.0)),
        4.0,
        c.ac,
    );
    painter.rect_filled(
        egui::Rect::from_min_size(bar.min + egui::vec2(32.0, 5.0), egui::vec2(12.0, 6.0)),
        3.0,
        c.track,
    );
    let side = r.width() * 0.24;
    let pane = egui::Rect::from_min_max(
        egui::pos2(r.left() + side, bar.bottom() + 1.0),
        r.max - egui::vec2(1.0, 1.0),
    );
    painter.rect_filled(pane, 0.0, c.pane);
    painter.vline(pane.left(), pane.y_range(), egui::Stroke::new(1.0, c.line));
    for (i, w) in [0.7, 0.45, 0.85].iter().enumerate() {
        let row = egui::Rect::from_min_size(
            pane.min + egui::vec2(6.0, 6.0 + i as f32 * 11.0),
            egui::vec2((pane.width() - 12.0) * w, 6.0),
        );
        painter.rect_filled(row, 3.0, c.track);
    }
    painter.rect_stroke(r, 7.0, stroke, egui::StrokeKind::Inside);
}
