//! "Download All Links" and the grabber (1.x `add.LinksDialog`, `main_window._grabber`; the Tauri
//! build's Links.svelte): the links of a web page, the browser's "all links", or the addresses in the
//! clipboard; the checked ones go to the Main queue, which then starts.

use std::sync::{Arc, Mutex};

use egui::{RichText, Ui};
use lider_core::links::{Filters, Link, LinkRow};
use lider_core::models::MAIN_QUEUE;
use lider_core::view;
use serde_json::{Map, Value};

use super::kit::{self, Badge, Head};
use super::{Body, Cx, Dialog};
use crate::any_body;
use crate::app::App;
use crate::boxes::{Answer, MsgBox};
use crate::icons::icon;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Grab,
    Busy,
    Links,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveMode {
    /// Every file to the folder of its category.
    Category,
    /// All files to one category.
    OneCategory,
    /// All files to one directory.
    OneDir,
}

type Fetched = Result<(String, Vec<Link>), String>;

pub struct Links {
    pub page: Page,
    pub grab_url: String,
    pub links: Vec<Link>,
    referer: String,
    parent_page: String,
    headers: Vec<(String, String)>,
    pub rows: Vec<LinkRow>,
    pub checked: Vec<bool>,
    /// The selected rows (click, Ctrl, Shift, Ctrl+A); Space toggles their checks.
    pub selected: Vec<usize>,
    /// The row Shift selects from.
    anchor: Option<usize>,
    pub hide_html: bool,
    pub hide_images: bool,
    pub hide_repeated: bool,
    pub save_mode: SaveMode,
    pub one_category: String,
    pub dir: String,
    pub use_text: bool,
    fetch: Option<Arc<Mutex<Option<Fetched>>>>,
    focused: bool,
    /// A note is open; the window closes after it.
    close_after: bool,
    /// The list came from the grabber: make the window as wide as the list's.
    widen: bool,
    /// The list was drawn on the last frame.
    listed: bool,
}

impl Links {
    fn new(app: &App, page: Page) -> Links {
        let first = app.manager.settings.categories.first();
        Links {
            page,
            grab_url: "https://".into(),
            links: vec![],
            referer: String::new(),
            parent_page: String::new(),
            headers: vec![],
            rows: vec![],
            checked: vec![],
            selected: vec![],
            anchor: None,
            hide_html: true,
            hide_images: false,
            hide_repeated: true,
            save_mode: SaveMode::Category,
            one_category: first.map(|c| c.name.clone()).unwrap_or_default(),
            dir: first.map(|c| c.save_dir.clone()).unwrap_or_default(),
            use_text: true,
            fetch: None,
            focused: false,
            close_after: false,
            widen: false,
            listed: false,
        }
    }

    /// The rows for the filters; the checks start on the auto-capture file types.
    pub fn refill(&mut self, app: &App) {
        let f = Filters {
            hide_html: self.hide_html,
            hide_images: self.hide_images,
            hide_repeated: self.hide_repeated,
        };
        self.rows = lider_core::links::rows(&app.manager.settings, &self.links, f);
        self.checked = self.rows.iter().map(|r| r.checked).collect();
        self.selected.clear();
        self.anchor = None;
    }

    /// A click on a row: plain selects it, Ctrl toggles it, Shift selects from the anchor to it.
    pub fn click(&mut self, i: usize, ctrl: bool, shift: bool) {
        if shift && let Some(a) = self.anchor {
            let range: Vec<usize> = (a.min(i)..=a.max(i)).collect();
            if ctrl {
                for r in range {
                    if !self.selected.contains(&r) {
                        self.selected.push(r);
                    }
                }
            } else {
                self.selected = range;
            }
            return;
        }
        if ctrl {
            if let Some(at) = self.selected.iter().position(|x| *x == i) {
                self.selected.remove(at);
            } else {
                self.selected.push(i);
            }
        } else {
            self.selected = vec![i];
        }
        self.anchor = Some(i);
    }

    /// Space: the selected rows all get the opposite of the first one's check.
    pub fn toggle_selected(&mut self) {
        let Some(first) = self.selected.first() else {
            return;
        };
        let on = !self.checked[*first];
        for i in &self.selected {
            self.checked[*i] = on;
        }
    }
}

fn label() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!("links-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// Links to choose from: (address, text, kind) each; the chosen ones go to the Main queue.
pub fn open(
    app: &mut App,
    items: Vec<(String, String, String)>,
    referer: String,
    parent_page: String,
    headers: Map<String, Value>,
) {
    let mut body = Links::new(app, Page::Links);
    body.links = items;
    body.referer = referer;
    body.parent_page = parent_page;
    body.headers = headers
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
        .collect();
    body.refill(app);
    app.effects.push(crate::app::Effect::Show);
    app.open_dialog(Dialog::new(
        &label(),
        "Download All Links",
        (840.0, 640.0),
        body,
    ));
}

/// The grabber: an address whose page's links are listed.
pub fn grabber(app: &mut App) {
    let mut body = Links::new(app, Page::Grab);
    let clip = super::add::clipboard_url();
    if !clip.is_empty() {
        body.grab_url = clip;
    }
    app.open_dialog(Dialog::new("grabber", "Grabber", (560.0, 200.0), body));
}

/// Fetch one page (at most 8 MiB) and list its links (`grab_page`).
fn fetch_page(app: &App, url: String) -> Arc<Mutex<Option<Fetched>>> {
    let slot: Arc<Mutex<Option<Fetched>>> = Default::default();
    let out = slot.clone();
    let client = app.manager.client();
    app.rt.spawn(async move {
        let r: Fetched = async {
            let client = client.map_err(|e| e.message())?;
            let mut r = client
                .open(&url, 0, false, &Default::default())
                .await
                .map_err(|e| e.message())?;
            let charset = r
                .headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .and_then(|ct| {
                    ct.split(';').find_map(|p| {
                        p.trim()
                            .strip_prefix("charset=")
                            .map(|c| c.trim_matches('"').to_string())
                    })
                })
                .unwrap_or_else(|| "utf-8".into());
            let mut body = Vec::new();
            while body.len() < 8 * 1024 * 1024 {
                match r
                    .body
                    .chunk(client.timeout)
                    .await
                    .map_err(|e| e.message())?
                {
                    Some(c) => body.extend_from_slice(&c),
                    None => break,
                }
            }
            body.truncate(8 * 1024 * 1024);
            let enc =
                encoding_rs::Encoding::for_label(charset.as_bytes()).unwrap_or(encoding_rs::UTF_8);
            let text = enc.decode_without_bom_handling(&body).0.into_owned();
            Ok((
                r.url.clone(),
                lider_core::links::extract_links(&text, &r.url),
            ))
        }
        .await;
        *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
    });
    slot
}

/// The checked links as downloads (`add_choices`): a free name each, all in the Main queue, which
/// then starts. Returns how many were added.
pub fn add_choices(app: &mut App, body: &Links) -> usize {
    let cats = app.manager.settings.categories.clone();
    let headers: Vec<(String, String)> = body.headers.clone();
    let mut n = 0;
    for (r, _) in body.rows.iter().zip(&body.checked).filter(|(_, on)| **on) {
        let cat = match body.save_mode {
            SaveMode::OneCategory => cats.iter().find(|c| c.name == body.one_category),
            _ => cats.iter().find(|c| c.name == r.category),
        }
        .or(cats.first())
        .cloned();
        let Some(cat) = cat else { continue };
        let dir = if body.save_mode == SaveMode::OneDir {
            body.dir.clone()
        } else {
            cat.save_dir.clone()
        };
        let name = view::unique_name(&app.manager, &dir, &r.name);
        app.manager.add(
            &r.url,
            &name,
            lider_core::manager::NewDownload {
                save_dir: Some(dir),
                category: Some(cat.name.clone()),
                description: if body.use_text {
                    r.text.clone()
                } else {
                    String::new()
                },
                referer: body.referer.clone(),
                parent_page: body.parent_page.clone(),
                headers: headers.iter().cloned().collect(),
                queue: MAIN_QUEUE.into(),
                ..Default::default()
            },
        );
        n += 1;
    }
    if n > 0 {
        let events = app.manager.start_queue(MAIN_QUEUE);
        app.handle_events(&events);
    }
    app.rebuild();
    n
}

impl Body for Links {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        if let Some(slot) = &self.fetch {
            let done = slot.lock().unwrap_or_else(|e| e.into_inner()).take();
            match done {
                Some(Ok((base, links))) => {
                    self.fetch = None;
                    if links.is_empty() {
                        self.note(cx, "Cannot find the links!");
                    } else {
                        self.links = links;
                        self.referer = base.clone();
                        self.parent_page = base;
                        self.refill(app);
                        self.page = Page::Links;
                        self.widen = true;
                    }
                }
                Some(Err(e)) => {
                    self.fetch = None;
                    self.note(cx, &format!("Cannot open the web page:\n{e}"));
                }
                None => ui
                    .ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }
        // The grabber's small window becomes the list's, a frame after the list shows (the window's
        // height fit of that frame would keep the old width).
        if self.widen && self.page == Page::Links && self.listed {
            self.widen = false;
            if !ui.ctx().embed_viewports() {
                // Keep the height the window was fitted to.
                let h = ui
                    .ctx()
                    .input(|i| i.viewport().inner_rect.map_or(640.0, |r| r.height()));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(840.0, h)));
            }
        }
        self.listed = self.page == Page::Links;
        match self.page {
            Page::Grab => self.grab_page(app, ui, cx),
            Page::Busy => {
                // Tall enough for the message box that may come over it.
                kit::body(ui, |ui| {
                    ui.set_min_height(240.0);
                    ui.add_space(100.0);
                    ui.vertical_centered(|ui| kit::muted(ui, &cx.p, "Reading the web page…"));
                });
                if cx.escape(ui) {
                    cx.close = true;
                }
            }
            Page::Links => {
                cx.title = Some("Download All Links".into());
                self.links_page(app, ui, cx);
            }
        }
    }

    fn answer(&mut self, _app: &mut App, _a: Answer, cx: &mut Cx) {
        if self.close_after {
            cx.close = true;
        }
    }
    any_body!();
}

impl Links {
    fn note(&mut self, cx: &mut Cx, text: &str) {
        self.close_after = true;
        *cx.boxx = Some(MsgBox::info("Grabber", text).with_icon("info", true));
    }

    fn grab_page(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let mut ok = false;
        kit::body(ui, |ui| {
            kit::header(
                ui,
                &p,
                &Head::new(
                    "grabber",
                    "Grabber",
                    "Enter the address of the web page to explore:",
                    Badge::None,
                ),
                |_| {},
            );
            let by = kit::caption(ui, &p, "Address");
            let r = kit::mono_field(ui, &mut self.grab_url, by);
            if !self.focused {
                r.request_focus();
                self.focused = true;
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
        if ok || cx.enter(ui) {
            let u = self.grab_url.trim().to_string();
            if !view::is_url(&u) {
                cx.close = true;
                return;
            }
            self.page = Page::Busy;
            self.fetch = Some(fetch_page(app, u));
        }
        if cx.escape(ui) {
            cx.close = true;
        }
    }

    fn links_page(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let mut ok = false;
        let mut refill = false;
        let subtitle = format!("{} links found · check the ones to add", self.links.len());
        let cats: Vec<String> = app
            .manager
            .settings
            .categories
            .iter()
            .map(|c| c.name.clone())
            .collect();
        kit::body(ui, |ui| {
            kit::header(
                ui,
                &p,
                &Head::new("grabber", "Download All Links", &subtitle, Badge::None),
                |_| {},
            );
            // FILE fits the longest name, up to 480 px (1.x resizes the column to its contents).
            let font = egui::TextStyle::Body.resolve(ui.style());
            let name_w = self
                .rows
                .iter()
                .map(|r| {
                    ui.painter()
                        .layout_no_wrap(r.name.clone(), font.clone(), p.tx)
                        .size()
                        .x
                })
                .fold(120.0_f32, f32::max)
                .min(480.0)
                + 4.0;
            let gap = ui.spacing().item_spacing.x;
            kit::panel(ui, &p, |ui| {
                ui.horizontal(|ui| {
                    // The check-all box: checks everything, or unchecks it when all are checked.
                    let all = !self.checked.is_empty() && self.checked.iter().all(|c| *c);
                    let some = self.checked.iter().any(|c| *c);
                    let mut on = all;
                    let r =
                        ui.add(egui::Checkbox::without_text(&mut on).indeterminate(some && !all));
                    let r = r.on_hover_text("Check all");
                    r.widget_info(|| {
                        egui::WidgetInfo::selected(egui::Role::CheckBox, true, all, "Check all")
                    });
                    if r.clicked() {
                        let to = !all;
                        self.checked.iter_mut().for_each(|c| *c = to);
                    }
                    let head = |ui: &mut Ui, text: &str, w: f32| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(w, 20.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_min_width(w);
                                ui.label(
                                    RichText::new(text)
                                        .small()
                                        .family(crate::fonts::bold())
                                        .color(p.mu),
                                );
                            },
                        );
                    };
                    head(ui, "FILE", 16.0 + gap + name_w);
                    head(ui, "TYPE", 64.0);
                    ui.label(
                        RichText::new("ADDRESS")
                            .small()
                            .family(crate::fonts::bold())
                            .color(p.mu),
                    );
                });
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .min_scrolled_height(300.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        let mut clicked = None;
                        for (i, r) in self.rows.iter().enumerate() {
                            let row = ui.horizontal(|ui| {
                                let check = ui
                                    .add(egui::Checkbox::without_text(&mut self.checked[i]))
                                    .accessible_name(&r.name);
                                ui.add(icon(&r.icon, 16.0, &p, p.tx));
                                let name = ui
                                    .allocate_ui_with_layout(
                                        egui::vec2(name_w, 20.0),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.set_min_width(name_w);
                                            ui.add(egui::Label::new(&r.name).truncate())
                                        },
                                    )
                                    .inner;
                                if !r.text.is_empty() {
                                    name.clone().on_hover_text(&r.text);
                                }
                                ui.allocate_ui(egui::vec2(64.0, 20.0), |ui| {
                                    ui.set_width(64.0);
                                    if !r.kind.is_empty() {
                                        kit::pill(ui, &p, &r.kind, "muted");
                                    }
                                });
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&r.address).monospace().color(p.mu),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(&r.url);
                                check.rect.right()
                            });
                            // The row (right of its check box) selects; the selection is drawn over it.
                            let mut rect = row.response.rect;
                            rect.min.x = row.inner + 2.0;
                            let hit = ui.interact(
                                rect,
                                ui.scope_id().with(("link-row", i)),
                                egui::Sense::click(),
                            );
                            if self.selected.contains(&i) {
                                ui.painter().rect_filled(
                                    rect,
                                    4.0,
                                    crate::theme::alpha(p.ac, 0.14),
                                );
                            }
                            if hit.clicked() {
                                clicked = Some(i);
                            }
                        }
                        if let Some(i) = clicked {
                            let m = ui.input(|x| x.modifiers);
                            self.click(i, m.command, m.shift);
                        }
                    });
            });
            ui.horizontal(|ui| {
                if kit::plain(ui, &p, "Check All") {
                    self.checked.iter_mut().for_each(|c| *c = true);
                }
                if kit::plain(ui, &p, "Uncheck All") {
                    self.checked.iter_mut().for_each(|c| *c = false);
                }
                kit::right(ui, |ui| {
                    refill |= ui
                        .checkbox(&mut self.hide_repeated, "Hide repeated files")
                        .changed();
                    refill |= ui
                        .checkbox(&mut self.hide_images, "Hide images")
                        .on_hover_text("Hide images located on this web page")
                        .changed();
                    refill |= ui
                        .checkbox(&mut self.hide_html, "Hide HTML files")
                        .changed();
                });
            });
            kit::panel(ui, &p, |ui| {
                kit::caption(ui, &p, "Save to");
                ui.radio_value(
                    &mut self.save_mode,
                    SaveMode::Category,
                    "Every file to the folder of its category",
                );
                ui.horizontal(|ui| {
                    let by = ui
                        .radio_value(
                            &mut self.save_mode,
                            SaveMode::OneCategory,
                            "All files to one category",
                        )
                        .id;
                    if kit::category_combo(
                        ui,
                        &p,
                        "links-category",
                        180.0,
                        &mut self.one_category,
                        &cats,
                        by,
                    ) {
                        self.save_mode = SaveMode::OneCategory;
                    }
                });
                ui.horizontal(|ui| {
                    let by = ui
                        .radio_value(
                            &mut self.save_mode,
                            SaveMode::OneDir,
                            "All files to one directory",
                        )
                        .id;
                    if kit::folder_field_with(ui, &p, &mut self.dir, by, "Browse...", "Save To") {
                        self.save_mode = SaveMode::OneDir;
                    }
                });
            });
            ui.checkbox(
                &mut self.use_text,
                "Use link texts as download descriptions",
            );
        });
        let n = self.checked.iter().filter(|c| **c).count();
        kit::footer(ui, &p, |ui| {
            kit::pill(ui, &p, &format!("{n} checked"), "accent");
            kit::right(ui, |ui| {
                ok |= kit::primary(ui, &p, "OK");
                if kit::plain(ui, &p, "Cancel") {
                    cx.close = true;
                }
            });
        });
        if refill {
            self.refill(app);
        }
        if !cx.blocked && !ui.ctx().egui_wants_keyboard_input() {
            let (space, all) = ui.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Space),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::A),
                )
            });
            if all {
                self.selected = (0..self.rows.len()).collect();
            }
            if space {
                self.toggle_selected();
            }
        }
        if ok || cx.enter(ui) {
            add_choices(app, self);
            cx.close = true;
        }
        if cx.escape(ui) {
            cx.close = true;
        }
    }
}
