//! Message boxes drawn over the window (QMessageBox and QInputDialog in 1.x): a question, a note, a
//! text input. The answer comes back to the app as a [`Then`].

use egui::{Align, Key, Layout, RichText};

use crate::icons::icon;
use crate::theme::{Palette, alpha};

/// What to do with the answer.
#[derive(Debug, Clone, PartialEq)]
pub enum Then {
    Nothing,
    /// Remove these downloads; the check box also deletes their files.
    Remove(Vec<i64>),
    DeleteCompleted,
    StartMainQueue,
    DeleteCategory(String),
    OpenWith(i64),
    /// Closing while downloading: `stop`, `background` or `keep`.
    CloseChoice,
    /// The power-off countdown: Cancel stops it, the end of the count turns the computer off.
    PowerOff,
    /// "Add LID to the application menu?"
    OfferMenu,
}

#[derive(Debug, Clone, Default)]
pub struct Button {
    pub id: &'static str,
    pub label: &'static str,
    pub primary: bool,
    /// Red, on the left (a button that stops something).
    pub danger: bool,
}

#[derive(Debug, Clone)]
pub struct MsgBox {
    pub title: String,
    pub text: String,
    pub icon: &'static str,
    /// The header badge is red (deletions, errors).
    pub bad: bool,
    pub buttons: Vec<Button>,
    pub checkbox: Option<(String, bool)>,
    pub input: Option<String>,
    /// A short list under the text: (category icon, name, progress), and "and N more".
    pub items: Vec<(String, String, String)>,
    pub more: Option<String>,
    /// A smaller line under everything else.
    pub note: Option<String>,
    pub then: Then,
    focused: bool,
}

pub struct Answer {
    pub button: &'static str,
    pub checked: bool,
    pub value: String,
    pub then: Then,
}

const OK: Button = Button {
    id: "ok",
    label: "OK",
    primary: true,
    danger: false,
};
const YES: Button = Button {
    id: "yes",
    label: "Yes",
    primary: true,
    danger: false,
};
const NO: Button = Button {
    id: "no",
    label: "No",
    primary: false,
    danger: false,
};

impl MsgBox {
    pub fn info(title: &str, text: &str) -> MsgBox {
        MsgBox {
            title: title.into(),
            text: text.into(),
            icon: "info",
            bad: false,
            buttons: vec![OK],
            checkbox: None,
            input: None,
            items: vec![],
            more: None,
            note: None,
            then: Then::Nothing,
            focused: false,
        }
    }
    pub fn question(title: &str, text: &str, then: Then) -> MsgBox {
        MsgBox {
            buttons: vec![YES, NO],
            then,
            ..MsgBox::info(title, text)
        }
    }
    pub fn input(title: &str, text: &str, then: Then) -> MsgBox {
        MsgBox {
            buttons: vec![
                OK,
                Button {
                    id: "cancel",
                    label: "Cancel",
                    primary: false,
                    danger: false,
                },
            ],
            input: Some(String::new()),
            then,
            ..MsgBox::info(title, text)
        }
    }
    pub fn with_icon(mut self, icon: &'static str, bad: bool) -> MsgBox {
        self.icon = icon;
        self.bad = bad;
        self
    }
    pub fn with_buttons(mut self, buttons: Vec<Button>) -> MsgBox {
        self.buttons = buttons;
        self
    }
    pub fn with_note(mut self, note: &str) -> MsgBox {
        self.note = Some(note.into());
        self
    }
    pub fn with_checkbox(mut self, label: &str) -> MsgBox {
        self.checkbox = Some((label.into(), false));
        self
    }

    /// Draw the box; the answer once a button, Enter (the primary button) or Escape (cancel) ends it.
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette) -> Option<Answer> {
        let mut pressed: Option<&'static str> = None;
        // The input field is named after the text (screen readers, the tests).
        let mut text_id = egui::Id::NULL;
        let modal = egui::Modal::new(egui::Id::unique("lid-box")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.horizontal_top(|ui| {
                let tint = if self.bad { p.bad } else { p.ac };
                let (r, _) = ui.allocate_exact_size(egui::vec2(42.0, 42.0), egui::Sense::hover());
                ui.painter()
                    .circle_filled(r.center(), 21.0, alpha(tint, 0.13));
                icon(self.icon, 22.0, p, p.tx).paint_at(
                    ui,
                    egui::Rect::from_center_size(r.center(), egui::vec2(22.0, 22.0)),
                );
                text_id = ui
                    .vertical(|ui| {
                        ui.label(RichText::new(&self.title).size(16.0).strong());
                        ui.label(&self.text).id
                    })
                    .inner;
            });
            for (cat, name, done) in &self.items {
                ui.horizontal(|ui| {
                    ui.add_space(52.0);
                    icon(cat, 16.0, p, p.mu).paint_at(
                        ui,
                        egui::Rect::from_min_size(ui.cursor().min, egui::vec2(16.0, 16.0)),
                    );
                    ui.add_space(20.0);
                    ui.label(name);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(done).color(p.mu).monospace());
                    });
                });
            }
            if let Some(more) = &self.more {
                ui.horizontal(|ui| {
                    ui.add_space(52.0);
                    ui.label(RichText::new(more).color(p.mu));
                });
            }
            if let Some(note) = &self.note {
                ui.add_space(4.0);
                ui.label(RichText::new(note).small().color(p.mu));
            }
            if let Some(value) = &mut self.input {
                let r = ui
                    .add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY))
                    .labelled_by(text_id);
                if !self.focused {
                    r.request_focus();
                    self.focused = true;
                }
            }
            if let Some((label, on)) = &mut self.checkbox {
                ui.checkbox(on, label.as_str());
            }
            ui.add_space(6.0);
            let button = |ui: &mut egui::Ui, b: &Button| {
                let text = RichText::new(b.label)
                    .strong()
                    .color(if b.primary || b.danger {
                        egui::Color32::WHITE
                    } else {
                        p.tx
                    });
                let mut button = egui::Button::new(text);
                if b.primary {
                    button = button.fill(p.ac);
                } else if b.danger {
                    button = button.fill(p.bad);
                }
                ui.add(button.min_size(egui::vec2(70.0, 28.0))).clicked()
            };
            ui.horizontal(|ui| {
                for b in self.buttons.iter().filter(|b| b.danger) {
                    if button(ui, b) {
                        pressed = Some(b.id);
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    for b in self.buttons.iter().filter(|b| !b.danger).rev() {
                        if button(ui, b) {
                            pressed = Some(b.id);
                        }
                    }
                });
            });
        });
        if pressed.is_none() {
            if ctx.input(|i| i.key_pressed(Key::Enter)) {
                pressed = self
                    .buttons
                    .iter()
                    .find(|b| b.primary)
                    .or(self.buttons.first())
                    .map(|b| b.id);
            } else if modal.should_close() {
                pressed = Some("cancel");
            }
        }
        pressed.map(|button| Answer {
            button,
            checked: self.checkbox.as_ref().is_some_and(|c| c.1),
            value: self.input.clone().unwrap_or_default(),
            then: self.then.clone(),
        })
    }
}
