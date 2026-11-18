//! Adding downloads: "Enter new address to download", "Duplicate download link", "Download File Info"
//! and "Batch download" (1.x `add.py` and `main_window.add_url`, the Tauri build's Add.svelte and
//! Batch.svelte). One window walks the steps of one address: the address → the duplicate rule → the
//! probe (or yt-dlp for YouTube) → Download File Info → the download.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use egui::{RichText, Ui};
use lider_core::batch::Fill;
use lider_core::media::{self, MediaInfo, Variant};
use lider_core::models::{MAIN_QUEUE, Status};
use lider_core::names::{filename_from_url, safe_filename};
use lider_core::{fmt, view};

use super::kit::{self, Badge, Head};
use super::small::CategoryForm;
use super::{Body, Cx, Dialog};
use crate::any_body;
use crate::app::{App, Request};
use crate::boxes::{Answer, Button, MsgBox};
use crate::icons::icon;

/// The addresses typed in "Enter new address", most recent first, at most 20 (for this run of LID).
static HISTORY: LazyLock<Mutex<Vec<String>>> = LazyLock::new(Default::default);
static NEXT: AtomicU64 = AtomicU64::new(1);

fn remember_url(url: &str) {
    let mut h = HISTORY.lock().unwrap_or_else(|e| e.into_inner());
    h.retain(|u| u != url);
    h.insert(0, url.to_string());
    h.truncate(20);
}

fn history() -> Vec<String> {
    HISTORY.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// A result that comes from another thread; the window looks at it each frame.
type Slot<T> = Arc<Mutex<Option<T>>>;

fn take<T>(slot: &Option<Slot<T>>) -> Option<T> {
    slot.as_ref()
        .and_then(|s| s.lock().unwrap_or_else(|e| e.into_inner()).take())
}

/// The clipboard's text when it is one address.
pub fn clipboard_url() -> String {
    let t = arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        .unwrap_or_default();
    let t = t.trim();
    if view::is_url(t) {
        t.to_string()
    } else {
        String::new()
    }
}

/// The file name to use: the probed one; the browser's when it has a dot and the probed one has none;
/// a name from the address when the probe failed (`probed` None).
pub fn name_choice(url: &str, probed: Option<&str>, browser: &str) -> String {
    match probed {
        None => safe_filename(&if browser.is_empty() {
            filename_from_url(url)
        } else {
            browser.to_string()
        }),
        Some(name) if browser.contains('.') && !name.contains('.') => safe_filename(browser),
        Some(name) => name.to_string(),
    }
}

fn cat_icon(name: &str) -> String {
    view::category_icon(name)
}

/// "12.34 MB", "(unknown)" below 0.
fn size_text(n: i64) -> String {
    if n < 0 {
        "(unknown)".into()
    } else {
        fmt::size(n)
    }
}

fn length(s: i64) -> String {
    let (h, m, x) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{x:02}")
    } else {
        format!("{m}:{x:02}")
    }
}

fn basename(p: &str) -> String {
    std::path::Path::new(p)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn join(dir: &str, name: &str) -> String {
    std::path::Path::new(dir)
        .join(name)
        .to_string_lossy()
        .into_owned()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Address,
    Duplicate,
    Busy,
    Info,
}

/// What an open message box of this window is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asking {
    Nothing,
    /// "Cannot get information about the file … Add it anyway?"
    AddAnyway,
    /// A note; the window closes after it.
    CloseAfter,
}

/// What the probe found.
struct Probed {
    size: i64,
    resumable: bool,
    filename: String,
}

pub struct Add {
    pub page: Page,
    // The request (browser, another start, the clipboard).
    referer: String,
    parent_page: String,
    headers: Vec<(String, String)>,
    browser_name: String,
    // "Enter new address".
    pub address: String,
    pub use_auth: bool,
    pub login: String,
    pub password: String,
    focused: bool,
    // The flow.
    flow_url: String,
    auth: (String, String),
    dup: Option<(i64, bool)>,
    pub dup_choice: String,
    pub dup_remember: bool,
    replace: Option<i64>,
    probe: Option<Slot<Result<Probed, String>>>,
    media_probe: Option<Slot<Result<MediaInfo, String>>>,
    asking: Asking,
    /// A message box asked for while drawing or deciding; the window shows it.
    pending_box: Option<MsgBox>,
    // Download File Info.
    pub url: String,
    pub size: i64,
    pub resumable: Option<bool>,
    pub category: String,
    pub path: String,
    pub paths: Vec<String>,
    pub remember: bool,
    pub description: String,
    pub checksum: kit::ChecksumEdit,
    pub media: Option<MediaInfo>,
    pub kind: String,
    pub variant: Option<Variant>,
    file_name: String,
    new_category: Option<CategoryForm>,
    /// Another window took over (Download yt-dlp?): close on the next frame.
    close_now: bool,
}

impl Add {
    fn new(page: Page) -> Add {
        Add {
            page,
            referer: String::new(),
            parent_page: String::new(),
            headers: vec![],
            browser_name: String::new(),
            address: String::new(),
            use_auth: false,
            login: String::new(),
            password: String::new(),
            focused: false,
            flow_url: String::new(),
            auth: Default::default(),
            dup: None,
            dup_choice: "number".into(),
            dup_remember: false,
            replace: None,
            probe: None,
            media_probe: None,
            asking: Asking::Nothing,
            pending_box: None,
            url: String::new(),
            size: -1,
            resumable: None,
            category: "General".into(),
            path: String::new(),
            paths: vec![],
            remember: false,
            description: String::new(),
            checksum: kit::ChecksumEdit::default(),
            media: None,
            kind: "video".into(),
            variant: None,
            file_name: String::new(),
            new_category: None,
            close_now: false,
        }
    }
}

fn label() -> String {
    format!("add-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// "Enter new address to download" (Tasks → Add new download).
pub fn add_url(app: &mut App) {
    let mut body = Add::new(Page::Address);
    body.address = clipboard_url();
    app.open_dialog(Dialog::new(
        &label(),
        "Enter new address to download",
        (560.0, 300.0),
        body,
    ));
}

/// An address from another start, the browser, the clipboard or a paste: the add flow (probe, the
/// duplicate rule, Download File Info).
pub fn from_request(app: &mut App, r: Request) {
    let Request::AddUrl {
        url,
        referer,
        parent_page,
        headers,
        filename,
    } = r
    else {
        return;
    };
    let mut body = Add::new(Page::Busy);
    body.referer = referer;
    body.parent_page = parent_page;
    body.headers = headers
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
        .collect();
    body.browser_name = filename;
    body.go(app, &url);
    app.open_dialog(Dialog::new(
        &label(),
        "Download File Info",
        (720.0, 420.0),
        body,
    ));
}

impl Add {
    /// OK on the address page.
    fn ok_address(&mut self, app: &mut App) {
        let u = self.address.trim().to_string();
        if !view::is_url(&u) {
            self.focused = false;
            return;
        }
        remember_url(&u);
        self.auth = if self.use_auth {
            (self.login.clone(), self.password.clone())
        } else {
            Default::default()
        };
        self.go(app, &u);
    }

    /// The duplicate rule, then the probe.
    fn go(&mut self, app: &mut App, u: &str) {
        self.flow_url = u.to_string();
        let m = &app.manager;
        self.dup = m
            .find_duplicate(u)
            .map(|d| (d.id, d.status == Status::Complete));
        if self.dup.is_some() {
            let action = m.settings.duplicate_action.clone();
            if action == "ask" {
                self.page = Page::Duplicate;
                return;
            }
            self.after_duplicate(app, &action);
        } else {
            self.after_duplicate(app, "");
        }
    }

    fn ok_duplicate(&mut self, app: &mut App, cx: &mut Cx) {
        if self.dup_remember {
            app.manager.settings.duplicate_action = self.dup_choice.clone();
            app.save_settings();
        }
        let choice = self.dup_choice.clone();
        if self.after_duplicate(app, &choice) {
            cx.close = true;
        }
    }

    /// True when the flow ended here (the existing download was shown or started).
    fn after_duplicate(&mut self, app: &mut App, action: &str) -> bool {
        if action == "complete"
            && let Some((id, complete)) = self.dup
        {
            if complete {
                super::complete::complete(app, id);
            } else {
                app.manager.start(id);
                super::progress::open(app, id, false);
            }
            app.rebuild();
            self.page = Page::Busy;
            self.asking = Asking::Nothing;
            self.probe = None;
            self.flow_url.clear();
            return true;
        }
        self.replace = if action == "overwrite" {
            self.dup.map(|d| d.0)
        } else {
            None
        };
        self.page = Page::Busy;
        if media::is_youtube(&self.flow_url) {
            if !media::available() {
                // "Download yt-dlp?" takes over; this address is added again once yt-dlp is there.
                let request = Request::AddUrl {
                    url: self.flow_url.clone(),
                    referer: self.referer.clone(),
                    parent_page: self.parent_page.clone(),
                    headers: self
                        .headers
                        .iter()
                        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                        .collect(),
                    filename: self.browser_name.clone(),
                };
                super::update::offer_yt_dlp(app, request);
                self.close_now = true;
                return true;
            }
            let slot: Slot<Result<MediaInfo, String>> = Default::default();
            let s = slot.clone();
            let url = self.flow_url.clone();
            std::thread::spawn(move || {
                let r = media::probe(&url);
                *s.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
            });
            self.media_probe = Some(slot);
            return false;
        }
        let headers = self.headers.iter().cloned().collect();
        let fut = app.manager.probe(
            &self.flow_url,
            &self.referer,
            &self.auth.0,
            &self.auth.1,
            headers,
        );
        let slot: Slot<Result<Probed, String>> = Default::default();
        let s = slot.clone();
        app.rt.spawn(async move {
            let r = fut
                .await
                .map(|p| Probed {
                    size: p.size,
                    resumable: p.resumable,
                    filename: p.filename,
                })
                .map_err(|e| e.message());
            *s.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
        });
        self.probe = Some(slot);
        false
    }

    /// The probe answered: Download File Info (or the download at once when that window is off).
    fn probed(&mut self, app: &mut App, cx: &mut Cx, r: Result<Probed, String>) {
        match r {
            Ok(p) => {
                let name = name_choice(&self.flow_url, Some(&p.filename), &self.browser_name);
                self.size = p.size;
                self.resumable = Some(p.resumable);
                self.prepared(app, cx, &name);
            }
            Err(e) => {
                self.asking = Asking::AddAnyway;
                self.pending_box = Some(MsgBox::question(
                    "LID",
                    &format!("Cannot get information about the file:\n{e}\n\nAdd it anyway?"),
                    crate::boxes::Then::Nothing,
                ));
            }
        }
    }

    /// The category, folder and a free name (`_confirm_and_add` in 1.x), then the info page.
    fn prepared(&mut self, app: &mut App, cx: &mut Cx, name: &str) {
        let s = &app.manager.settings;
        let cat = lider_core::categories::classify(s, name, &self.flow_url).clone();
        let name = if self.replace.is_some() {
            name.to_string()
        } else {
            view::unique_name(&app.manager, &cat.save_dir, name)
        };
        self.url = self.flow_url.clone();
        self.description.clear();
        let path = join(&cat.save_dir, &name);
        if !app.manager.settings.show_start_dialog {
            let action = if app.manager.settings.only_add_to_queue {
                "later"
            } else {
                "start"
            };
            self.path = path;
            self.category = cat.name;
            let _ = self.submit(app, action, "");
            cx.close = true;
            return;
        }
        self.file_name = name;
        self.show_info(app, &cat.name, &path);
    }

    fn show_info(&mut self, app: &mut App, cat: &str, path: &str) {
        self.category = cat.to_string();
        self.path = path.to_string();
        self.rebuild_paths(app);
        if app.manager.settings.checksum_from_clipboard && self.media.is_none() {
            let text = arboard::Clipboard::new()
                .and_then(|mut c| c.get_text())
                .unwrap_or_default();
            if let Ok(v) = lider_core::checksum::parse(&text, "auto")
                && !v.is_empty()
            {
                self.checksum.text = lider_core::checksum::split(&v).1.to_string();
            }
        }
        self.page = Page::Info;
    }

    fn cat_dir(&self, app: &App, name: &str) -> String {
        let cats = &app.manager.settings.categories;
        cats.iter()
            .find(|c| c.name == name)
            .or(cats.first())
            .map(|c| c.save_dir.clone())
            .unwrap_or_default()
    }

    /// The Save As choices: the category's folder first, then the other categories' folders.
    fn rebuild_paths(&mut self, app: &App) {
        let base = {
            let b = basename(&self.path);
            if b.is_empty() {
                self.file_name.clone()
            } else {
                b
            }
        };
        let own_dir = self.cat_dir(app, &self.category);
        let own = join(&own_dir, &base);
        let mut paths = vec![own.clone()];
        for c in &app.manager.settings.categories {
            if c.name != self.category && c.save_dir != own_dir {
                let p = join(&c.save_dir, &base);
                if !paths.contains(&p) {
                    paths.push(p);
                }
            }
        }
        self.paths = paths;
        self.path = own;
    }

    fn set_category(&mut self, app: &App, name: &str) {
        self.category = name.to_string();
        self.rebuild_paths(app);
    }

    /// The YouTube page was read.
    fn media_read(&mut self, app: &mut App, r: Result<MediaInfo, String>) {
        let info = match r {
            Ok(i) => i,
            Err(e) => {
                self.asking = Asking::CloseAfter;
                self.pending_box = Some(
                    MsgBox::info(
                        "LID",
                        &format!(
                            "Cannot read this YouTube video:\n{}",
                            e.replace("ERROR: ", "")
                        ),
                    )
                    .with_icon("info", true),
                );
                return;
            }
        };
        if info.variants.is_empty() {
            self.asking = Asking::CloseAfter;
            self.pending_box = Some(
                MsgBox::info("LID", "This YouTube video has no downloadable formats.")
                    .with_icon("info", true),
            );
            return;
        }
        let s = &app.manager.settings;
        let Some(v) = info
            .default_variant(&s.media_kind, &s.media_quality)
            .cloned()
        else {
            return;
        };
        let name = media::suggested_filename(&info, &v);
        self.kind = v.kind.clone();
        self.size = v.size;
        self.resumable = None;
        self.url = info.url.clone();
        self.file_name = name.clone();
        self.variant = Some(v);
        let cat = lider_core::categories::classify(&app.manager.settings, &name, &info.url).clone();
        self.media = Some(info);
        self.show_info(app, &cat.name, &join(&cat.save_dir, &name));
        let target = if self.kind == "video" {
            "Video"
        } else {
            "Music"
        };
        if app
            .manager
            .settings
            .categories
            .iter()
            .any(|c| c.name == target)
        {
            self.set_category(app, target);
        }
    }

    fn set_kind(&mut self, app: &App, k: &str) {
        self.kind = k.to_string();
        if let Some(info) = &self.media {
            self.variant = info
                .default_variant(k, "")
                .cloned()
                .or_else(|| info.variants.iter().find(|v| v.kind == k).cloned());
        }
        let cat = if k == "video" { "Video" } else { "Music" };
        if app
            .manager
            .settings
            .categories
            .iter()
            .any(|c| c.name == cat)
        {
            self.category = cat.to_string();
        }
        self.quality(app);
    }

    fn quality(&mut self, app: &App) {
        let Some(v) = &self.variant else { return };
        self.size = v.size;
        let stem = match self.path.rfind('.') {
            Some(i) if !self.path[i..].contains('/') => self.path[..i].to_string(),
            _ => self.path.clone(),
        };
        self.path = format!("{stem}.{}", v.ext);
        self.rebuild_paths(app);
    }

    /// Add the download as the window decided (`add_download`); it starts with its progress window
    /// unless "later".
    pub fn submit(&mut self, app: &mut App, action: &str, queue: &str) -> Result<i64, String> {
        let path = std::path::Path::new(self.path.trim());
        let filename = path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .ok_or("no file name")?;
        let save_dir = path
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let checksum = if self.media.is_some() {
            String::new()
        } else {
            self.checksum.value()?
        };
        let start = action == "start";
        let full = self.path.trim().to_string();
        let m = &mut app.manager;
        if self.remember {
            m.settings.category_mut(&self.category).save_dir = save_dir.clone();
        }
        if let Some(v) = &self.variant {
            m.settings.media_kind = v.kind.clone();
            m.settings.media_quality = v.quality.clone();
        }
        if let Some(old) = self.replace {
            let same = m
                .downloads
                .get(&old)
                .is_some_and(|d| d.status == Status::Complete && d.path() == full);
            m.remove(&[old], same);
        }
        let media_record = match (&self.media, &self.variant) {
            (Some(i), Some(v)) => Some(media::media_record(i, v)),
            _ => None,
        };
        let youtube = self.media.is_some();
        let d = m.add(
            &self.url,
            &filename,
            lider_core::manager::NewDownload {
                save_dir: Some(save_dir),
                category: Some(self.category.clone()),
                description: self.description.clone(),
                referer: if youtube {
                    String::new()
                } else {
                    self.referer.clone()
                },
                parent_page: if youtube {
                    self.url.clone()
                } else {
                    self.parent_page.clone()
                },
                username: self.auth.0.clone(),
                password: self.auth.1.clone(),
                size: Some(self.size),
                resumable: self.resumable,
                queue: if action == "later" {
                    queue.to_string()
                } else {
                    String::new()
                },
                start,
                media: media_record,
                headers: if youtube {
                    Default::default()
                } else {
                    self.headers.iter().cloned().collect()
                },
                checksum,
            },
        );
        app.save_settings();
        if start {
            super::progress::open(app, d.id, false);
        }
        app.rebuild();
        Ok(d.id)
    }
}

impl Add {
    /// Show the message box asked for (a window has one at a time).
    fn flush_box(&mut self, cx: &mut Cx) {
        if let Some(b) = self.pending_box.take() {
            *cx.boxx = Some(b);
        }
    }
}

impl Body for Add {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        if self.close_now {
            cx.close = true;
            return;
        }
        if let Some(r) = take(&self.probe) {
            self.probe = None;
            self.probed(app, cx, r);
        }
        if let Some(r) = take(&self.media_probe) {
            self.media_probe = None;
            self.media_read(app, r);
        }
        if self.probe.is_some() || self.media_probe.is_some() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(form) = &mut self.new_category {
            match form.ui(ui, cx) {
                Some(Some(c)) => {
                    let name = c.name.clone();
                    self.new_category = None;
                    if super::small::save_category(app, None, c) {
                        self.set_category(app, &name);
                    }
                }
                Some(None) => self.new_category = None,
                None => {}
            }
            self.flush_box(cx);
            return;
        }
        match self.page {
            Page::Address => {
                cx.title = Some("Enter new address to download".into());
                self.address_page(app, ui, cx);
            }
            Page::Duplicate => {
                cx.title = Some("Duplicate download link".into());
                self.duplicate_page(app, ui, cx);
            }
            Page::Busy => {
                // Tall enough for the message box that may come over it.
                kit::body(ui, |ui| {
                    ui.set_min_height(240.0);
                    ui.add_space(100.0);
                    ui.vertical_centered(|ui| kit::muted(ui, &cx.p, "Reading the address…"));
                });
                if cx.escape(ui) {
                    cx.close = true;
                }
            }
            Page::Info => {
                cx.title = Some("Download File Info".into());
                self.info_page(app, ui, cx);
            }
        }
        self.flush_box(cx);
    }

    fn answer(&mut self, app: &mut App, a: Answer, cx: &mut Cx) {
        let asking = std::mem::replace(&mut self.asking, Asking::Nothing);
        match asking {
            Asking::AddAnyway if a.button == "yes" => {
                let name = name_choice(&self.flow_url, None, &self.browser_name);
                self.size = -1;
                self.resumable = None;
                self.prepared(app, cx, &name);
            }
            Asking::AddAnyway | Asking::CloseAfter => cx.close = true,
            Asking::Nothing => {}
        }
    }
    any_body!();
}

impl Add {
    fn address_page(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let mut ok = false;
        kit::body(ui, |ui| {
            kit::header(
                ui,
                &p,
                &Head::new(
                    "add-url",
                    "Enter new address to download",
                    "Paste a link. LID reads the file name and size next.",
                    Badge::None,
                ),
                |_| {},
            );
            kit::row(ui, "Address", 70.0, |ui, by| {
                let w = ui.available_width() - 34.0;
                let r = ui
                    .add(
                        egui::TextEdit::singleline(&mut self.address)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(w),
                    )
                    .labelled_by(by);
                if !self.focused {
                    r.request_focus();
                    self.focused = true;
                }
                let arrow = ui
                    .add(egui::Button::image(icon("down-arrow", 12.0, &p, p.mu)))
                    .on_hover_text("Earlier addresses");
                egui::Popup::menu(&arrow).show(|ui| {
                    let h = history();
                    if h.is_empty() {
                        ui.label(RichText::new("No earlier addresses").color(p.fa));
                    }
                    for u in h {
                        if ui.button(&u).clicked() {
                            self.address = u;
                            ui.close();
                        }
                    }
                });
            });
            if media::is_youtube(self.address.trim()) {
                kit::note(
                    ui,
                    &p,
                    "YouTube video detected — choose the resolution or audio only in the next window.",
                );
            } else {
                kit::panel(ui, &p, |ui| {
                    ui.checkbox(&mut self.use_auth, "Use authorization");
                    ui.horizontal(|ui| {
                        let on = self.use_auth;
                        let half = (ui.available_width() - 140.0) / 2.0;
                        let l = ui.label("Login").id;
                        ui.add_enabled(
                            on,
                            egui::TextEdit::singleline(&mut self.login).desired_width(half),
                        )
                        .labelled_by(l);
                        let l = ui.label("Password").id;
                        ui.add_enabled(
                            on,
                            egui::TextEdit::singleline(&mut self.password)
                                .password(true)
                                .desired_width(half),
                        )
                        .labelled_by(l);
                    });
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
        if ok || cx.enter(ui) {
            self.ok_address(app);
        }
        if cx.escape(ui) {
            cx.close = true;
        }
    }

    fn duplicate_page(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let mut ok = false;
        let url = self.flow_url.clone();
        kit::body(ui, |ui| {
            kit::header(
                ui,
                &p,
                &Head::new(
                    "queue",
                    "Duplicate download link",
                    "This file is already in your download list.",
                    Badge::None,
                ),
                |_| {},
            );
            ui.horizontal(|ui| {
                ui.add(icon("link", 15.0, &p, p.mu));
                let by = ui.label("Address").id;
                kit::readonly(ui, &url, by);
            });
            kit::panel(ui, &p, |ui| {
                for (v, l) in [
                    ("number", "Add the duplicate with a numbered file name"),
                    (
                        "overwrite",
                        "Add the duplicate and overwrite the existing file",
                    ),
                    (
                        "complete",
                        "If existing file is complete, show download complete dialog; otherwise resume it",
                    ),
                ] {
                    ui.radio_value(&mut self.dup_choice, v.to_string(), l);
                }
            });
            ui.checkbox(
                &mut self.dup_remember,
                "Remember my selection and don't ask again (change it in Options → Downloads)",
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
            self.ok_duplicate(app, cx);
        }
        if cx.escape(ui) {
            cx.close = true;
        }
    }

    fn info_page(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let mut start = false;
        let mut later: Option<String> = None;
        let valid = self.checksum.valid();
        let cats: Vec<(String, String)> = app
            .manager
            .settings
            .categories
            .iter()
            .map(|c| (c.name.clone(), c.name.clone()))
            .collect();
        let icon_name = cat_icon(&self.category);
        let subtitle = basename(&self.path);
        kit::body(ui, |ui| {
            kit::header(
                ui,
                &p,
                &Head::new(&icon_name, "Download File Info", &subtitle, Badge::Tile),
                |_| {},
            );
            ui.horizontal_top(|ui| {
                let left = ui.available_width() - 130.0;
                ui.vertical(|ui| {
                    ui.set_width(left);
                    kit::row(ui, "URL", 90.0, |ui, by| {
                        ui.add_enabled(
                            self.media.is_none(),
                            egui::TextEdit::singleline(&mut self.url)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY),
                        )
                        .labelled_by(by);
                    });
                    kit::row(ui, "Category", 90.0, |ui, by| {
                        let mut c = self.category.clone();
                        if kit::combo_by(ui, "add-category", 200.0, &mut c, &cats, by) {
                            self.set_category(app, &c);
                        }
                        kit::right(ui, |ui| {
                            if ui.button("+").on_hover_text("Add category").clicked() {
                                self.new_category = Some(CategoryForm::new(None));
                            }
                        });
                    });
                    kit::row(ui, "Save As", 90.0, |ui, by| {
                        let w = ui.available_width() - 80.0;
                        ui.add(
                            egui::TextEdit::singleline(&mut self.path)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(w),
                        )
                        .labelled_by(by);
                        let arrow = ui
                            .add(egui::Button::image(icon("down-arrow", 12.0, &p, p.mu)))
                            .on_hover_text("The folders of the categories");
                        egui::Popup::menu(&arrow).show(|ui| {
                            for x in self.paths.clone() {
                                if ui.button(&x).clicked() {
                                    self.path = x;
                                    ui.close();
                                }
                            }
                        });
                        if ui.button("...").on_hover_text("Save As").clicked() {
                            let dir = std::path::Path::new(&self.path)
                                .parent()
                                .map(|d| d.to_path_buf())
                                .unwrap_or_default();
                            if let Some(f) = rfd::FileDialog::new()
                                .set_title("Save As")
                                .set_directory(dir)
                                .set_file_name(basename(&self.path))
                                .save_file()
                            {
                                self.path = f.to_string_lossy().into_owned();
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.add_space(98.0);
                        ui.checkbox(
                            &mut self.remember,
                            format!("Remember this path for \"{}\" category", self.category),
                        );
                    });
                    if self.media.is_some() {
                        self.media_panel(app, ui, cx);
                    }
                    kit::row(ui, "Description", 90.0, |ui, by| {
                        kit::field(ui, &mut self.description, by);
                    });
                    if self.media.is_none() {
                        ui.horizontal_top(|ui| {
                            let by = ui
                                .allocate_ui_with_layout(
                                    egui::vec2(90.0, 22.0),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.set_min_width(90.0);
                                        ui.label("Checksum")
                                    },
                                )
                                .inner
                                .id;
                            ui.vertical(|ui| {
                                self.checksum.ui(ui, &p, true, by);
                            });
                        });
                    }
                });
                ui.vertical_centered(|ui| {
                    ui.set_width(116.0);
                    kit::badge_icon(ui, &p, &icon_name, Badge::Tile);
                    let approx = self.variant.as_ref().is_some_and(|v| v.convert == "mp3");
                    ui.label(
                        RichText::new(format!(
                            "{}{}",
                            if approx { "~" } else { "" },
                            size_text(self.size)
                        ))
                        .monospace()
                        .strong(),
                    );
                    match self.resumable {
                        Some(true) => {
                            kit::pill(ui, &p, "✓ Resumable", "ok");
                        }
                        Some(false) => {
                            kit::pill(ui, &p, "Not resumable", "muted");
                        }
                        None => {}
                    }
                });
            });
        });
        let queues = app.view.queues.clone();
        kit::footer(ui, &p, |ui| {
            kit::right(ui, |ui| {
                if kit::plain(ui, &p, "Cancel") {
                    cx.close = true;
                }
                let b = egui::Button::image_and_text(
                    icon("add-white", 15.0, &p, egui::Color32::WHITE),
                    RichText::new("Start Download")
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(p.ac)
                .min_size(egui::vec2(76.0, 28.0));
                start |= ui.add_enabled(valid, b).clicked();
                let lb = egui::Button::image_and_text(
                    icon("queue", 15.0, &p, p.tx),
                    RichText::new("Download Later").strong(),
                )
                .min_size(egui::vec2(76.0, 28.0));
                let lr = ui.add_enabled(valid, lb);
                egui::Popup::menu(&lr).show(|ui| {
                    for q in &queues {
                        if ui.button(q).clicked() {
                            later = Some(q.clone());
                            ui.close();
                        }
                    }
                });
            });
        });
        if valid && cx.enter(ui) {
            start = true;
        }
        if cx.escape(ui) {
            cx.close = true;
        }
        let action = if start {
            Some(("start", String::new()))
        } else {
            later.map(|q| ("later", q))
        };
        if let Some((action, queue)) = action {
            if self.path.trim().is_empty() {
                return;
            }
            match self.submit(app, action, &queue) {
                Ok(_) => cx.close = true,
                Err(e) => self.pending_box = Some(MsgBox::info("LID", &e)),
            }
        }
    }

    fn media_panel(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let Some(info) = self.media.clone() else {
            return;
        };
        kit::panel(ui, &p, |ui| {
            kit::caption(ui, &p, "YouTube video");
            ui.add(egui::Label::new(RichText::new(&info.title).strong()).wrap());
            let by: Vec<String> = [
                info.uploader.clone(),
                if info.duration > 0 {
                    length(info.duration)
                } else {
                    String::new()
                },
            ]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
            kit::muted(ui, &p, &by.join(" · "));
            ui.horizontal(|ui| {
                let has = |k: &str| info.variants.iter().any(|v| v.kind == k);
                for (k, l) in [("video", "Video"), ("audio", "Audio only")] {
                    let on = self.kind == k;
                    if ui
                        .add_enabled(has(k), egui::RadioButton::new(on, l))
                        .clicked()
                        && !on
                    {
                        self.set_kind(app, k);
                    }
                }
                ui.add_space(10.0);
                let by = ui.label("Quality").id;
                let current = self
                    .variant
                    .as_ref()
                    .map(|v| v.label.clone())
                    .unwrap_or_default();
                let mut pick = None;
                let r = egui::ComboBox::from_id_salt("add-quality")
                    .width(ui.available_width())
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for v in info.variants.iter().filter(|v| v.kind == self.kind) {
                            if ui
                                .selectable_label(self.variant.as_ref() == Some(v), &v.label)
                                .clicked()
                            {
                                pick = Some(v.clone());
                            }
                        }
                    });
                r.response.labelled_by(by);
                if let Some(v) = pick {
                    self.variant = Some(v);
                    self.quality(app);
                }
            });
        });
    }
}

// ---------------------------------------------------------------------- Batch download

pub struct Batch {
    pub address: String,
    pub letters: bool,
    pub num_from: u32,
    pub num_to: u32,
    pub width: usize,
    pub let_from: String,
    pub let_to: String,
    pub use_auth: bool,
    pub login: String,
    pub password: String,
    /// Added, asking whether to start the Main queue.
    asked: bool,
}

/// "Batch download".
pub fn batch(app: &mut App) {
    let body = Batch {
        address: clipboard_url(),
        letters: false,
        num_from: 1,
        num_to: 10,
        width: 1,
        let_from: "a".into(),
        let_to: "z".into(),
        use_auth: false,
        login: String::new(),
        password: String::new(),
        asked: false,
    };
    app.open_dialog(Dialog::new("batch", "Batch download", (620.0, 520.0), body));
}

impl Batch {
    pub fn urls(&self) -> Vec<String> {
        let fill = if self.letters {
            Fill::Letters {
                from: self.let_from.clone(),
                to: self.let_to.clone(),
            }
        } else {
            Fill::Numbers {
                from: self.num_from.min(999_999),
                to: self.num_to.min(999_999),
                width: self.width.clamp(1, 9),
            }
        };
        lider_core::batch::expand(&self.address, &fill)
    }
}

impl Body for Batch {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let urls = self.urls();
        let mut ok = false;
        kit::body(ui, |ui| {
            kit::header(
                ui,
                &p,
                &Head::new(
                    "add-url",
                    "Batch download",
                    "Add a numbered series of files: put one asterisk in the address, e.g. https://www.example.com/pictures/img*.jpg",
                    Badge::None,
                ),
                |_| {},
            );
            kit::row(ui, "Address", 70.0, |ui, by| {
                kit::mono_field(ui, &mut self.address, by);
            });
            kit::panel(ui, &p, |ui| {
                kit::caption(ui, &p, "Replace asterisk with");
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 20.0;
                    ui.radio_value(&mut self.letters, false, "Numbers");
                    ui.radio_value(&mut self.letters, true, "Letters");
                });
                ui.horizontal(|ui| {
                    let l = ui.label("From").id;
                    if self.letters {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.let_from)
                                .char_limit(1)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(70.0),
                        )
                        .labelled_by(l);
                    } else {
                        ui.add(egui::DragValue::new(&mut self.num_from).range(0..=999_999))
                            .labelled_by(l);
                    }
                    let l = ui.label("To").id;
                    if self.letters {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.let_to)
                                .char_limit(1)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(70.0),
                        )
                        .labelled_by(l);
                    } else {
                        ui.add(egui::DragValue::new(&mut self.num_to).range(0..=999_999))
                            .labelled_by(l);
                    }
                    let l = ui.label("Wildcard size").id;
                    ui.add_enabled(
                        !self.letters,
                        egui::DragValue::new(&mut self.width).range(1..=9),
                    )
                    .labelled_by(l);
                });
            });
            ui.checkbox(&mut self.use_auth, "Use authorization");
            if self.use_auth {
                ui.horizontal(|ui| {
                    let half = (ui.available_width() - 140.0) / 2.0;
                    let l = ui.label("Login").id;
                    ui.add(egui::TextEdit::singleline(&mut self.login).desired_width(half))
                        .labelled_by(l);
                    let l = ui.label("Password").id;
                    ui.add(
                        egui::TextEdit::singleline(&mut self.password)
                            .password(true)
                            .desired_width(half),
                    )
                    .labelled_by(l);
                });
            }
            kit::panel(ui, &p, |ui| {
                let last = urls.last().cloned().unwrap_or_default();
                for (l, u) in [
                    ("First file", urls.first().cloned().unwrap_or_default()),
                    ("Second file", urls.get(1).cloned().unwrap_or_default()),
                    ("Last file", last),
                ] {
                    kit::row(ui, l, 80.0, |ui, _| {
                        ui.label(RichText::new(u).monospace().color(p.actx));
                    });
                }
            });
        });
        kit::footer(ui, &p, |ui| {
            kit::muted(
                ui,
                &p,
                &if urls.is_empty() {
                    "Put one * in the address".to_string()
                } else {
                    format!("{} files will be added", urls.len())
                },
            );
            kit::right(ui, |ui| {
                ok |= kit::primary(ui, &p, "OK");
                if kit::plain(ui, &p, "Cancel") {
                    cx.close = true;
                }
            });
        });
        if (ok || cx.enter(ui)) && !self.asked {
            if urls.is_empty() {
                cx.close = true;
                return;
            }
            let items: Vec<_> = urls
                .into_iter()
                .map(|u| (u, String::new(), String::new()))
                .collect();
            let (login, password) = if self.use_auth {
                (self.login.as_str(), self.password.as_str())
            } else {
                ("", "")
            };
            let n = view::add_many(&mut app.manager, &items, login, password);
            app.rebuild();
            if n == 0 {
                cx.close = true;
                return;
            }
            self.asked = true;
            *cx.boxx = Some(
                MsgBox::question(
                    "LID",
                    &format!(
                        "{n} file(s) were added to the \"{MAIN_QUEUE}\". Start the queue now?"
                    ),
                    crate::boxes::Then::Nothing,
                )
                .with_buttons(vec![
                    Button {
                        id: "yes",
                        label: "Yes",
                        primary: true,
                        danger: false,
                        left: false,
                    },
                    Button {
                        id: "no",
                        label: "No",
                        primary: false,
                        danger: false,
                        left: false,
                    },
                ]),
            );
            return;
        }
        if cx.escape(ui) {
            cx.close = true;
        }
    }

    fn answer(&mut self, app: &mut App, a: Answer, cx: &mut Cx) {
        if a.button == "yes" {
            let events = app.manager.start_queue(MAIN_QUEUE);
            app.handle_events(&events);
            app.rebuild();
        }
        cx.close = true;
    }
    any_body!();
}
