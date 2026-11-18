//! LID's dialog windows (`ui/dialogs/*.py` in 1.x, `ui/src/windows/dialogs` in the Tauri build). Each one
//! is an egui viewport, a real window of the desktop (decision G5), drawn in the same pass as the main
//! window ("immediate"), so a dialog works on the app directly, without a bridge. Under a backend without
//! windows of its own (the test harness) a dialog is drawn as an egui window over the main one.
//!
//! A dialog is a [`Dialog`]: its window (label, title, size) and a [`Body`] that draws it and decides.
//! The label names the window, so opening a dialog that is already open brings it forward instead
//! (`options`, `progress-5`). Windows take the height of what they show, like the Tauri build's `fit()`.

use std::any::Any;

use egui::{Ui, ViewportBuilder, ViewportClass, ViewportCommand, ViewportId};

use crate::app::App;
use crate::boxes::{Answer, MsgBox};
use crate::theme::Palette;

pub mod kit;

pub mod add;
pub mod browsers;
pub mod checkhash;
pub mod complete;
pub mod links;
pub mod options;
pub mod progress;
pub mod properties;
pub mod scheduler;
pub mod small;
pub mod update;

/// What a dialog's drawing asks of its window.
pub struct Cx<'a> {
    pub p: Palette,
    /// Close the window after this frame.
    pub close: bool,
    /// A new title for the window.
    pub title: Option<String>,
    /// A message box over this window (its answer comes back to [`Body::answer`]).
    pub boxx: &'a mut Option<MsgBox>,
    /// A message box is open over this window: the dialog's keys wait.
    pub blocked: bool,
}

impl Cx<'_> {
    /// Enter, unless a message box or a text area has it.
    pub fn enter(&self, ui: &Ui) -> bool {
        !self.blocked && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.is_none())
    }
    pub fn escape(&self, ui: &Ui) -> bool {
        !self.blocked && ui.input(|i| i.key_pressed(egui::Key::Escape))
    }
}

/// The inside of a dialog window.
pub trait Body: Any {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx);
    /// A message box of this window was answered.
    fn answer(&mut self, _app: &mut App, _a: Answer, _cx: &mut Cx) {}
    /// The window went away, whichever way (its own buttons, the title bar, LID closing).
    fn closed(&mut self, _app: &mut App) {}
    fn as_any(&mut self) -> &mut dyn Any;
}

/// `impl Body` boilerplate: `as_any`.
#[macro_export]
macro_rules! any_body {
    () => {
        fn as_any(&mut self) -> &mut dyn std::any::Any {
            self
        }
    };
}

pub struct Dialog {
    pub label: String,
    pub title: String,
    pub width: f32,
    pub height: f32,
    pub body: Box<dyn Body>,
    pub boxx: Option<MsgBox>,
    /// Bring the window forward on the next frame.
    focus: bool,
    /// Drawn at least once (a new dialog needs a pass even while the main window is hidden).
    pub shown: bool,
    /// The content height the window was last fitted to.
    fitted: f32,
    /// Where the window opens: centred over the main window.
    at: Option<egui::Pos2>,
    /// The narrowest the window may get (its own width up to 360 px unless set).
    min_width: Option<f32>,
}

impl Dialog {
    pub fn new(label: &str, title: &str, size: (f32, f32), body: impl Body) -> Dialog {
        Dialog {
            label: label.into(),
            title: title.into(),
            width: size.0,
            height: size.1,
            body: Box::new(body),
            boxx: None,
            focus: true,
            shown: false,
            fitted: 0.0,
            at: None,
            min_width: None,
        }
    }

    /// Keep the window at least this wide (the progress window: 600 px, as in 1.x).
    pub fn min_width(mut self, w: f32) -> Dialog {
        self.min_width = Some(w);
        self
    }

    fn id(&self) -> ViewportId {
        ViewportId::from_hash_of(("lid-dialog", &self.label))
    }
}

impl App {
    /// Open a dialog window, or bring forward the one with this label.
    pub fn open_dialog(&mut self, mut d: Dialog) {
        if let Some(open) = self.dialogs.iter_mut().find(|x| x.label == d.label) {
            open.focus = true;
            return;
        }
        if let Some(g) = self.geometry {
            d.at = Some(egui::pos2(
                g.x as f32 + (g.width as f32 - d.width) / 2.0,
                g.y as f32 + (g.height as f32 - d.height) / 2.0,
            ));
        }
        self.dialogs.push(d);
    }

    /// The dialog with this label is open.
    pub fn dialog_open(&self, label: &str) -> bool {
        self.dialogs.iter().any(|d| d.label == label)
    }

    /// Close the dialog with this label (its [`Body::closed`] runs).
    pub fn close_dialog(&mut self, label: &str) {
        if let Some(i) = self.dialogs.iter().position(|d| d.label == label) {
            let mut d = self.dialogs.remove(i);
            d.body.closed(self);
        }
    }

    /// The body of an open dialog, as its type (tests, and the progress window's completion options).
    pub fn dialog_body<T: Body>(&mut self, label: &str) -> Option<&mut T> {
        self.dialogs
            .iter_mut()
            .find(|d| d.label == label)
            .and_then(|d| d.body.as_any().downcast_mut::<T>())
    }

    /// Every dialog goes away (LID closes).
    pub fn close_all_dialogs(&mut self) {
        for mut d in std::mem::take(&mut self.dialogs) {
            d.body.closed(self);
        }
    }
}

/// Draw every open dialog (called once per frame of the main window).
pub fn show_all(app: &mut App, ctx: &egui::Context) {
    let mut open = std::mem::take(&mut app.dialogs);
    let mut keep = Vec::with_capacity(open.len());
    for mut d in open.drain(..) {
        if show(app, ctx, &mut d) {
            keep.push(d);
        } else {
            d.body.closed(app);
        }
    }
    // Dialogs opened while drawing (a dialog opening another one) come after the open ones.
    let new = std::mem::replace(&mut app.dialogs, keep);
    for d in new {
        app.open_dialog(d);
    }
}

/// One dialog's window; false when it closed.
fn show(app: &mut App, ctx: &egui::Context, d: &mut Dialog) -> bool {
    let id = d.id();
    let mut builder = ViewportBuilder::default()
        .with_title(d.title.clone())
        .with_app_id("lider")
        .with_inner_size([d.width, d.height])
        .with_min_inner_size([
            d.min_width.unwrap_or(d.width.min(360.0)),
            d.height.min(200.0),
        ]);
    if let Some(at) = d.at {
        builder = builder.with_position(at);
    }
    let p = app.palette;
    let first = !d.shown;
    d.shown = true;
    let focus = std::mem::take(&mut d.focus);
    ctx.show_viewport_immediate(id, builder, |ui, class| {
        let embedded = class == ViewportClass::EmbeddedWindow;
        if !embedded && focus && !first {
            ui.ctx().send_viewport_cmd(ViewportCommand::Focus);
        }
        let closing = !embedded && ui.input(|i| i.viewport().close_requested());
        let mut cx = Cx {
            p,
            close: closing,
            title: None,
            blocked: d.boxx.is_some(),
            boxx: &mut d.boxx,
        };
        if !embedded {
            ui.painter().rect_filled(ui.max_rect(), 0.0, p.win);
        }
        let width = if embedded {
            d.width
        } else {
            ui.available_width()
        };
        let inner = ui.vertical(|ui| {
            ui.set_width(width);
            d.body.ui(app, ui, &mut cx)
        });
        // The window takes the height of what it shows (the content's own height, not the window's).
        let h = inner.response.rect.height();
        if !embedded && (h - d.fitted).abs() > 1.0 && h > 40.0 {
            d.fitted = h;
            let w = ui
                .ctx()
                .input(|i| i.viewport().inner_rect.map_or(d.width, |r| r.width()));
            ui.ctx()
                .send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(w, h)));
        }
        if let Some(mut b) = cx.boxx.take() {
            match b.show(ui.ctx(), &p) {
                Some(a) => {
                    cx.blocked = false;
                    d.body.answer(app, a, &mut cx);
                }
                None => *cx.boxx = Some(b),
            }
        }
        if let Some(t) = cx.title.take()
            && t != d.title
        {
            d.title = t.clone();
            if !embedded {
                ui.ctx().send_viewport_cmd(ViewportCommand::Title(t));
            }
        }
        !cx.close
    })
}
