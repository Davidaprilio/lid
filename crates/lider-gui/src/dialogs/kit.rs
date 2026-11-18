//! The building blocks of the dialogs (1.x `widgets.py`, the Tauri build's `components/form`): the shell
//! of every dialog (header, body, footer), buttons, form rows, pills, the tab strip, notes, the checksum
//! field and the tag field.

use egui::{
    Align, Color32, CornerRadius, FontId, Layout, Margin, RichText, Sense, Stroke, Ui, vec2,
};

use crate::icons::icon;
use crate::theme::{Palette, alpha};

/// The circle or tile behind the header icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Badge {
    /// The icon alone, larger.
    None,
    Accent,
    Bad,
    Ok,
    /// A rounded square (categories, files).
    Tile,
}

pub struct Head<'a> {
    pub icon: &'a str,
    pub title: &'a str,
    pub subtitle: &'a str,
    pub badge: Badge,
}

impl<'a> Head<'a> {
    pub fn new(icon: &'a str, title: &'a str, subtitle: &'a str, badge: Badge) -> Head<'a> {
        Head {
            icon,
            title,
            subtitle,
            badge,
        }
    }
}

/// The header icon in its badge.
pub fn badge_icon(ui: &mut Ui, p: &Palette, name: &str, badge: Badge) {
    let (size, glyph) = if badge == Badge::None {
        (34.0, 30.0)
    } else {
        (42.0, 22.0)
    };
    let (r, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let tint = match badge {
        Badge::Bad => p.bad,
        Badge::Ok => p.ok,
        _ => p.ac,
    };
    match badge {
        Badge::None => {}
        Badge::Tile => {
            ui.painter()
                .rect_filled(r, CornerRadius::same(10), alpha(p.ac, 0.12));
        }
        _ => {
            ui.painter()
                .circle_filled(r.center(), size / 2.0, alpha(tint, 0.13));
        }
    }
    let ink = match badge {
        Badge::Bad => p.bad,
        Badge::Ok => p.ok,
        _ => p.tx,
    };
    icon(name, glyph, p, ink).paint_at(
        ui,
        egui::Rect::from_center_size(r.center(), vec2(glyph, glyph)),
    );
}

/// The title row of a dialog: icon, title, subtitle; `extra` draws at the right end (a pill).
pub fn header(ui: &mut Ui, p: &Palette, head: &Head, extra: impl FnOnce(&mut Ui)) {
    ui.horizontal_top(|ui| {
        if !head.icon.is_empty() {
            badge_icon(ui, p, head.icon, head.badge);
        }
        ui.vertical(|ui| {
            ui.add_space(2.0);
            ui.label(RichText::new(head.title).size(16.0).strong());
            if !head.subtitle.is_empty() {
                ui.add(egui::Label::new(RichText::new(head.subtitle).color(p.mu)).wrap());
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Min), extra);
    });
    ui.add_space(4.0);
}

/// The body of a dialog, with 1.x's margins (18/16/18/14).
pub fn body<R>(ui: &mut Ui, f: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .inner_margin(Margin {
            left: 18,
            right: 18,
            top: 16,
            bottom: 14,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            f(ui)
        })
        .inner
}

/// The footer bar. What `f` adds starts at the left edge; put the right-hand buttons in [`right`].
pub fn footer<R>(ui: &mut Ui, p: &Palette, f: impl FnOnce(&mut Ui) -> R) -> R {
    let r = ui.available_rect_before_wrap();
    ui.painter()
        .hline(r.x_range(), r.top(), Stroke::new(1.0, p.line));
    egui::Frame::new()
        .fill(p.hdr)
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(f).inner
        })
        .inner
}

/// The right end of a footer: the first button added is the rightmost (add OK, then Cancel).
pub fn right<R>(ui: &mut Ui, f: impl FnOnce(&mut Ui) -> R) -> R {
    ui.with_layout(Layout::right_to_left(Align::Center), f)
        .inner
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Normal,
    Primary,
    Danger,
}

/// A dialog button (1.x `primary()` for the accent one).
pub fn button(ui: &mut Ui, p: &Palette, label: &str, look: Look, enabled: bool) -> bool {
    let colour = match look {
        Look::Normal => p.tx,
        _ => Color32::WHITE,
    };
    let mut b =
        egui::Button::new(RichText::new(label).strong().color(colour)).min_size(vec2(76.0, 28.0));
    b = match look {
        Look::Primary => b.fill(p.ac),
        Look::Danger => b.fill(p.bad),
        Look::Normal => b,
    };
    ui.add_enabled(enabled, b).clicked()
}

pub fn primary(ui: &mut Ui, p: &Palette, label: &str) -> bool {
    button(ui, p, label, Look::Primary, true)
}

pub fn plain(ui: &mut Ui, p: &Palette, label: &str) -> bool {
    button(ui, p, label, Look::Normal, true)
}

/// A button with an icon before its label.
pub fn icon_button(ui: &mut Ui, p: &Palette, name: &str, label: &str, look: Look) -> bool {
    let ink = if look == Look::Normal {
        p.tx
    } else {
        Color32::WHITE
    };
    let mut b = egui::Button::image_and_text(
        icon(name, 14.0, p, ink),
        RichText::new(label).strong().color(ink),
    )
    .min_size(vec2(76.0, 28.0));
    b = match look {
        Look::Primary => b.fill(p.ac),
        Look::Danger => b.fill(p.bad),
        Look::Normal => b,
    };
    ui.add(b).clicked()
}

/// A form row: a label column of `width`, then the controls. The closure gets the label's id: fields
/// name themselves after it (`.labelled_by(id)`) for screen readers and the tests.
pub fn row<R>(ui: &mut Ui, label: &str, width: f32, f: impl FnOnce(&mut Ui, egui::Id) -> R) -> R {
    ui.horizontal(|ui| {
        // The label at the left of a fixed column (add_sized would centre it).
        let l = ui
            .allocate_ui_with_layout(
                vec2(width, 22.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.set_min_width(width);
                    ui.label(label)
                },
            )
            .inner;
        f(ui, l.id)
    })
    .inner
}

/// A small muted caption over a field; its id names the field (`.labelled_by`).
pub fn caption(ui: &mut Ui, p: &Palette, text: &str) -> egui::Id {
    ui.label(RichText::new(text).small().strong().color(p.mu))
        .id
}

/// A muted line.
pub fn muted(ui: &mut Ui, p: &Palette, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).color(p.mu)).wrap());
}

pub fn faint(ui: &mut Ui, p: &Palette, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).small().color(p.fa)).wrap());
}

pub fn error_text(ui: &mut Ui, p: &Palette, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).small().color(p.bad)).wrap());
}

/// A note in a tinted box (1.x `note()`).
pub fn note(ui: &mut Ui, p: &Palette, text: &str) {
    egui::Frame::new()
        .fill(alpha(p.ac, 0.07))
        .stroke(Stroke::new(1.0, alpha(p.ac, 0.25)))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(text).color(p.mu)).wrap());
        });
}

/// A panel: a framed group on the pane colour (1.x `panel()`).
pub fn panel<R>(ui: &mut Ui, p: &Palette, f: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(p.pane)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            f(ui)
        })
        .inner
}

/// The colour pair of a pill tone (`ok`, `bad`, `accent`, `muted`).
pub fn tone(p: &Palette, tone: &str) -> (Color32, Color32) {
    match tone {
        "ok" => (p.ok, alpha(p.ok, 0.13)),
        "bad" => (p.bad, alpha(p.bad, 0.12)),
        "accent" => (p.actx, alpha(p.ac, 0.13)),
        _ => (p.mu, p.track),
    }
}

/// A rounded label in a tone.
pub fn pill(ui: &mut Ui, p: &Palette, text: &str, t: &str) -> egui::Response {
    let (fg, bg) = tone(p, t);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), FontId::proportional(11.5), fg);
    let (r, resp) = ui.allocate_exact_size(vec2(galley.size().x + 16.0, 20.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(10), bg);
    ui.painter().galley(
        egui::pos2(r.left() + 8.0, r.center().y - galley.size().y / 2.0),
        galley,
        fg,
    );
    resp.on_hover_text(text)
}

/// Underlined tabs on a strip (1.x `tab_strip`); returns true when the tab changed.
pub fn tab_strip(ui: &mut Ui, p: &Palette, names: &[&str], active: &mut usize) -> bool {
    let mut changed = false;
    let r = ui.available_rect_before_wrap();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        for (i, n) in names.iter().enumerate() {
            let on = *active == i;
            let text = RichText::new(*n).color(if on { p.tx } else { p.mu });
            let text = if on { text.strong() } else { text };
            let resp = ui.add(egui::Label::new(text).sense(Sense::click()));
            if on {
                let b = resp.rect;
                ui.painter()
                    .hline(b.x_range(), b.bottom() + 5.0, Stroke::new(2.0, p.ac));
            }
            if resp.clicked() && !on {
                *active = i;
                changed = true;
            }
        }
    });
    let y = ui.min_rect().bottom() + 3.0;
    ui.painter().hline(r.x_range(), y, Stroke::new(1.0, p.line));
    ui.add_space(8.0);
    changed
}

/// A read-only monospace field (an address, a path), named by the label `by`.
pub fn readonly(ui: &mut Ui, text: &str, by: egui::Id) {
    let mut t = text.to_string();
    ui.add(
        egui::TextEdit::singleline(&mut t)
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY),
    )
    .labelled_by(by);
}

/// A text field as wide as the row, named by the label `by`.
pub fn field(ui: &mut Ui, text: &mut String, by: egui::Id) -> egui::Response {
    ui.add(egui::TextEdit::singleline(text).desired_width(f32::INFINITY))
        .labelled_by(by)
}

pub fn mono_field(ui: &mut Ui, text: &mut String, by: egui::Id) -> egui::Response {
    ui.add(
        egui::TextEdit::singleline(text)
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY),
    )
    .labelled_by(by)
}

/// A combo box over (value, label) pairs; true when the choice changed.
pub fn combo(
    ui: &mut Ui,
    id: &str,
    width: f32,
    value: &mut String,
    choices: &[(String, String)],
) -> bool {
    combo_named(ui, id, width, value, choices, None)
}

/// Pairs for [`combo`] where the value is the label.
pub fn same(names: &[&str]) -> Vec<(String, String)> {
    names
        .iter()
        .map(|n| (n.to_string(), n.to_string()))
        .collect()
}

/// A folder chooser next to a path field ("Browse..."); the chosen folder replaces the text.
pub fn folder_field(ui: &mut Ui, p: &Palette, text: &mut String, by: egui::Id) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let browse = ui.available_width() - 90.0;
        ui.add(
            egui::TextEdit::singleline(text)
                .font(egui::TextStyle::Monospace)
                .desired_width(browse.max(80.0)),
        )
        .labelled_by(by);
        if plain(ui, p, "Browse...")
            && let Some(d) = rfd::FileDialog::new()
                .set_title("Select a Folder")
                .set_directory(&*text)
                .pick_folder()
        {
            *text = d.to_string_lossy().into_owned();
            changed = true;
        }
    });
    changed
}

/// "12.3 MB (12,345,678 Bytes)".
pub fn size_with_bytes(n: i64) -> String {
    let digits = n.to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!("{} ({grouped} Bytes)", lider_core::fmt::size(n))
}

// ---------------------------------------------------------------------- the checksum field

pub const CHECKSUM_HINT: &str = "Optional — LID checks the file when the download finishes.";

/// The checksum field (1.x `widgets.ChecksumEdit`): algorithm, hash, the detected type, a message line.
#[derive(Debug, Clone)]
pub struct ChecksumEdit {
    pub algorithm: String,
    pub text: String,
    pub hint: String,
}

impl Default for ChecksumEdit {
    fn default() -> Self {
        ChecksumEdit {
            algorithm: "auto".into(),
            text: String::new(),
            hint: CHECKSUM_HINT.into(),
        }
    }
}

impl ChecksumEdit {
    /// Show a stored `algo:hex` (the algorithm goes back to Auto-detect, only the hex shows).
    pub fn with_value(stored: &str) -> ChecksumEdit {
        ChecksumEdit {
            text: lider_core::checksum::split(stored).1.to_string(),
            ..Default::default()
        }
    }

    /// `algo:hex`, "" when empty, or why the text is not a checksum.
    pub fn value(&self) -> Result<String, String> {
        lider_core::checksum::parse(&self.text, &self.algorithm).map_err(|e| e.0)
    }

    pub fn valid(&self) -> bool {
        self.value().is_ok()
    }

    /// Draw it (the hash field named by the label `by`); true when something changed.
    pub fn ui(&mut self, ui: &mut Ui, p: &Palette, enabled: bool, by: egui::Id) -> bool {
        let mut changed = false;
        let parsed = self.value();
        ui.add_enabled_ui(enabled, |ui| {
            ui.horizontal(|ui| {
                let mut choices = vec![("auto".to_string(), "Auto-detect".to_string())];
                choices.extend(lider_core::checksum::ALGORITHMS.iter().map(|(a, _)| {
                    (
                        a.to_string(),
                        lider_core::checksum::algorithm_title(a).to_string(),
                    )
                }));
                changed |= combo_by(
                    ui,
                    "checksum-algo",
                    110.0,
                    &mut self.algorithm,
                    &choices,
                    by,
                );
                let title = parsed
                    .as_ref()
                    .map(|v| lider_core::checksum::title(v))
                    .unwrap_or("");
                let room = if title.is_empty() { 0.0 } else { 80.0 };
                let mut edit = egui::TextEdit::singleline(&mut self.text)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("Paste a hash (optional)")
                    .desired_width((ui.available_width() - room).max(80.0));
                if parsed.is_err() {
                    edit = edit.text_color(p.bad);
                }
                changed |= ui.add(edit).labelled_by(by).changed();
                if !title.is_empty() {
                    pill(ui, p, title, "accent");
                }
            });
        });
        match &parsed {
            Err(e) => error_text(ui, p, e),
            Ok(_) => faint(ui, p, &self.hint),
        }
        changed
    }
}

// ---------------------------------------------------------------------- the tag field

/// Values as removable chips (1.x `widgets.TagInput`): Enter or "," adds, Backspace on an empty field
/// removes the last one, text is split on commas and spaces.
#[derive(Debug, Clone, Default)]
pub struct TagInput {
    pub values: Vec<String>,
    pub text: String,
    /// Extensions are stored without dots, in lower case; sites in lower case.
    pub extensions: bool,
}

impl TagInput {
    pub fn new(values: Vec<String>, extensions: bool) -> TagInput {
        TagInput {
            values,
            text: String::new(),
            extensions,
        }
    }

    fn norm(&self, v: &str) -> String {
        let v = v.trim().to_lowercase();
        if self.extensions {
            v.trim_start_matches('.').to_string()
        } else {
            v
        }
    }

    /// Add what is typed in the field.
    pub fn commit(&mut self) {
        let text = std::mem::take(&mut self.text);
        for part in text.split(|c: char| c == ',' || c.is_whitespace()) {
            let v = self.norm(part);
            if !v.is_empty() && !self.values.contains(&v) {
                self.values.push(v);
            }
        }
    }

    /// Draw it (the field named by the label `by`).
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        placeholder: &str,
        noun: &str,
        enabled: bool,
        by: egui::Id,
    ) {
        ui.add_enabled_ui(enabled, |ui| {
            egui::Frame::new()
                .fill(p.input)
                .stroke(Stroke::new(1.0, p.line2))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::same(6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        let mut remove = None;
                        for (i, v) in self.values.iter().enumerate() {
                            let b = egui::Button::new(RichText::new(format!("{v}  ×")).small())
                                .fill(alpha(p.ac, 0.12))
                                .corner_radius(CornerRadius::same(10));
                            if ui.add(b).on_hover_text(format!("Remove {v}")).clicked() {
                                remove = Some(i);
                            }
                        }
                        if let Some(i) = remove {
                            self.values.remove(i);
                        }
                        let was_empty = self.text.is_empty();
                        let r = ui
                            .add(
                                egui::TextEdit::singleline(&mut self.text)
                                    .hint_text(placeholder)
                                    .frame(egui::Frame::NONE)
                                    .return_key(None)
                                    .desired_width(160.0),
                            )
                            .labelled_by(by);
                        if r.changed() && self.text.contains(',') {
                            self.commit();
                        }
                        if r.has_focus() {
                            // Enter adds the tag; it does not press the dialog's OK (consumed here).
                            let enter = ui.input_mut(|i| {
                                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                            });
                            let back = ui.input(|i| i.key_pressed(egui::Key::Backspace));
                            if back && was_empty {
                                self.values.pop();
                            }
                            if enter {
                                self.commit();
                            }
                        }
                        if r.lost_focus() {
                            self.commit();
                        }
                    });
                });
            ui.horizontal(|ui| {
                faint(ui, p, "Enter or , adds · Backspace removes the last");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("Clear all").clicked() {
                        self.values.clear();
                    }
                    ui.label(
                        RichText::new(format!("{} {noun}", self.values.len()))
                            .small()
                            .color(p.fa),
                    );
                });
            });
        });
    }
}

// ---- added for the add dialogs

/// [`combo`] named by the label `by` (screen readers and the tests need a name).
pub fn combo_by(
    ui: &mut Ui,
    id: &str,
    width: f32,
    value: &mut String,
    choices: &[(String, String)],
    by: egui::Id,
) -> bool {
    combo_named(ui, id, width, value, choices, Some(by))
}

fn combo_named(
    ui: &mut Ui,
    id: &str,
    width: f32,
    value: &mut String,
    choices: &[(String, String)],
    by: Option<egui::Id>,
) -> bool {
    let shown = choices
        .iter()
        .find(|(v, _)| v == value)
        .map(|(_, l)| l.clone())
        .unwrap_or_else(|| value.clone());
    let mut changed = false;
    let r = egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (v, l) in choices {
                if ui.selectable_label(value == v, l).clicked() && value != v {
                    *value = v.clone();
                    changed = true;
                }
            }
        });
    if let Some(by) = by {
        r.response.labelled_by(by);
    }
    changed
}
