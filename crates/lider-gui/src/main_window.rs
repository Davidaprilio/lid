//! The main window (phase E0: the tree, the download list with its status pills, the status bar).

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, RichText, Sense, Stroke,
    StrokeKind, Ui, vec2,
};
use lider_core::view::{Pill, TreeNode};

use crate::app::App;
use crate::icons::icon;
use crate::theme::{Palette, alpha};

const ROW: f32 = 30.0;

fn column_width(key: &str) -> f32 {
    match key {
        "_check" => 34.0,
        "queue" => 28.0,
        "filename" => 300.0,
        "status" => 170.0,
        "size" => 90.0,
        "description" | "save_to" | "referer" | "parent_page" => 200.0,
        _ => 110.0,
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    egui::Panel::bottom("status")
        .frame(
            egui::Frame::new()
                .fill(p.win)
                .inner_margin(egui::Margin::symmetric(10, 5)),
        )
        .show(ui, |ui| {
            if let Some(s) = &app.view.status {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&s.speed).monospace().color(p.mu));
                    ui.label(RichText::new(&s.counts).color(p.mu));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(&s.clipboard).color(p.mu));
                        ui.label(RichText::new(&s.limiter).color(p.mu));
                    });
                });
            }
        });
    egui::Panel::left("tree")
        .default_size(app.manager.settings.sidebar_width as f32)
        .frame(
            egui::Frame::new()
                .fill(p.win)
                .inner_margin(egui::Margin::same(8)),
        )
        .show(ui, |ui| {
            ui.label(RichText::new("CATEGORIES").size(11.0).strong().color(p.fa));
            ui.add_space(4.0);
            let nodes = app.view.tree.clone();
            let mut picked = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                for n in &nodes {
                    tree_node(ui, &p, n, 0, &app.filter, &mut picked);
                }
            });
            if let Some(f) = picked {
                app.set_filter(f);
            }
        });
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(p.pane))
        .show(ui, |ui| list(app, ui));
}

fn tree_node(
    ui: &mut Ui,
    p: &Palette,
    n: &TreeNode,
    depth: usize,
    current: &lider_core::view::TreeFilter,
    picked: &mut Option<lider_core::view::TreeFilter>,
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
    ui.painter().text(
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
    if resp.clicked() {
        *picked = Some(n.filter.clone());
    }
    if n.expanded {
        for c in &n.children {
            tree_node(ui, p, c, depth + 1, current, picked);
        }
    }
}

fn list(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let cols = app.view.columns.clone();
    // Header.
    let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    ui.painter().rect_filled(head, CornerRadius::ZERO, p.hdr);
    ui.painter()
        .hline(head.x_range(), head.bottom(), Stroke::new(1.0, p.line));
    let mut x = head.left();
    for c in &cols {
        let w = column_width(c);
        if c != "_check" {
            let title = lider_core::view::column_title(c).to_uppercase();
            ui.painter().text(
                egui::pos2(x + 8.0, head.center().y),
                Align2::LEFT_CENTER,
                title,
                FontId::proportional(11.0),
                p.mu,
            );
        }
        x += w;
    }
    // Rows.
    let rows = app.view.rows.len();
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show_rows(ui, ROW, rows, |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for i in range {
                let row = &app.view.rows[i];
                let (rect, resp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
                let selected = app.selected.contains(&row.id);
                let fill = if selected {
                    p.acsel
                } else if i % 2 == 1 {
                    p.alt
                } else {
                    p.pane
                };
                ui.painter().rect_filled(rect, CornerRadius::ZERO, fill);
                let mut x = rect.left();
                for (c, text) in cols.iter().zip(&row.cells) {
                    let w = column_width(c);
                    let cell = Rect::from_min_size(egui::pos2(x, rect.top()), vec2(w, ROW));
                    let clip = ui.painter().with_clip_rect(cell.shrink2(vec2(4.0, 0.0)));
                    match c.as_str() {
                        "_check" => {
                            let b = Rect::from_center_size(cell.center(), vec2(15.0, 15.0));
                            if row.checked {
                                clip.rect_filled(b, CornerRadius::same(4), p.ac);
                                clip.text(
                                    b.center(),
                                    Align2::CENTER_CENTER,
                                    "✓",
                                    FontId::proportional(11.0),
                                    Color32::WHITE,
                                );
                            } else {
                                clip.rect_stroke(
                                    b,
                                    CornerRadius::same(4),
                                    Stroke::new(1.0, p.line2),
                                    StrokeKind::Inside,
                                );
                            }
                        }
                        "queue" => {
                            if !row.queue.is_empty() {
                                icon("queue", 15.0, &p, p.mu).paint_at(
                                    ui,
                                    Rect::from_center_size(cell.center(), vec2(15.0, 15.0)),
                                );
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
                                FontId::proportional(13.0),
                                p.tx,
                            );
                        }
                        "status" => pill(&clip, &p, &row.pill, cell),
                        "size" | "rate" | "time_left" => {
                            clip.text(
                                egui::pos2(x + 8.0, cell.center().y),
                                Align2::LEFT_CENTER,
                                text,
                                FontId::monospace(12.0),
                                p.tx,
                            );
                        }
                        _ => {
                            clip.text(
                                egui::pos2(x + 8.0, cell.center().y),
                                Align2::LEFT_CENTER,
                                text,
                                FontId::proportional(13.0),
                                p.tx,
                            );
                        }
                    }
                    x += w;
                }
                if resp.clicked() {
                    app.selected = vec![row.id];
                }
            }
        });
}

/// The Status cell: a progress pill while running or paused, a coloured pill otherwise.
fn pill(painter: &egui::Painter, p: &Palette, pill: &Pill, cell: Rect) {
    let font = FontId::proportional(11.5);
    match pill.kind {
        "run" | "pause" => {
            let r = Rect::from_min_size(
                egui::pos2(cell.left() + 8.0, cell.center().y - 9.0),
                vec2(150.0, 18.0),
            );
            painter.rect_filled(r, CornerRadius::same(9), p.track);
            let fill = Rect::from_min_size(
                r.min,
                vec2((r.width() * pill.progress as f32).max(0.0), r.height()),
            );
            let colour = if pill.kind == "run" {
                alpha(p.ac, 0.35)
            } else {
                alpha(p.fa, 0.3)
            };
            painter
                .with_clip_rect(fill)
                .rect_filled(r, CornerRadius::same(9), colour);
            painter.text(
                r.center(),
                Align2::CENTER_CENTER,
                &pill.text,
                FontId::monospace(11.0),
                p.tx,
            );
        }
        kind => {
            let (fg, bg) = match kind {
                "done" | "verified" => (p.ok, alpha(p.ok, 0.13)),
                "mismatch" | "error" => (p.bad, alpha(p.bad, 0.12)),
                _ => (p.mu, p.track),
            };
            let galley = painter.layout_no_wrap(pill.text.clone(), font, fg);
            let r = Rect::from_min_size(
                egui::pos2(cell.left() + 8.0, cell.center().y - 9.0),
                vec2(galley.size().x + 16.0, 18.0),
            );
            painter.rect_filled(r, CornerRadius::same(9), bg);
            painter.galley(
                egui::pos2(r.left() + 8.0, r.center().y - galley.size().y / 2.0),
                galley,
                fg,
            );
        }
    }
}
