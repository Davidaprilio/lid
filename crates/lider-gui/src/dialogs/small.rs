//! The small settings dialogs: Speed Limiter settings, Columns, Choose Font, Find file / LID Search
//! settings, and the category editor (1.x `options.py`, `find.py`).

use egui::{RichText, Ui};
use lider_core::search::flags;
use lider_core::settings::{Category, ColumnState, DEFAULT_COLUMNS};
use lider_sys::platform::prelude::*;

use super::kit::{self, Badge, Head};
use super::{Body, Cx, Dialog};
use crate::any_body;
use crate::app::App;
use crate::boxes::MsgBox;

// ---------------------------------------------------------------------- Speed Limiter settings

pub struct Limiter {
    pub kbps: i64,
    pub on_startup: bool,
}

pub fn limiter(app: &mut App) {
    let s = &app.manager.settings;
    let body = Limiter {
        kbps: s.speed_limit_kbps,
        on_startup: s.speed_limiter_on_startup,
    };
    app.open_dialog(Dialog::new(
        "limiter",
        "Speed Limiter settings",
        (520.0, 300.0),
        body,
    ));
}

impl Body for Limiter {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let mut ok = false;
        kit::body(ui, |ui| {
            kit::header(
                ui,
                &p,
                &Head::new(
                    "speed",
                    "Speed Limiter settings",
                    "Applies to all downloads while the limiter is on",
                    Badge::Accent,
                ),
                |_| {},
            );
            kit::row(ui, "Maximum speed", 110.0, |ui, by| {
                ui.add(
                    egui::DragValue::new(&mut self.kbps)
                        .range(1..=10_000_000)
                        .speed(5.0),
                )
                .labelled_by(by);
                ui.label("KBytes/sec");
            });
            kit::note(
                ui,
                &p,
                "Note: servers may close the connection when the speed is very low.",
            );
            ui.checkbox(
                &mut self.on_startup,
                "Always turn on Speed Limiter on LID startup",
            );
        });
        kit::footer(ui, &p, |ui| {
            kit::right(ui, |ui| {
                ok |= kit::primary(ui, &p, "OK");
                if kit::plain(ui, &p, "Cancel") {
                    cx.close = true;
                }
            });
        });
        if ok || cx.enter(ui) {
            let m = &mut app.manager;
            m.settings.speed_limit_kbps = self.kbps.clamp(1, 10_000_000);
            m.settings.speed_limiter_on_startup = self.on_startup;
            let on = m.settings.speed_limiter_on;
            m.set_speed_limiter(on, None);
            app.save_settings();
            app.rebuild();
            cx.close = true;
        }
        if cx.escape(ui) {
            cx.close = true;
        }
    }
    any_body!();
}

// ---------------------------------------------------------------------- Columns

pub struct Columns {
    pub cols: Vec<ColumnState>,
    pub current: usize,
}

pub fn columns(app: &mut App) {
    let body = Columns {
        cols: app.manager.settings.columns.clone(),
        current: 0,
    };
    app.open_dialog(Dialog::new("columns", "Columns", (440.0, 520.0), body));
}

fn column_title(key: &str) -> &str {
    DEFAULT_COLUMNS
        .iter()
        .find(|c| c.key == key)
        .map_or(key, |c| c.title)
}

/// Write the columns ("filename" always shows); None puts back the defaults.
pub fn apply_columns(app: &mut App, columns: Option<Vec<ColumnState>>) {
    app.manager.settings.columns = match columns {
        Some(mut c) => {
            for col in &mut c {
                if col.0 == "filename" {
                    col.1 = true;
                }
            }
            c
        }
        None => DEFAULT_COLUMNS
            .iter()
            .map(|c| ColumnState(c.key.into(), c.visible, c.width))
            .collect(),
    };
    app.save_settings();
    app.rebuild();
}

/// One column in the list: its check box, then its title, which selects the row. True when clicked.
/// What a click or a drag did to a column row.
#[derive(Default)]
struct RowAction {
    clicked: bool,
    /// The row dragged here (its index).
    dropped: Option<usize>,
}

fn column_row(
    ui: &mut Ui,
    p: &crate::theme::Palette,
    c: &mut ColumnState,
    index: usize,
    selected: bool,
) -> RowAction {
    let id = ui.scope_id().with(("column", &c.0));
    let title = column_title(&c.0).to_string();
    let fixed = c.0 == "filename";
    let mut out = RowAction::default();
    ui.horizontal(|ui| {
        ui.add_enabled(!fixed, egui::Checkbox::without_text(&mut c.1))
            .labelled_by(id);
        let size = egui::vec2(ui.available_width(), 22.0);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        // Drag a title onto another row to move it there (1.x's InternalMove).
        let resp = ui.interact(rect, id, egui::Sense::click_and_drag());
        resp.dnd_set_drag_payload(index);
        if let Some(from) = resp.dnd_release_payload::<usize>() {
            out.dropped = Some(*from);
        }
        if resp
            .dnd_hover_payload::<usize>()
            .is_some_and(|f| *f != index)
        {
            ui.painter()
                .hline(rect.x_range(), rect.top(), egui::Stroke::new(2.0, p.ac));
        }
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, &title));
        if selected {
            ui.painter().rect_filled(rect, 4.0, p.acsel);
        }
        ui.painter().text(
            rect.left_center() + egui::vec2(4.0, 0.0),
            egui::Align2::LEFT_CENTER,
            &title,
            egui::FontId::proportional(13.0),
            if selected { p.actx } else { p.tx },
        );
        out.clicked = resp.clicked();
    });
    out
}

impl Columns {
    /// Move a column to another place in the list (a drop); it becomes the current one.
    pub fn move_column(&mut self, from: usize, to: usize) {
        if from == to || from >= self.cols.len() || to >= self.cols.len() {
            return;
        }
        let c = self.cols.remove(from);
        self.cols.insert(to, c);
        self.current = to;
    }

    fn shift(&mut self, d: i64) {
        let j = self.current as i64 + d;
        if j < 0 || j as usize >= self.cols.len() {
            return;
        }
        self.cols.swap(self.current, j as usize);
        self.current = j as usize;
    }
}

impl Body for Columns {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let mut ok = false;
        let mut reset = false;
        kit::body(ui, |ui| {
            kit::muted(
                ui,
                &p,
                "Check the columns to show. Drag or use the buttons to change the order.",
            );
            ui.horizontal_top(|ui| {
                let list_w = ui.available_width() - 110.0;
                ui.vertical(|ui| {
                    ui.set_width(list_w);
                    kit::panel(ui, &p, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        let mut moved = None;
                        for i in 0..self.cols.len() {
                            let r = column_row(ui, &p, &mut self.cols[i], i, self.current == i);
                            if r.clicked {
                                self.current = i;
                            }
                            if let Some(from) = r.dropped {
                                moved = Some((from, i));
                            }
                        }
                        if let Some((from, to)) = moved {
                            self.move_column(from, to);
                        }
                    });
                });
                ui.vertical(|ui| {
                    if kit::plain(ui, &p, "Move up") {
                        self.shift(-1);
                    }
                    if kit::plain(ui, &p, "Move down") {
                        self.shift(1);
                    }
                    ui.add_space(20.0);
                    if ui
                        .button("Reset")
                        .on_hover_text("Back to the default columns")
                        .clicked()
                    {
                        reset = true;
                    }
                });
            });
            if let Some(c) = self.cols.get_mut(self.current) {
                kit::row(ui, "Selected column width", 150.0, |ui, by| {
                    ui.add(egui::DragValue::new(&mut c.2).range(10..=2000))
                        .labelled_by(by);
                    ui.label("px");
                });
            }
        });
        kit::footer(ui, &p, |ui| {
            kit::right(ui, |ui| {
                ok |= kit::primary(ui, &p, "OK");
                if kit::plain(ui, &p, "Cancel") {
                    cx.close = true;
                }
            });
        });
        if reset {
            apply_columns(app, None);
            cx.close = true;
        } else if ok || cx.enter(ui) {
            apply_columns(app, Some(self.cols.clone()));
            cx.close = true;
        }
        if cx.escape(ui) {
            cx.close = true;
        }
    }
    any_body!();
}

// ---------------------------------------------------------------------- Choose Font

pub struct Font {
    pub families: Vec<String>,
    pub family: String,
    pub size: f64,
    pub filter: String,
    /// Chosen for the Options window: OK hands it there instead of writing it.
    pub for_options: bool,
    /// Families whose font file was looked for (loaded or not), so each is looked for once.
    tried: std::collections::HashSet<String>,
}

/// The egui font family that draws `family` for the preview, once its file is loaded. The first call
/// loads it (fontconfig finds the file) and gives None: egui takes new fonts at its next pass.
fn preview_family(
    ctx: &egui::Context,
    family: &str,
    tried: &mut std::collections::HashSet<String>,
) -> Option<egui::FontFamily> {
    let name = format!("preview:{family}");
    let fam = egui::FontFamily::Name(name.clone().into());
    let loaded = ctx.fonts(|f| f.definitions().families.contains_key(&fam));
    if loaded {
        return Some(fam);
    }
    if !tried.insert(family.to_string()) {
        return None;
    }
    let file = family_file(family)?;
    let bytes = std::fs::read(file).ok()?;
    let mut defs = ctx.fonts(|f| f.definitions().clone());
    defs.font_data.insert(
        name.clone(),
        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
    );
    // The system fonts after it: a glyph the family lacks still draws.
    let mut list = vec![name];
    list.extend(
        defs.families
            .get(&egui::FontFamily::Proportional)
            .cloned()
            .unwrap_or_default(),
    );
    defs.families.insert(fam, list);
    ctx.set_fonts(defs);
    None
}

/// The font file of a family, when fontconfig has that family (not a stand-in for it).
fn family_file(family: &str) -> Option<std::path::PathBuf> {
    if cfg!(windows) {
        return None;
    }
    let out = std::process::Command::new("fc-match")
        .args(["-f", "%{family[0]}\n%{file}", family])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (found, file) = text.split_once('\n')?;
    let file = std::path::PathBuf::from(file.trim());
    (found.trim().eq_ignore_ascii_case(family) && file.is_file()).then_some(file)
}

/// "family,size" of a font setting.
pub fn split_font(font: &str) -> (String, f64) {
    let (f, s) = font.split_once(',').unwrap_or((font, ""));
    (f.trim().to_string(), s.trim().parse().unwrap_or(10.0))
}

pub fn font(app: &mut App, current: Option<String>) {
    let families = os().font_families();
    let for_options = current.is_some();
    let (mut family, size) =
        split_font(&current.unwrap_or_else(|| app.manager.settings.list_font.clone()));
    if family.is_empty() {
        family = families
            .iter()
            .find(|x| ["Noto Sans", "Cantarell", "DejaVu Sans"].contains(&x.as_str()))
            .or(families.first())
            .cloned()
            .unwrap_or_default();
    }
    let body = Font {
        families,
        family,
        size,
        filter: String::new(),
        for_options,
        tried: Default::default(),
    };
    app.open_dialog(Dialog::new("font", "Choose Font", (460.0, 520.0), body));
}

impl Body for Font {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let mut ok = false;
        kit::body(ui, |ui| {
            kit::header(
                ui,
                &p,
                &Head::new(
                    "general",
                    "Choose Font",
                    "The font of the download list",
                    Badge::None,
                ),
                |_| {},
            );
            let by = kit::caption(ui, &p, "Font family");
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("Search fonts")
                    .desired_width(f32::INFINITY),
            )
            .labelled_by(by);
            kit::panel(ui, &p, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let needle = self.filter.to_lowercase();
                        for f in self
                            .families
                            .iter()
                            .filter(|f| f.to_lowercase().contains(&needle))
                        {
                            if ui.selectable_label(*f == self.family, f).clicked() {
                                self.family = f.clone();
                            }
                        }
                    });
            });
            kit::row(ui, "Size", 60.0, |ui, by| {
                ui.add(
                    egui::DragValue::new(&mut self.size)
                        .range(6.0..=48.0)
                        .speed(0.5),
                )
                .labelled_by(by);
                ui.label("pt");
            });
            // The sample in the chosen family and size (1.x previews the QFont).
            let fam = match preview_family(ui.ctx(), &self.family, &mut self.tried) {
                Some(f) => f,
                None => {
                    // Loaded at the next pass.
                    ui.ctx().request_repaint();
                    egui::FontFamily::Proportional
                }
            };
            kit::panel(ui, &p, |ui| {
                ui.label(
                    RichText::new("AaBbCc 0123 · ubuntu-24.04.iso")
                        .font(egui::FontId::new(self.size as f32 * 1.333, fam)),
                );
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
        if (ok || cx.enter(ui)) && !self.family.is_empty() {
            let font = format!("{},{}", self.family, self.size);
            if self.for_options {
                app.font_chosen = Some(font);
            } else {
                app.manager.settings.list_font = font;
                app.save_settings();
                app.rebuild();
            }
            cx.close = true;
        }
        if cx.escape(ui) {
            cx.close = true;
        }
    }
    any_body!();
}

// ---------------------------------------------------------------------- Find file / Search settings

pub struct Find {
    /// "Find file" (F3 without a query): with the query field and Find.
    pub with_query: bool,
    pub query: String,
    pub flags: i64,
    focused: bool,
}

pub fn find(app: &mut App, with_query: bool) {
    let body = Find {
        with_query,
        query: app.search.query.clone(),
        flags: app.search.flags & 0x1F,
        focused: false,
    };
    let title = if with_query {
        "Find file"
    } else {
        "LID Search settings"
    };
    app.open_dialog(Dialog::new("find", title, (460.0, 340.0), body));
}

impl Find {
    fn bit(&mut self, ui: &mut Ui, bit: i64, label: &str) {
        let mut on = self.flags & bit != 0;
        if ui.checkbox(&mut on, label).changed() {
            self.flags = if on {
                self.flags | bit
            } else {
                self.flags & !bit
            };
        }
    }
}

impl Body for Find {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let mut ok = false;
        kit::body(ui, |ui| {
            if self.with_query {
                let by = kit::caption(ui, &p, "Find");
                let r = ui
                    .horizontal(|ui| {
                        ui.add(crate::icons::icon("search", 16.0, &p, p.mu));
                        kit::field(ui, &mut self.query, by)
                    })
                    .inner;
                if !self.focused {
                    r.request_focus();
                    self.focused = true;
                }
            }
            kit::panel(ui, &p, |ui| {
                kit::caption(ui, &p, "Look in");
                self.bit(ui, flags::TARGET_NAME, "File name or part of the name");
                self.bit(
                    ui,
                    flags::TARGET_DESCRIPTION,
                    "Description or part of the description",
                );
                self.bit(
                    ui,
                    flags::TARGET_LINKS,
                    "Site name / download link / parent web page / referer",
                );
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                self.bit(ui, flags::MATCH_CASE, "Match case");
                self.bit(ui, flags::WHOLE_STRING, "Match whole string only");
            });
        });
        let with_query = self.with_query;
        kit::footer(ui, &p, |ui| {
            if with_query {
                kit::faint(ui, &p, "Ctrl+F · F3 finds next");
            }
            kit::right(ui, |ui| {
                ok |= kit::primary(ui, &p, if with_query { "Find" } else { "OK" });
                if kit::plain(ui, &p, "Cancel") {
                    cx.close = true;
                }
            });
        });
        if ok || cx.enter(ui) {
            if self.flags & flags::TARGETS == 0 {
                self.flags |= flags::TARGET_NAME;
                *cx.boxx = Some(MsgBox::info(
                    "Search settings",
                    lider_core::search::NO_TARGET,
                ));
                return;
            }
            app.search.flags = self.flags & 0x1F;
            if self.with_query {
                app.search.query = self.query.clone();
                app.search_text = self.query.clone();
            }
            app.finder.settings = app.search.clone();
            app.save_search();
            if self.with_query {
                match app.find_next_match() {
                    Ok(Some(_)) => {}
                    Ok(None) => {
                        *cx.boxx = Some(MsgBox::info("Find file", "No such files found"));
                        return;
                    }
                    Err(e) => {
                        *cx.boxx = Some(MsgBox::info("LID Search settings", &e));
                        return;
                    }
                }
            }
            app.rebuild();
            cx.close = true;
        }
        if cx.escape(ui) {
            cx.close = true;
        }
    }
    any_body!();
}

// ---------------------------------------------------------------------- the category editor

/// The category form (1.x `options.CategoryDialog`): in its own window, and over Options.
pub struct CategoryForm {
    /// The category's name when editing one; None for a new one.
    pub old: Option<String>,
    pub name: String,
    pub types: kit::TagInput,
    pub use_sites: bool,
    pub sites: kit::TagInput,
    pub folder: String,
    pub remember: bool,
}

impl CategoryForm {
    pub fn new(cat: Option<&Category>) -> CategoryForm {
        match cat {
            Some(c) => CategoryForm {
                old: Some(c.name.clone()),
                name: c.name.clone(),
                types: kit::TagInput::new(c.extensions.clone(), true),
                use_sites: !c.sites.is_empty(),
                sites: kit::TagInput::new(c.sites.clone(), false),
                folder: c.save_dir.clone(),
                remember: c.remember_last,
            },
            None => CategoryForm {
                old: None,
                name: String::new(),
                types: kit::TagInput::new(vec![], true),
                use_sites: false,
                sites: kit::TagInput::new(vec![], false),
                folder: lider_core::paths::home_dir()
                    .join("Downloads/New")
                    .to_string_lossy()
                    .into_owned(),
                remember: false,
            },
        }
    }

    fn icon(&self) -> String {
        match &self.old {
            Some(n)
                if ["compressed", "documents", "music", "programs", "video"]
                    .contains(&n.to_lowercase().as_str()) =>
            {
                format!("category-{}", n.to_lowercase())
            }
            _ => "folder".into(),
        }
    }

    /// The form's category, or None while the name or the folder is empty.
    pub fn category(&mut self) -> Option<Category> {
        self.types.commit();
        self.sites.commit();
        if self.name.trim().is_empty() || self.folder.trim().is_empty() {
            return None;
        }
        Some(Category {
            name: self.name.trim().to_string(),
            extensions: self.types.values.clone(),
            save_dir: self.folder.trim().to_string(),
            sites: if self.use_sites {
                self.sites.values.clone()
            } else {
                vec![]
            },
            remember_last: self.remember,
        })
    }

    /// Draw the form; Some(Some(category)) on OK, Some(None) on Cancel.
    pub fn ui(&mut self, ui: &mut Ui, cx: &mut Cx) -> Option<Option<Category>> {
        let p = cx.p;
        let mut out = None;
        let subtitle = match &self.old {
            None => "New category".to_string(),
            Some(n) => format!("Edit the {n} category"),
        };
        let icon = self.icon();
        kit::body(ui, |ui| {
            kit::header(
                ui,
                &p,
                &Head::new(&icon, "LID categories", &subtitle, Badge::Tile),
                |_| {},
            );
            let general = self.old.as_deref() == Some("General");
            kit::row(ui, "Category name", 110.0, |ui, by| {
                ui.add_enabled(
                    !general,
                    egui::TextEdit::singleline(&mut self.name).desired_width(f32::INFINITY),
                )
                .labelled_by(by);
            });
            let by = ui
                .label("Automatically put in this category the following file types:")
                .id;
            self.types
                .ui(ui, &p, "Add a type, e.g. mkv", "types", true, by);
            let by = ui
                .checkbox(
                    &mut self.use_sites,
                    "Automatically put in this category the files from these sites:",
                )
                .id;
            self.sites.ui(
                ui,
                &p,
                "Add a site, e.g. *.example.com",
                "sites",
                self.use_sites,
                by,
            );
            let by = ui.label("Save future downloads of this category to:").id;
            kit::folder_field(ui, &p, &mut self.folder, by);
            ui.checkbox(&mut self.remember, "Remember last save path");
        });
        let mut ok = false;
        kit::footer(ui, &p, |ui| {
            kit::right(ui, |ui| {
                ok |= kit::primary(ui, &p, "OK");
                if kit::plain(ui, &p, "Cancel") {
                    out = Some(None);
                }
            });
        });
        if (ok || cx.enter(ui))
            && let Some(c) = self.category()
        {
            out = Some(Some(c));
        }
        if cx.escape(ui) {
            out = Some(None);
        }
        out
    }
}

/// The category form floating over another window (Download File Info, Options), as the Tauri build's
/// overlay: Some(Some(category)) on OK, Some(None) on Cancel.
pub fn form_over(ui: &mut Ui, cx: &mut Cx, form: &mut CategoryForm) -> Option<Option<Category>> {
    let p = cx.p;
    let mut out = None;
    egui::Modal::new(ui.scope_id().with("category-form"))
        .frame(
            egui::Frame::new()
                .fill(p.win)
                .stroke(egui::Stroke::new(1.0, p.line2))
                .corner_radius(10),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(600.0);
            out = form.ui(ui, cx);
        });
    out
}

/// Add a category (`old` None; refused when the name exists) or replace one; a rename moves its
/// downloads. True when it was written.
pub fn save_category(app: &mut App, old: Option<&str>, category: Category) -> bool {
    let m = &mut app.manager;
    let written = match old {
        None => {
            if m.settings
                .categories
                .iter()
                .any(|c| c.name == category.name)
            {
                false
            } else {
                m.settings.categories.push(category);
                true
            }
        }
        Some(name) => {
            if let Some(i) = m.settings.categories.iter().position(|c| c.name == name) {
                m.settings.categories[i] = category.clone();
            }
            if category.name != name {
                let ids: Vec<i64> = m
                    .downloads
                    .values()
                    .filter(|d| d.category == name)
                    .map(|d| d.id)
                    .collect();
                for id in ids {
                    m.edit(id, |d| d.category = category.name.clone());
                }
            }
            true
        }
    };
    app.save_settings();
    app.rebuild();
    written
}

pub struct CategoryWindow {
    pub form: CategoryForm,
}

/// Add category (None) or a category's Properties.
pub fn category(app: &mut App, name: Option<&str>) {
    let cat = name.and_then(|n| {
        app.manager
            .settings
            .categories
            .iter()
            .find(|c| c.name == n)
            .cloned()
    });
    let body = CategoryWindow {
        form: CategoryForm::new(cat.as_ref()),
    };
    app.open_dialog(Dialog::new(
        "category",
        "LID categories",
        (520.0, 420.0),
        body,
    ));
}

impl Body for CategoryWindow {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        match self.form.ui(ui, cx) {
            Some(Some(c)) => {
                let old = self.form.old.clone();
                save_category(app, old.as_deref(), c);
                cx.close = true;
            }
            Some(None) => cx.close = true,
            None => {}
        }
    }
    any_body!();
}
