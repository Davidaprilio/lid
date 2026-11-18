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

// ---------------------------------------------------------------------- Alt mnemonics

/// A Qt label with its "&" mnemonic: the text without the "&", and the letter's char index and letter.
pub fn mnemonic(label: &str) -> (String, Option<(usize, char)>) {
    let mut text = String::new();
    let mut found = None;
    let mut chars = label.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            match chars.next() {
                Some('&') => text.push('&'),
                Some(n) => {
                    if found.is_none() {
                        found = Some((text.chars().count(), n));
                    }
                    text.push(n);
                }
                None => {}
            }
        } else {
            text.push(c);
        }
    }
    (text, found)
}

/// The label as a text, its mnemonic letter underlined while Alt is held (Qt shows it only then).
pub fn mnemonic_text(ui: &Ui, label: &str, colour: Color32) -> egui::WidgetText {
    let (text, m) = mnemonic(label);
    let alt = ui.input(|i| i.modifiers.alt);
    let style = ui.style();
    let font = egui::TextStyle::Button.resolve(style);
    let mut job = egui::text::LayoutJob::default();
    let fmt = |underline: bool| egui::TextFormat {
        font_id: font.clone(),
        color: colour,
        underline: if underline {
            Stroke::new(1.0, colour)
        } else {
            Stroke::NONE
        },
        ..Default::default()
    };
    for (i, c) in text.chars().enumerate() {
        let under = alt && m.is_some_and(|(at, _)| at == i);
        job.append(&c.to_string(), 0.0, fmt(under));
    }
    job.into()
}

/// Alt plus the label's mnemonic letter was pressed (and taken, so nothing else sees it). Only while the
/// widget's window takes input: not under a message box.
pub fn mnemonic_pressed(ui: &Ui, label: &str) -> bool {
    let Some((_, letter)) = mnemonic(label).1 else {
        return false;
    };
    if !ui.is_enabled() {
        return false;
    }
    let modal = ui.ctx().memory(|m| m.top_modal_layer());
    if modal.is_some_and(|l| l != ui.layer_id()) {
        return false;
    }
    let Some(key) = egui::Key::from_name(&letter.to_ascii_uppercase().to_string()) else {
        return false;
    };
    ui.input_mut(|i| i.consume_key(egui::Modifiers::ALT, key))
}

/// A check box whose label may carry a "&" mnemonic (Alt+letter toggles it).
pub fn check(ui: &mut Ui, on: &mut bool, label: &str) -> egui::Response {
    let colour = ui.visuals().text_color();
    let text = mnemonic_text(ui, label, colour);
    let mut r = ui.checkbox(on, text);
    if mnemonic_pressed(ui, label) {
        *on = !*on;
        r.mark_changed();
    }
    r
}

/// A dialog button (1.x `primary()` for the accent one). A "&" in the label is its Alt mnemonic.
pub fn button(ui: &mut Ui, p: &Palette, label: &str, look: Look, enabled: bool) -> bool {
    let colour = match look {
        Look::Normal => p.tx,
        _ => Color32::WHITE,
    };
    let text = if label.contains('&') {
        mnemonic_text(ui, label, colour)
    } else {
        RichText::new(label).strong().color(colour).into()
    };
    let mut b = egui::Button::new(text).min_size(vec2(76.0, 28.0));
    b = match look {
        Look::Primary => b.fill(p.ac),
        Look::Danger => b.fill(p.bad),
        Look::Normal => b,
    };
    let clicked = ui.add_enabled(enabled, b).clicked();
    clicked || (enabled && mnemonic_pressed(ui, label))
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
    let text: egui::WidgetText = if label.contains('&') {
        mnemonic_text(ui, label, ink)
    } else {
        RichText::new(label).strong().color(ink).into()
    };
    let mut b =
        egui::Button::image_and_text(icon(name, 14.0, p, ink), text).min_size(vec2(76.0, 28.0));
    b = match look {
        Look::Primary => b.fill(p.ac),
        Look::Danger => b.fill(p.bad),
        Look::Normal => b,
    };
    ui.add(b).clicked() || mnemonic_pressed(ui, label)
}

/// A form row: a label column of `width`, then the controls. The closure gets the label's id: fields
/// name themselves after it (`.labelled_by(id)`) for screen readers and the tests.
pub fn row<R>(ui: &mut Ui, label: &str, width: f32, f: impl FnOnce(&mut Ui, egui::Id) -> R) -> R {
    ui.horizontal(|ui| {
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

/// A caption over a field or a group (1.x `caption()`: small, upper case, faint); its id names the field
/// (`.labelled_by`).
pub fn caption(ui: &mut Ui, p: &Palette, text: &str) -> egui::Id {
    ui.label(
        RichText::new(text.to_uppercase())
            .size(11.0)
            .strong()
            .color(p.fa),
    )
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

/// A note in a box (1.x `note(boxed=True)`, the Tauri build's `noteBox`).
pub fn note(ui: &mut Ui, p: &Palette, text: &str) {
    egui::Frame::new()
        .fill(p.track)
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(text).color(p.mu)).wrap());
        });
}

/// An accent box with an icon (the running note of Check hash, the Scheduler's next run).
pub fn accent_note(ui: &mut Ui, p: &Palette, icon_name: &str, text: impl Into<egui::WidgetText>) {
    egui::Frame::new()
        .fill(p.acsel)
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.add(icon(icon_name, 16.0, p, p.actx));
                ui.add(egui::Label::new(text).wrap());
            });
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
    let strip = ui.horizontal(|ui| {
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
    let y = strip.response.rect.bottom() + 3.0;
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

/// A folder chooser next to a path field ("Browse..."); the chosen folder replaces the text.
pub fn folder_field(ui: &mut Ui, p: &Palette, text: &mut String, by: egui::Id) -> bool {
    folder_field_with(ui, p, text, by, "Browse...", "Select a Folder")
}

/// [`folder_field`] with its button's label (a "&" mnemonic too) and the folder dialog's title.
pub fn folder_field_with(
    ui: &mut Ui,
    p: &Palette,
    text: &mut String,
    by: egui::Id,
    button: &str,
    title: &str,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let browse = ui.available_width() - 90.0;
        ui.add(
            egui::TextEdit::singleline(text)
                .font(egui::TextStyle::Monospace)
                .desired_width(browse.max(80.0)),
        )
        .labelled_by(by);
        if plain(ui, p, button)
            && let Some(d) = rfd::FileDialog::new()
                .set_title(title)
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
            // Properties shows no hint (1.x: an empty message line takes no room).
            Ok(_) if self.hint.is_empty() => {}
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

/// A combo box over (value, label) pairs, named by the label `by`; true when the choice changed.
pub fn combo_by(
    ui: &mut Ui,
    id: &str,
    width: f32,
    value: &mut String,
    choices: &[(String, String)],
    by: egui::Id,
) -> bool {
    let shown = choices
        .iter()
        .find(|(v, _)| v == value)
        .map(|(_, l)| l.clone())
        .unwrap_or_else(|| value.clone());
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (v, l) in choices {
                if ui.selectable_label(value == v, l).clicked() && value != v {
                    *value = v.clone();
                    changed = true;
                }
            }
        })
        .response
        .labelled_by(by);
    changed
}

/// A segmented choice (a row of toggle buttons, one on); true when it changed.
pub fn segmented<T: PartialEq + Clone>(
    ui: &mut Ui,
    p: &Palette,
    value: &mut T,
    choices: &[(T, &str)],
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (v, label) in choices {
            let on = value == v;
            let text = RichText::new(*label).color(if on { p.actx } else { p.tx });
            let b = egui::Button::selectable(on, text).min_size(vec2(0.0, 26.0));
            if ui.add(b).clicked() && !on {
                *value = v.clone();
                changed = true;
            }
        }
    });
    changed
}

/// A category combo: each choice with its category icon (1.x's category combos).
pub fn category_combo(
    ui: &mut Ui,
    p: &Palette,
    id: &str,
    width: f32,
    value: &mut String,
    names: &[String],
    by: egui::Id,
) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(value.clone())
        .show_ui(ui, |ui| {
            for n in names {
                let ic = icon(&lider_core::view::category_icon(n), 16.0, p, p.tx);
                let b = egui::Button::selectable(value == n, (ic, n.as_str()));
                if ui.add(b).clicked() && value != n {
                    *value = n.clone();
                    changed = true;
                }
            }
        })
        .response
        .labelled_by(by);
    changed
}

/// A date field (yyyy-mm-dd) with a calendar under its button (1.x's calendar popup); true when the
/// date changed. A bad date typed into the field is left for the caller to check.
pub fn date_field(
    ui: &mut Ui,
    p: &Palette,
    text: &mut String,
    enabled: bool,
    by: egui::Id,
) -> egui::Response {
    use chrono::{Datelike, NaiveDate};
    let r = ui
        .add_enabled(
            enabled,
            egui::TextEdit::singleline(text)
                .font(egui::TextStyle::Monospace)
                .char_limit(10)
                .desired_width(100.0),
        )
        .labelled_by(by);
    let button = ui
        .add_enabled(
            enabled,
            egui::Button::image(icon("scheduler", 14.0, p, p.tx)),
        )
        .on_hover_text("Choose a date")
        .accessible_name("Choose a date");
    let id = ui.make_persistent_id(("calendar", by));
    let today = chrono::Local::now().date_naive();
    let shown = NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap_or(today);
    // The month the calendar shows (kept while it is open).
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&button));
    let first = shown.with_day(1).unwrap_or(shown);
    let mut month: NaiveDate = if open {
        ui.data(|d| d.get_temp(id)).unwrap_or(first)
    } else {
        first
    };
    let mut picked = None;
    egui::Popup::menu(&button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.horizontal(|ui| {
                if ui
                    .small_button("<")
                    .on_hover_text("Previous month")
                    .clicked()
                {
                    month = month - chrono::Months::new(1);
                }
                ui.label(RichText::new(month.format("%B %Y").to_string()).strong());
                if ui.small_button(">").on_hover_text("Next month").clicked() {
                    month = month + chrono::Months::new(1);
                }
            });
            egui::Grid::new(("calendar-grid", by)).show(ui, |ui| {
                for d in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] {
                    ui.label(RichText::new(d).small().color(p.mu));
                }
                ui.end_row();
                let lead = month.weekday().num_days_from_monday();
                for _ in 0..lead {
                    ui.label("");
                }
                let mut day = month;
                let mut col = lead;
                while day.month() == month.month() {
                    let sel = day == shown;
                    if ui
                        .selectable_label(sel, RichText::new(day.day().to_string()).monospace())
                        .clicked()
                    {
                        picked = Some(day);
                    }
                    col += 1;
                    if col == 7 {
                        col = 0;
                        ui.end_row();
                    }
                    day = day.succ_opt().unwrap_or(day);
                    if day.day() == 1 {
                        break;
                    }
                }
            });
            if picked.is_some() {
                ui.close();
            }
        });
    ui.data_mut(|d| d.insert_temp(id, month));
    if let Some(d) = picked {
        *text = d.format("%Y-%m-%d").to_string();
        ui.data_mut(|m| m.remove::<NaiveDate>(id));
    }
    let mut r = r;
    if picked.is_some() {
        r.mark_changed();
    }
    r
}
