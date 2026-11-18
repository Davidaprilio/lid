//! The main window (`main_window.py` in 1.x): menu bar, toolbar, search bar, category tree, download list,
//! details pane, status bar. Everything shown comes from `App::view`; every click becomes `App::run` or
//! one of the list methods.

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Key, Layout, Modifiers, Rect, RichText, Sense,
    Stroke, StrokeKind, Ui, vec2,
};
use lider_core::menu::Item;
use lider_core::view::{CheckState, Pill, TreeFilter, TreeNode};

use crate::app::App;
use crate::icons::icon;
use crate::theme::{Palette, alpha};

/// A small filled triangle pointing down (or up): sort order and drop-down arrows, drawn rather than taken
/// from a font, which may not have the glyph.
pub fn triangle(painter: &egui::Painter, center: egui::Pos2, down: bool, colour: Color32) {
    let (w, h) = (4.0, 3.0);
    let pts = if down {
        vec![
            center + vec2(-w, -h / 1.5),
            center + vec2(w, -h / 1.5),
            center + vec2(0.0, h),
        ]
    } else {
        vec![
            center + vec2(-w, h / 1.5),
            center + vec2(w, h / 1.5),
            center + vec2(0.0, -h),
        ]
    };
    painter.add(egui::Shape::convex_polygon(pts, colour, Stroke::NONE));
}

/// A check mark in `rect`.
pub fn check_mark(painter: &egui::Painter, rect: Rect, colour: Color32) {
    let c = rect.center();
    let s = rect.width() / 2.0;
    painter.add(egui::Shape::line(
        vec![
            c + vec2(-0.55 * s, 0.0),
            c + vec2(-0.15 * s, 0.4 * s),
            c + vec2(0.6 * s, -0.45 * s),
        ],
        Stroke::new(2.0, colour),
    ));
}

/// A small close button (the set's `close` icon).
pub fn close_button(ui: &mut Ui, p: &Palette, tip: &str) -> bool {
    ui.add(egui::Button::image(icon("close", 12.0, p, p.mu)).frame(false))
        .on_hover_text(tip)
        .clicked()
}

/// A label without its `&` mnemonic marks (`&&` is a literal `&`).
pub fn plain(label: &str) -> String {
    label
        .replace("&&", "\u{1}")
        .replace('&', "")
        .replace('\u{1}', "&")
}

/// Row height by density: 32 comfortable, 26 compact (1.x MW:82).
pub fn row_height(app: &App) -> f32 {
    if app.manager.settings.density == "compact" {
        26.0
    } else {
        32.0
    }
}

/// The list's text: the family of View → Font (crate::fonts::LIST) at its size (points → pixels as the Tauri
/// window does), 13 px without a choice; the mono cells one pixel smaller.
pub fn list_fonts(app: &App) -> (FontId, FontId) {
    let size = app
        .manager
        .settings
        .list_font_family_and_size()
        .map_or(13.0, |(_, pt)| (pt * 1.333) as f32)
        .clamp(8.0, 40.0);
    (
        FontId::new(size, egui::FontFamily::Name(crate::fonts::LIST.into())),
        FontId::monospace(size - 1.0),
    )
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    keyboard(app, ui.ctx());
    app.status_tip.clear();

    egui::Panel::top("menubar")
        .frame(
            egui::Frame::new()
                .fill(p.win)
                .inner_margin(egui::Margin::symmetric(6, 2)),
        )
        .show(ui, |ui| {
            menubar(app, ui);
        });
    egui::Panel::top("toolbar")
        .frame(
            egui::Frame::new()
                .fill(p.win)
                .inner_margin(egui::Margin::symmetric(10, 7)),
        )
        .show(ui, |ui| {
            toolbar(app, ui);
        });
    if app.search_open {
        egui::Panel::top("search")
            .frame(
                egui::Frame::new()
                    .fill(p.win)
                    .inner_margin(egui::Margin::symmetric(10, 6)),
            )
            .show(ui, |ui| {
                searchbar(app, ui);
            });
    }
    if app.manager.settings.show_statusbar {
        egui::Panel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(p.win)
                    .inner_margin(egui::Margin::symmetric(10, 5)),
            )
            .show(ui, |ui| {
                statusbar(app, ui);
            });
    }
    // The sidebar and the details pane are dragged to size; the right side keeps at least 560 px and the
    // sizes are saved when the drag ends (1.x MW:430, 1517-1520).
    let released = ui.input(|i| i.pointer.any_released());
    // As the Tauri window: the window's width less 560, its height less 220.
    let room = ui.ctx().content_rect();
    if app.manager.settings.show_categories {
        let max = (room.width() - 560.0).max(170.0);
        let r = egui::Panel::left("tree")
            .resizable(true)
            .default_size(app.manager.settings.sidebar_width as f32)
            .size_range(170.0..=max)
            .frame(
                egui::Frame::new()
                    .fill(p.win)
                    .inner_margin(egui::Margin::same(8)),
            )
            .show(ui, |ui| tree(app, ui));
        let w = r.response.rect.width().round() as i64;
        if released && w != app.manager.settings.sidebar_width {
            app.manager.settings.sidebar_width = w;
            app.save_settings();
        }
    }
    // The pane shows only while a download is selected (1.x MW:1241-1252).
    if app.manager.settings.show_details && app.view.details.is_some() {
        let max = (room.height() - 220.0).max(170.0);
        let r = egui::Panel::bottom("details")
            .resizable(true)
            .default_size(app.manager.settings.details_height as f32)
            .size_range(170.0..=max)
            .frame(egui::Frame::new().fill(p.pane))
            .show(ui, |ui| details(app, ui));
        let h = r.response.rect.height().round() as i64;
        if released && h != app.manager.settings.details_height {
            app.manager.settings.details_height = h;
            app.save_settings();
        }
    }
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(p.pane))
        .show(ui, |ui| list(app, ui));

    if let Some(mut b) = app.boxx.take() {
        match b.show(ui.ctx(), &p) {
            Some(answer) => app.answer(answer),
            None => app.boxx = Some(b),
        }
    }
    crate::dialogs::show_all(app, ui.ctx());
}

// ---------------------------------------------------------------------- keyboard

fn keyboard(app: &mut App, ctx: &egui::Context) {
    // Drawn over the main window (the test harness), a dialog has the keys.
    if app.boxx.is_some() || (ctx.embed_viewports() && !app.dialogs.is_empty()) {
        return;
    }
    let typing = ctx.egui_wants_keyboard_input();
    // Ctrl+V on the window (not in a text field) adds the addresses in the clipboard (decision G2).
    let pasted = (!typing)
        .then(|| {
            ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Paste(t) => Some(t.clone()),
                    _ => None,
                })
            })
        })
        .flatten();
    if let Some(text) = pasted {
        app.paste(&text);
        return;
    }
    let hit = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
    if hit(Modifiers::CTRL, Key::N) {
        app.run("add");
    } else if hit(Modifiers::CTRL, Key::Q) {
        app.run("exit");
    } else if hit(Modifiers::CTRL, Key::F) {
        app.run("find");
    } else if hit(Modifiers::NONE, Key::F3) {
        app.run("find_next");
    } else if hit(Modifiers::NONE, Key::F9) {
        app.run("details");
    } else if app.search_open && hit(Modifiers::NONE, Key::Escape) {
        app.close_search();
    } else if !typing && !egui::Popup::is_any_open(ctx) {
        // (An open menu has the arrows, Enter and its letters.)
        if hit(Modifiers::NONE, Key::Delete) {
            app.run("remove");
        } else if hit(Modifiers::CTRL, Key::A) {
            app.select_all();
        } else if hit(Modifiers::NONE, Key::ArrowDown) {
            app.move_current(1, None);
        } else if hit(Modifiers::NONE, Key::ArrowUp) {
            app.move_current(-1, None);
        } else if hit(Modifiers::NONE, Key::Home) {
            app.move_current(0, Some(false));
        } else if hit(Modifiers::NONE, Key::End) {
            app.move_current(0, Some(true));
        } else if hit(Modifiers::NONE, Key::Enter)
            && let Some(id) = app.current
        {
            app.activate(id);
        } else if hit(Modifiers::NONE, Key::Space)
            && let Some(id) = app.current
        {
            let on = !app.checked.contains(&id);
            app.check(id, on);
        }
    }
}

// ---------------------------------------------------------------------- menus

/// Draw menu items; the id of a clicked action goes to `picked`, the tip under the mouse to `tip`.
pub fn menu_items(
    ui: &mut Ui,
    p: &Palette,
    items: &[Item],
    picked: &mut Option<String>,
    tip: &mut String,
) {
    // While a menu is open its items' mnemonic letters pick them (no Alt needed, as in Qt menus).
    let letter = ui.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if modifiers.is_none() && key.name().len() == 1 => {
                key.name().to_ascii_lowercase().chars().next()
            }
            _ => None,
        })
    });
    for it in items {
        match it {
            Item::Separator => {
                ui.separator();
            }
            Item::Submenu { label, items } => {
                ui.menu_button(plain(label), |ui| menu_items(ui, p, items, picked, tip));
            }
            Item::Action {
                id,
                label,
                shortcut,
                icon: icon_name,
                tip: text,
                enabled,
                checked,
            } => {
                let r = if let Some(on) = checked {
                    let mut on = *on;
                    ui.add_enabled(*enabled, egui::Checkbox::new(&mut on, plain(label)))
                } else {
                    let mut b = match icon_name {
                        Some(name) => {
                            egui::Button::image_and_text(icon(name, 16.0, p, p.tx), plain(label))
                        }
                        None => egui::Button::new(plain(label)),
                    };
                    if let Some(s) = shortcut {
                        b = b.shortcut_text(*s);
                    }
                    ui.add_enabled(*enabled, b)
                };
                if r.hovered()
                    && let Some(t) = text
                {
                    *tip = (*t).to_string();
                }
                let by_letter = *enabled && letter.is_some() && letter == mnemonic(label);
                if r.clicked() || by_letter {
                    *picked = Some(id.clone());
                    ui.close();
                }
            }
        }
    }
}

/// The letter after `&` in a label (its Alt mnemonic), lower case.
pub fn mnemonic(label: &str) -> Option<char> {
    let mut chars = label.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            match chars.next() {
                Some('&') => continue,
                Some(m) => return Some(m.to_ascii_lowercase()),
                None => return None,
            }
        }
    }
    None
}

/// A label with its mnemonic letter underlined (shown while Alt is held, as in 1.x and the Tauri window).
fn underlined(label: &str, colour: Color32) -> egui::text::LayoutJob {
    let text = plain(label);
    let mut job = egui::text::LayoutJob::default();
    let at = mnemonic(label).and_then(|m| text.to_lowercase().find(m));
    let font = egui::TextFormat {
        font_id: FontId::proportional(14.0),
        color: colour,
        ..Default::default()
    };
    match at {
        Some(i) => {
            let end = i + text[i..].chars().next().map_or(1, char::len_utf8);
            job.append(&text[..i], 0.0, font.clone());
            job.append(
                &text[i..end],
                0.0,
                egui::TextFormat {
                    underline: Stroke::new(1.0, colour),
                    ..font.clone()
                },
            );
            job.append(&text[end..], 0.0, font);
        }
        None => job.append(&text, 0.0, font),
    }
    job
}

/// The letter of an Alt+letter press this frame (consumed), if any.
fn alt_letter(ctx: &egui::Context) -> Option<char> {
    ctx.input_mut(|i| {
        let hit = i.events.iter().find_map(|e| match e {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if modifiers.alt && !modifiers.ctrl && !modifiers.command => {
                let name = key.name();
                (name.len() == 1).then(|| (*key, name.to_ascii_lowercase().chars().next()))
            }
            _ => None,
        });
        let (key, letter) = hit?;
        i.consume_key(Modifiers::ALT, key);
        letter
    })
}

fn menubar(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let menus = app.view.menus.clone();
    let mut picked = None;
    let mut tip = String::new();
    // Alt+letter opens the menu with that mnemonic (Alt+T: Tasks); the letters show underlined while Alt is
    // held. No dialog or message box may have the keys.
    let free = app.boxx.is_none() && !(ui.ctx().embed_viewports() && !app.dialogs.is_empty());
    let alt = free && ui.input(|i| i.modifiers.alt);
    let letter = if free { alt_letter(ui.ctx()) } else { None };
    egui::MenuBar::new().ui(ui, |ui| {
        for m in &menus {
            let text: egui::WidgetText = if alt {
                underlined(&m.label, p.tx).into()
            } else {
                plain(&m.label).into()
            };
            let r = ui
                .menu_button(text, |ui| {
                    menu_items(ui, &p, &m.items, &mut picked, &mut tip)
                })
                .response;
            if letter.is_some() && letter == mnemonic(&m.label) {
                let id = egui::Popup::default_response_id(&r);
                egui::Popup::open_id(ui.ctx(), id);
                ui.ctx().request_repaint();
            }
        }
    });
    if !tip.is_empty() {
        app.status_tip = tip;
    }
    if let Some(id) = picked {
        app.run(&id);
    }
}

// ---------------------------------------------------------------------- toolbar

struct Tool {
    id: &'static str,
    icon: &'static str,
    label: &'static str,
    tip: &'static str,
}

fn tool(
    app: &mut App,
    ui: &mut Ui,
    t: &Tool,
    enabled: bool,
    primary: bool,
    picked: &mut Option<&'static str>,
) {
    let p = app.palette;
    let labels = app.manager.settings.toolbar_labels || primary;
    let ink = if primary {
        Color32::WHITE
    } else if enabled {
        p.tx
    } else {
        p.fa
    };
    let image = icon(t.icon, 17.0, &p, ink);
    let mut b = if labels {
        egui::Button::image_and_text(image, RichText::new(t.label).strong().color(ink))
    } else {
        egui::Button::image(image)
    };
    b = b
        .corner_radius(CornerRadius::same(16))
        .min_size(vec2(34.0, 32.0));
    if primary {
        b = b.fill(p.ac).stroke(Stroke::NONE);
    }
    let r = ui.add_enabled(enabled, b);
    if r.hovered() {
        app.status_tip = t.tip.to_string();
    }
    if r.on_hover_text(t.tip).clicked() {
        *picked = Some(t.id);
    }
}

fn toolbar(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let a = app.view.actions;
    let on = |f: fn(&lider_core::view::ActionState) -> bool| a.as_ref().is_some_and(f);
    let mut picked = None;
    ui.horizontal(|ui| {
        tool(
            app,
            ui,
            &Tool {
                id: "add",
                icon: "add-white",
                label: "Add URL",
                tip: "Add new download address manually",
            },
            true,
            true,
            &mut picked,
        );
        ui.separator();
        tool(
            app,
            ui,
            &Tool {
                id: "resume",
                icon: "resume",
                label: "Start",
                tip: "Start or resume the checked (or selected) downloads",
            },
            on(|a| a.resume),
            false,
            &mut picked,
        );
        tool(
            app,
            ui,
            &Tool {
                id: "stop",
                icon: "stop",
                label: "Stop",
                tip: "Stop the checked (or selected) downloads",
            },
            on(|a| a.stop),
            false,
            &mut picked,
        );
        ui.separator();
        tool(
            app,
            ui,
            &Tool {
                id: "remove",
                icon: "delete",
                label: "Delete",
                tip: "Remove the checked (or selected) files from the list",
            },
            on(|a| a.remove),
            false,
            &mut picked,
        );
        // The arrow next to Delete: the other ways to delete.
        let arrow = ui.add(
            egui::Button::new("")
                .min_size(vec2(22.0, 32.0))
                .corner_radius(CornerRadius::same(10)),
        );
        triangle(ui.painter(), arrow.rect.center(), true, p.mu);
        let arrow = arrow.on_hover_text("More ways to delete");
        // The split part of Delete (1.x MW:316-321).
        egui::Popup::menu(&arrow).show(|ui| {
            for (id, label, icon_name, enabled) in [
                (
                    "remove",
                    "Delete checked or selected",
                    "delete",
                    on(|a| a.remove),
                ),
                (
                    "delete_completed",
                    "Delete all completed",
                    "delete-completed",
                    on(|a| a.delete_completed),
                ),
            ] {
                let b = egui::Button::image_and_text(icon(icon_name, 16.0, &p, p.tx), label);
                if ui.add_enabled(enabled, b).clicked() {
                    picked = Some(id);
                    ui.close();
                }
            }
        });
        let checked = a.as_ref().map_or(0, |a| a.checked);
        if checked > 0 {
            ui.separator();
            // The chip "N checked ✕" (1.x CheckChip).
            egui::Frame::new()
                .fill(p.acsel)
                .corner_radius(CornerRadius::same(15))
                .inner_margin(egui::Margin {
                    left: 10,
                    right: 6,
                    top: 4,
                    bottom: 4,
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (b, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                        check_box(ui.painter(), &p, b.center(), CheckState::Partial);
                        ui.label(
                            RichText::new(format!("{checked} checked"))
                                .strong()
                                .color(p.actx),
                        );
                        if ui
                            .add(egui::Button::image(icon("close", 12.0, &p, p.actx)).frame(false))
                            .on_hover_text("Clear the checks")
                            .clicked()
                        {
                            app.checked.clear();
                            app.rebuild();
                        }
                    });
                });
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            tool(
                app,
                ui,
                &Tool {
                    id: "grabber",
                    icon: "grabber",
                    label: "Grabber",
                    tip: "Run site grabber",
                },
                true,
                false,
                &mut picked,
            );
            tool(
                app,
                ui,
                &Tool {
                    id: "scheduler",
                    icon: "scheduler",
                    label: "Scheduler",
                    tip: "Scheduler and download queues",
                },
                true,
                false,
                &mut picked,
            );
            tool(
                app,
                ui,
                &Tool {
                    id: "options",
                    icon: "options",
                    label: "Options",
                    tip: "Integration, file types, save folders, connections, proxy and others",
                },
                true,
                false,
                &mut picked,
            );
        });
    });
    if let Some(id) = picked {
        app.run(id);
    }
}

// ---------------------------------------------------------------------- search bar

fn searchbar(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    // "Find:" [field] [Find] [Settings...] … [✕] (1.x find.py:95-124).
    ui.horizontal(|ui| {
        let label = ui.label("Find:");
        let field = ui
            .add(egui::TextEdit::singleline(&mut app.search_text).desired_width(260.0))
            .labelled_by(label.id);
        if std::mem::take(&mut app.focus_search) {
            field.request_focus();
            // Show the current query selected, as 1.x does.
            let mut state = egui::TextEdit::load_state(ui.ctx(), field.id).unwrap_or_default();
            let all = egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(app.search_text.chars().count()),
            );
            state.cursor.set_char_range(Some(all));
            state.store(ui.ctx(), field.id);
        }
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        let find = ui.add(
            egui::Button::new(RichText::new("Find").strong().color(Color32::WHITE)).fill(p.ac),
        );
        if find.clicked() || enter {
            app.run_search();
        }
        if ui
            .button("Settings...")
            .on_hover_text("LID Search settings")
            .clicked()
        {
            app.run("find_settings");
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if close_button(ui, &p, "Close SearchBar") {
                app.close_search();
            }
        });
    });
}

// ---------------------------------------------------------------------- status bar

fn statusbar(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let Some(s) = app.view.status.clone() else {
        return;
    };
    ui.horizontal(|ui| {
        if app.status_tip.is_empty() {
            ui.label(RichText::new(&s.speed).monospace().color(p.mu));
            ui.label(RichText::new(&s.counts).color(p.mu));
        } else {
            ui.label(RichText::new(&app.status_tip).color(p.mu));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let browser = crate::dialogs::browsers::status_text(app);
            let link = ui
                .add(egui::Button::new(RichText::new(browser).color(p.actx)).frame(false))
                .on_hover_text("Browser integration…");
            if link.clicked() {
                app.run("browsers");
            }
            ui.label(RichText::new(&s.clipboard).color(p.mu));
            ui.label(RichText::new(&s.limiter).color(p.mu));
        });
    });
}

// ---------------------------------------------------------------------- tree

fn tree(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui.horizontal(|ui| {
        ui.label(RichText::new("CATEGORIES").size(11.0).strong().color(p.fa));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if close_button(ui, &p, "Hide categories") {
                app.run("hide_categories");
            }
        });
    });
    ui.add_space(4.0);
    let nodes = app.view.tree.clone();
    let mut picked: Option<TreeFilter> = None;
    let mut action: Option<String> = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        for n in &nodes {
            tree_node(ui, &p, n, 0, &app.filter, &mut picked, &mut action);
        }
    });
    if let Some(f) = picked {
        app.set_filter(f);
    }
    if let Some(id) = action {
        app.run(&id);
    }
}

fn tree_node(
    ui: &mut Ui,
    p: &Palette,
    n: &TreeNode,
    depth: usize,
    current: &TreeFilter,
    picked: &mut Option<TreeFilter>,
    action: &mut Option<String>,
) {
    let selected = &n.filter == current;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    if selected || resp.hovered() {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(7),
            if selected {
                p.acsel
            } else {
                alpha(p.acsel, 0.5)
            },
        );
    }
    let ink = if selected { p.actx } else { p.tx };
    let x = rect.left() + 8.0 + depth as f32 * 16.0;
    icon(&n.icon, 16.0, p, ink).paint_at(
        ui,
        Rect::from_min_size(egui::pos2(x, rect.center().y - 8.0), vec2(16.0, 16.0)),
    );
    let clip = ui.painter().with_clip_rect(rect.shrink2(vec2(4.0, 0.0)));
    clip.text(
        egui::pos2(x + 24.0, rect.center().y),
        Align2::LEFT_CENTER,
        &n.title,
        FontId::proportional(13.0),
        ink,
    );
    if n.count > 0 {
        let text = n.count.to_string();
        let w = 10.0 + 7.0 * text.len() as f32;
        let badge = Rect::from_center_size(
            egui::pos2(rect.right() - 8.0 - w / 2.0, rect.center().y),
            vec2(w, 16.0),
        );
        ui.painter().rect_filled(
            badge,
            CornerRadius::same(8),
            if selected { p.ac } else { p.track },
        );
        ui.painter().text(
            badge.center(),
            Align2::CENTER_CENTER,
            text,
            FontId::monospace(10.5),
            if selected { Color32::WHITE } else { p.mu },
        );
    }
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, &n.title));
    if resp.clicked() {
        *picked = Some(n.filter.clone());
    }
    // The context menu: queues get Start, Stop and Scheduler; categories Add, Delete and Properties.
    resp.context_menu(|ui| {
        use lider_core::view::NodeKind;
        let value = n.filter.value.clone().unwrap_or_default();
        let mut item = |ui: &mut Ui, label: &str, id: String, enabled: bool| {
            if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                *action = Some(id);
                ui.close();
            }
        };
        match n.filter.kind {
            NodeKind::Queue => {
                item(ui, "Start", format!("q_start:{value}"), true);
                item(ui, "Stop", format!("q_stop:{value}"), true);
                ui.separator();
                item(ui, "Scheduler...", format!("q_scheduler:{value}"), true);
            }
            NodeKind::Queues => item(ui, "Scheduler...", "q_scheduler".into(), true),
            _ => {
                item(ui, "Add category", "cat_add".into(), true);
                item(
                    ui,
                    "Delete",
                    format!("cat_delete:{value}"),
                    !value.is_empty(),
                );
                item(
                    ui,
                    "Properties",
                    format!("cat_properties:{value}"),
                    !value.is_empty(),
                );
            }
        }
    });
    if n.expanded {
        for c in &n.children {
            tree_node(ui, p, c, depth + 1, current, picked, action);
        }
    }
}

// ---------------------------------------------------------------------- list

fn check_box(painter: &egui::Painter, p: &Palette, center: egui::Pos2, state: CheckState) {
    let b = Rect::from_center_size(center, vec2(15.0, 15.0));
    match state {
        CheckState::Unchecked => {
            painter.rect_filled(b, CornerRadius::same(4), p.input);
            painter.rect_stroke(
                b,
                CornerRadius::same(4),
                Stroke::new(1.0, p.line2),
                StrokeKind::Inside,
            );
        }
        s => {
            painter.rect_filled(b, CornerRadius::same(4), p.ac);
            if s == CheckState::Checked {
                check_mark(painter, b.shrink(2.0), Color32::WHITE);
            } else {
                painter.hline(
                    b.left() + 4.0..=b.right() - 4.0,
                    b.center().y,
                    Stroke::new(2.0, Color32::WHITE),
                );
            }
        }
    }
}

fn list(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let cols = app.view.columns.clone();
    let height = row_height(app);
    let sort_key = app.manager.settings.sort_key.clone();
    let sort_desc = app.manager.settings.sort_desc;
    let sort_shown = app.sort_shown;
    let (text_font, mono_font) = list_fonts(app);
    let total: f32 = cols.iter().map(|c| c.width).sum();
    let mut resized: Option<(String, f32, bool)> = None;
    let mut header_menu = false;

    egui::ScrollArea::horizontal()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.set_min_width(total.max(ui.available_width()));
            ui.spacing_mut().item_spacing.y = 0.0;
            // Header: click sorts (again reverses); the first column checks or unchecks every row.
            let (head, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
            ui.painter().rect_filled(head, CornerRadius::ZERO, p.hdr);
            ui.painter()
                .hline(head.x_range(), head.bottom(), Stroke::new(1.0, p.line));
            let mut x = head.left();
            for c in &cols {
                let cell =
                    Rect::from_min_size(egui::pos2(x, head.top()), vec2(c.width, head.height()));
                let r = ui.interact(cell, ui.scope_id().with(("head", &c.key)), Sense::click());
                let title = if c.key == "_check" {
                    "Check all".to_string()
                } else {
                    format!("Column {}", lider_core::view::column_title(&c.key))
                };
                r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &title));
                if c.key == "_check" {
                    check_box(
                        ui.painter(),
                        &p,
                        cell.center(),
                        lider_core::view::check_state(&app.visible, &app.checked),
                    );
                    if r.clicked() {
                        app.toggle_all();
                    }
                } else {
                    let title = lider_core::view::column_title(&c.key).to_uppercase();
                    let sorted = sort_shown && c.key == sort_key;
                    let ink = if sorted { p.actx } else { p.mu };
                    let clip = ui.painter().with_clip_rect(cell);
                    let r_text = clip.text(
                        egui::pos2(x + 8.0, cell.center().y),
                        Align2::LEFT_CENTER,
                        title,
                        FontId::proportional(11.0),
                        ink,
                    );
                    if sorted {
                        triangle(
                            &clip,
                            egui::pos2(r_text.right() + 9.0, cell.center().y),
                            sort_desc,
                            ink,
                        );
                    }
                    if r.clicked() {
                        app.header_click(&c.key);
                    }
                    // Right-click on the header opens Customize columns (1.x MW:411-412).
                    if r.secondary_clicked() {
                        header_menu = true;
                    }
                    // The grip at the right edge: drag to size the column.
                    let grip = Rect::from_min_max(
                        egui::pos2(cell.right() - 4.0, cell.top()),
                        egui::pos2(cell.right() + 3.0, cell.bottom()),
                    );
                    let g = ui
                        .interact(grip, ui.scope_id().with(("grip", &c.key)), Sense::drag())
                        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                    g.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::Splitter,
                            true,
                            format!("Width of {}", lider_core::view::column_title(&c.key)),
                        )
                    });
                    if g.dragged() {
                        resized = Some((c.key.clone(), c.width + g.drag_delta().x, false));
                    }
                    if g.drag_stopped() {
                        resized = Some((c.key.clone(), c.width, true));
                    }
                    ui.painter().vline(
                        cell.right() - 1.0,
                        cell.y_range().shrink(6.0),
                        Stroke::new(
                            1.0,
                            if g.hovered() || g.dragged() {
                                p.ac
                            } else {
                                p.line
                            },
                        ),
                    );
                }
                x += c.width;
            }

            let count = app.view.rows.len();
            let scroll_row = app
                .scroll_to
                .take()
                .and_then(|id| app.view.rows.iter().position(|r| r.id == id));
            let mut area = egui::ScrollArea::vertical()
                .auto_shrink(false)
                .id_salt("rows");
            if let Some(i) = scroll_row {
                // Bring the row into view (Find Next, arrow keys).
                area = area.vertical_scroll_offset((i as f32 * height - 2.0 * height).max(0.0));
            }
            let mut click: Option<(i64, bool, bool)> = None;
            let mut check: Option<(i64, bool)> = None;
            let mut activate = None;
            let mut action: Option<String> = None;
            let background = area
                .show_rows(ui, height, count, |ui, range| {
                    for i in range {
                        let row = &app.view.rows[i];
                        let (rect, resp) = ui.allocate_exact_size(
                            vec2(ui.available_width(), height),
                            Sense::click(),
                        );
                        let selected = app.selected.contains(&row.id);
                        // Selected: acsel with a 3 px accent bar; checked: acsel at 150/255 (1.x widgets.py:899-914).
                        let fill = if selected {
                            p.acsel
                        } else if row.checked {
                            alpha(p.acsel, 150.0 / 255.0)
                        } else if i % 2 == 1 {
                            p.alt
                        } else {
                            p.pane
                        };
                        ui.painter().rect_filled(rect, CornerRadius::ZERO, fill);
                        if selected {
                            ui.painter().rect_filled(
                                Rect::from_min_size(rect.min, vec2(3.0, height)),
                                CornerRadius::ZERO,
                                p.ac,
                            );
                        }
                        let mut x = rect.left();
                        // The check column has no text of its own; the cells belong to the columns after it.
                        let offset = cols.len().saturating_sub(row.cells.len());
                        let empty = String::new();
                        for (n, c) in cols.iter().enumerate() {
                            let text = if n >= offset {
                                row.cells.get(n - offset).unwrap_or(&empty)
                            } else {
                                &empty
                            };
                            let cell = Rect::from_min_size(
                                egui::pos2(x, rect.top()),
                                vec2(c.width, height),
                            );
                            let clip = ui.painter().with_clip_rect(cell.shrink2(vec2(4.0, 0.0)));
                            let at = egui::pos2(x + 8.0, cell.center().y);
                            match c.key.as_str() {
                                "_check" => {
                                    check_box(
                                        &clip,
                                        &p,
                                        cell.center(),
                                        if row.checked {
                                            CheckState::Checked
                                        } else {
                                            CheckState::Unchecked
                                        },
                                    );
                                    let cr = ui.interact(
                                        cell,
                                        ui.scope_id().with(("check", row.id)),
                                        Sense::click(),
                                    );
                                    let label = format!("Check {}", row.id);
                                    cr.widget_info(|| {
                                        egui::WidgetInfo::selected(
                                            egui::Role::CheckBox,
                                            true,
                                            row.checked,
                                            &label,
                                        )
                                    });
                                    if cr.clicked() {
                                        check = Some((row.id, !row.checked));
                                    }
                                }
                                "queue" => {
                                    if !row.queue.is_empty() {
                                        let r =
                                            Rect::from_center_size(cell.center(), vec2(16.0, 16.0));
                                        icon("queue", 16.0, &p, p.mu).paint_at(ui, r);
                                        // The queue's name as the icon's tooltip.
                                        ui.interact(
                                            r,
                                            ui.scope_id().with(("q", row.id)),
                                            Sense::hover(),
                                        )
                                        .on_hover_text(&row.queue);
                                    }
                                }
                                "filename" => {
                                    icon(&row.icon, 16.0, &p, p.tx).paint_at(
                                        ui,
                                        Rect::from_min_size(
                                            egui::pos2(x + 8.0, cell.center().y - 8.0),
                                            vec2(16.0, 16.0),
                                        ),
                                    );
                                    clip.text(
                                        egui::pos2(x + 32.0, cell.center().y),
                                        Align2::LEFT_CENTER,
                                        text,
                                        text_font.clone(),
                                        p.tx,
                                    );
                                }
                                "status" => {
                                    let r = pill(ui, &clip, &p, &row.pill, cell, &text_font);
                                    // An error's message as the pill's tooltip.
                                    if !row.error.is_empty() {
                                        ui.interact(
                                            r,
                                            ui.scope_id().with(("e", row.id)),
                                            Sense::hover(),
                                        )
                                        .on_hover_text(&row.error);
                                    }
                                }
                                "size" | "rate" | "time_left" | "last_try" | "date_added" => {
                                    clip.text(
                                        at,
                                        Align2::LEFT_CENTER,
                                        text,
                                        mono_font.clone(),
                                        p.tx,
                                    );
                                }
                                _ => {
                                    clip.text(
                                        at,
                                        Align2::LEFT_CENTER,
                                        text,
                                        text_font.clone(),
                                        p.tx,
                                    );
                                }
                            }
                            x += c.width;
                        }
                        let name = row
                            .cells
                            .iter()
                            .find(|c| !c.is_empty())
                            .cloned()
                            .unwrap_or_default();
                        resp.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::Role::Button,
                                true,
                                selected,
                                format!("Download {name}"),
                            )
                        });
                        let mods = ui.input(|i| i.modifiers);
                        if resp.double_clicked() {
                            activate = Some(row.id);
                        } else if resp.clicked() {
                            click = Some((row.id, mods.command, mods.shift));
                        }
                        if resp.secondary_clicked() && !selected {
                            click = Some((row.id, false, false));
                        }
                        let id = row.id;
                        resp.context_menu(|ui| row_menu(app, ui, id, &mut action));
                    }
                    // The empty part below the rows.
                    ui.allocate_response(ui.available_size(), Sense::click())
                })
                .inner;
            if background.clicked() {
                app.selected.clear();
                app.current = None;
                app.rebuild();
            }
            background.context_menu(|ui| {
                if ui
                    .add(egui::Button::image_and_text(
                        icon("add-url", 16.0, &p, p.tx),
                        "Add new download",
                    ))
                    .clicked()
                {
                    action = Some("add".into());
                    ui.close();
                }
            });
            if let Some((key, w, done)) = resized {
                app.set_column_width(&key, w);
                if done {
                    app.save_settings();
                }
            }
            if header_menu {
                action = Some("columns".into());
            }
            if let Some((id, on)) = check {
                app.check(id, on);
            }
            if let Some((id, ctrl, shift)) = click {
                app.click_row(id, ctrl, shift);
            }
            if let Some(id) = activate {
                app.activate(id);
            }
            if let Some(id) = action {
                app.run(&id);
            }
        });
}

#[allow(clippy::too_many_arguments)]
fn menu_item(
    ui: &mut Ui,
    p: &Palette,
    action: &mut Option<String>,
    label: &str,
    icon_name: Option<&str>,
    id: &str,
    enabled: bool,
    key: Option<&str>,
) {
    let mut b = match icon_name {
        Some(n) => egui::Button::image_and_text(icon(n, 16.0, p, p.tx), label),
        None => egui::Button::new(label),
    };
    if let Some(k) = key {
        b = b.shortcut_text(k);
    }
    if ui.add_enabled(enabled, b).clicked() {
        *action = Some(id.to_string());
        ui.close();
    }
}

/// The context menu of a download (1.x `_list_menu`).
fn row_menu(app: &App, ui: &mut Ui, id: i64, action: &mut Option<String>) {
    let p = app.palette;
    let a = app.view.actions;
    let on = |f: fn(&lider_core::view::ActionState) -> bool| a.as_ref().is_some_and(f);
    let row = app.view.rows.iter().find(|r| r.id == id);
    let complete = row.is_some_and(|r| matches!(r.pill.kind, "done" | "verified" | "mismatch"));
    let queued = app
        .view
        .rows
        .iter()
        .any(|r| app.selected.contains(&r.id) && !r.queue.is_empty());
    let one = app.selected.len() <= 1;
    menu_item(ui, &p, action, "Open", None, "ctx_open", complete, None);
    menu_item(
        ui,
        &p,
        action,
        "Open with...",
        None,
        "ctx_open_with",
        complete,
        None,
    );
    menu_item(
        ui,
        &p,
        action,
        "Open folder",
        None,
        "ctx_open_folder",
        true,
        None,
    );
    ui.separator();
    menu_item(
        ui,
        &p,
        action,
        "Move/Rename",
        None,
        "ctx_properties",
        true,
        None,
    );
    menu_item(
        ui,
        &p,
        action,
        "Redownload",
        None,
        "redownload",
        on(|a| a.redownload),
        None,
    );
    menu_item(
        ui,
        &p,
        action,
        "Check hash…",
        Some("checksum"),
        "ctx_check_hash",
        one && app.can_check(id),
        None,
    );
    ui.separator();
    menu_item(
        ui,
        &p,
        action,
        "Download Now",
        Some("resume"),
        "resume",
        on(|a| a.resume),
        None,
    );
    menu_item(
        ui,
        &p,
        action,
        "Stop Download",
        Some("stop"),
        "stop",
        on(|a| a.stop),
        None,
    );
    menu_item(
        ui,
        &p,
        action,
        "Remove",
        Some("delete"),
        "remove",
        on(|a| a.remove),
        Some("Del"),
    );
    ui.separator();
    ui.menu_button("Add to queue", |ui| {
        for q in &app.view.queues {
            if ui.button(q.as_str()).clicked() {
                *action = Some(format!("ctx_queue:{q}"));
                ui.close();
            }
        }
    });
    menu_item(
        ui,
        &p,
        action,
        "Delete from queue",
        None,
        "ctx_unqueue",
        queued,
        None,
    );
    ui.separator();
    menu_item(
        ui,
        &p,
        action,
        "Copy download address",
        None,
        "ctx_copy",
        true,
        None,
    );
    menu_item(
        ui,
        &p,
        action,
        "Properties",
        None,
        "ctx_properties",
        true,
        None,
    );
}

/// The Status cell (1.x widgets.py:916-935): a progress pill while running or paused (150 px, the done part
/// at alpha 70 with a 3 px edge), otherwise a coloured pill with its mark (✓, !, a shield). Returns its rect.
fn pill(
    ui: &Ui,
    painter: &egui::Painter,
    p: &Palette,
    pill: &Pill,
    cell: Rect,
    font: &FontId,
) -> Rect {
    let size = (font.size * 0.88).max(10.0);
    let h = 20.0_f32.min(cell.height() - 8.0);
    match pill.kind {
        "run" | "pause" => {
            let r = Rect::from_min_size(
                egui::pos2(cell.left() + 8.0, cell.center().y - h / 2.0),
                vec2(150.0_f32.min(cell.width() - 16.0), h),
            );
            let radius = CornerRadius::same((h / 2.0) as u8);
            painter.rect_filled(r, radius, p.track);
            let colour = if pill.kind == "run" { p.ac } else { p.fa };
            let fill = Rect::from_min_size(
                r.min,
                vec2((r.width() * pill.progress as f32).max(0.0), r.height()),
            );
            let clip = painter.with_clip_rect(fill.intersect(painter.clip_rect()));
            clip.rect_filled(r, radius, alpha(colour, 70.0 / 255.0));
            clip.rect_filled(
                Rect::from_min_size(r.min, vec2(3.0, h)),
                CornerRadius::ZERO,
                colour,
            );
            painter.text(
                r.center(),
                Align2::CENTER_CENTER,
                &pill.text,
                FontId::monospace(size),
                p.tx,
            );
            r
        }
        kind => {
            let (fg, bg) = match kind {
                "done" | "verified" => (p.ok, alpha(p.ok, 34.0 / 255.0)),
                "mismatch" | "error" => (p.bad, alpha(p.bad, 30.0 / 255.0)),
                _ => (p.mu, p.track),
            };
            let text = match kind {
                "error" => format!("! {}", pill.text),
                _ => pill.text.clone(),
            };
            // The marks are drawn, not taken from a font that may lack the glyph.
            let mark = match kind {
                "verified" => Some("shield-check"),
                "mismatch" => Some("shield-alert"),
                "done" => Some(""),
                _ => None,
            };
            let galley =
                painter.layout_no_wrap(text, FontId::new(size, egui::FontFamily::Proportional), fg);
            let pad = if mark.is_some() { 26.0 } else { 10.0 };
            let w = (galley.size().x + pad + 10.0).min(cell.width() - 8.0);
            let r = Rect::from_min_size(
                egui::pos2(cell.left() + 8.0, cell.center().y - h / 2.0),
                vec2(w, h),
            );
            painter.rect_filled(r, CornerRadius::same((h / 2.0) as u8), bg);
            let at = Rect::from_min_size(
                egui::pos2(r.left() + 9.0, r.center().y - 6.0),
                vec2(12.0, 12.0),
            );
            match mark {
                Some("") => check_mark(painter, at, fg),
                Some(m) => icon(m, 12.0, p, fg).paint_at(ui, at),
                None => {}
            }
            painter.galley(
                egui::pos2(r.left() + pad, r.center().y - galley.size().y / 2.0),
                galley,
                fg,
            );
            r
        }
    }
}

// ---------------------------------------------------------------------- details pane

fn details(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    // Fill the pane: a resizable panel otherwise shrinks to its contents.
    ui.set_min_size(ui.available_size());
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.max_rect().top(),
        Stroke::new(1.0, p.line),
    );
    let mut button: Option<(i64, String)> = None;
    let mut folder = None;
    // The tab strip on the header colour (1.x detailsTabs, 30 px).
    let strip = Rect::from_min_size(ui.max_rect().min, vec2(ui.max_rect().width(), 30.0));
    ui.painter().rect_filled(strip, CornerRadius::ZERO, p.hdr);
    ui.horizontal(|ui| {
        ui.set_min_height(28.0);
        ui.add_space(8.0);
        for (i, name) in ["Download status", "Connections", "Speed Limiter"]
            .iter()
            .enumerate()
        {
            let on = app.details_tab == i;
            let text = RichText::new(*name)
                .strong()
                .color(if on { p.actx } else { p.mu });
            if ui.add(egui::Button::new(text).frame(false)).clicked() {
                app.details_tab = i;
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if close_button(ui, &p, "Hide the details pane") {
                app.run("details");
            }
            let id = app.view.details.as_ref().map(|d| d.id);
            let open = ui
                .add_enabled(
                    id.is_some(),
                    egui::Button::image(icon("open-window", 13.0, &p, p.mu)).frame(false),
                )
                .on_hover_text("Open in a window");
            if open.clicked()
                && let Some(id) = id
            {
                crate::dialogs::progress::open(app, id, true);
            }
            ui.label(
                RichText::new("View → Details pane (F9)")
                    .color(p.fa)
                    .size(12.5),
            );
        });
    });
    ui.painter()
        .hline(strip.x_range(), strip.bottom(), Stroke::new(1.0, p.line));
    ui.add_space(4.0);
    let Some(d) = app.view.details.clone() else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new(
                    "Select a download to see its progress, connections and speed limit here.",
                )
                .color(p.fa),
            );
        });
        return;
    };
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(14, 6))
        .show(ui, |ui| match app.details_tab {
            0 => {
                // The stats column hides when the pane is narrower than 960 px (details.py:58, 125-127).
                let wide = ui.available_width() >= 960.0 - 28.0;
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(300.0);
                        ui.horizontal(|ui| {
                            // The category icon on a 52 px tile.
                            let (t, _) = ui.allocate_exact_size(vec2(52.0, 52.0), Sense::hover());
                            ui.painter().rect_filled(t, CornerRadius::same(12), p.win);
                            icon(&d.icon, 30.0, &p, p.tx)
                                .paint_at(ui, Rect::from_center_size(t.center(), vec2(30.0, 30.0)));
                            ui.vertical(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&d.filename).strong().size(15.0),
                                    )
                                    .truncate(),
                                );
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&d.url).monospace().size(12.0).color(p.actx),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(&d.url_full);
                            });
                        });
                        ui.add(egui::Label::new(RichText::new(&d.state).color(p.mu)).truncate());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            let first = if d.action_icon.is_empty() {
                                egui::Button::new(RichText::new(d.action_label).strong())
                            } else {
                                egui::Button::image_and_text(
                                    icon(d.action_icon, 13.0, &p, p.tx),
                                    RichText::new(d.action_label).strong(),
                                )
                            };
                            let compact = |b: egui::Button<'static>| {
                                b.min_size(vec2(0.0, 30.0))
                                    .corner_radius(CornerRadius::same(7))
                            };
                            if ui.add(compact(first)).clicked() {
                                button = Some((d.id, d.action.to_string()));
                            }
                            if ui
                                .add(compact(egui::Button::new(
                                    RichText::new("Open folder").strong(),
                                )))
                                .clicked()
                            {
                                folder = Some(d.id);
                            }
                        });
                    });
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        let stats = if wide { 330.0 + 24.0 } else { 0.0 };
                        ui.set_width((ui.available_width() - stats).max(160.0));
                        ui.label(RichText::new(&d.percent).monospace().size(26.0).strong());
                        ui.label(RichText::new(&d.of).color(p.mu));
                        bar(ui, &p, d.bar as f32, 8.0);
                        ui.add_space(4.0);
                        ui.label(RichText::new(d.caption).color(p.fa).size(12.0));
                        segments(ui, &p, d.size, &d.segments);
                    });
                    if !wide {
                        return;
                    }
                    ui.add_space(24.0);
                    // Two columns of caption over value (1.x details.py stats grid, 330 px).
                    let tone = match d.checksum_tone {
                        "okText" => p.ok,
                        "badText" => p.bad,
                        _ => p.tx,
                    };
                    let cells = [
                        ("Transfer rate", d.rate.as_str(), p.tx),
                        ("Time left", d.time_left.as_str(), p.tx),
                        ("Downloaded", d.done.as_str(), p.tx),
                        ("Connections", d.connections.as_str(), p.tx),
                        ("Resume", d.resume, p.ok),
                        ("Checksum", d.checksum.as_str(), tone),
                    ];
                    egui::Grid::new("stats")
                        .num_columns(2)
                        .min_col_width(156.0)
                        .spacing(vec2(18.0, 10.0))
                        .show(ui, |ui| {
                            for pair in cells.chunks(2) {
                                for (cap, val, ink) in pair {
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 2.0;
                                        ui.label(
                                            RichText::new(cap.to_uppercase())
                                                .color(p.fa)
                                                .size(12.0)
                                                .strong(),
                                        );
                                        ui.label(
                                            RichText::new(*val).monospace().strong().color(*ink),
                                        );
                                    });
                                }
                                ui.end_row();
                            }
                        });
                });
            }
            1 => {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("conns")
                        .num_columns(4)
                        .striped(true)
                        .spacing(vec2(24.0, 4.0))
                        .show(ui, |ui| {
                            for h in ["N.", "DOWNLOADED", "SPEED", "INFO"] {
                                ui.label(RichText::new(h).size(11.0).color(p.mu));
                            }
                            ui.end_row();
                            for c in &d.rows {
                                ui.label(&c.n);
                                ui.label(RichText::new(&c.downloaded).monospace());
                                ui.label(RichText::new(&c.speed).monospace());
                                ui.label(RichText::new(&c.info).color(if c.info == "Finished" {
                                    p.ok
                                } else {
                                    p.tx
                                }));
                                ui.end_row();
                            }
                        });
                });
            }
            _ => {
                let mut on = d.limit_on;
                let mut kbps = d.limit_kbps.max(1);
                let mut changed = ui
                    .checkbox(&mut on, "Use Speed Limiter for this download")
                    .changed();
                ui.horizontal(|ui| {
                    ui.label("Maximum download speed:");
                    changed |= ui
                        .add_enabled(on, egui::DragValue::new(&mut kbps).range(1..=10_000_000))
                        .changed();
                    ui.label("KBytes/sec");
                });
                if changed {
                    app.manager
                        .set_download_speed_limit(d.id, if on { kbps } else { 0 });
                    app.rebuild();
                }
            }
        });
    if let Some((id, action)) = button {
        app.details_button(id, &action);
    }
    if let Some(id) = folder {
        app.open_folder(id);
    }
}

/// A progress bar.
pub(crate) fn bar(ui: &mut Ui, p: &Palette, fraction: f32, height: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let radius = CornerRadius::same((height / 2.0) as u8);
    ui.painter().rect_filled(r, radius, p.track);
    let done = Rect::from_min_size(r.min, vec2(r.width() * fraction.clamp(0.0, 1.0), height));
    ui.painter()
        .with_clip_rect(done)
        .rect_filled(r, radius, p.ac);
}

/// The per-connection bar: each segment's downloaded part, with gaps between segments.
pub(crate) fn segments(ui: &mut Ui, p: &Palette, size: i64, segs: &[lider_core::models::Segment]) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 12.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(3), p.track);
    if size <= 0 {
        return;
    }
    let x = |pos: i64| r.left() + r.width() * (pos as f32 / size as f32).clamp(0.0, 1.0);
    for s in segs {
        let end = if s.end >= 0 { s.end } else { size };
        let done = s.pos.min(end);
        if done > s.start {
            let part = Rect::from_min_max(
                egui::pos2(x(s.start) + 1.0, r.top()),
                egui::pos2((x(done) - 1.0).max(x(s.start) + 2.0), r.bottom()),
            );
            ui.painter().rect_filled(
                part,
                CornerRadius::same(2),
                if done >= end { p.ok } else { p.ac },
            );
        }
    }
}
