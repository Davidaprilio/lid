//! "Scheduler" (1.x `scheduler.SchedulerDialog`): the queues on the left, the schedule and the files of
//! the selected queue on the right. Edits are always kept (switching queue, Start now, Apply, Close).

use egui::{RichText, Ui};
use lider_core::models::{MAIN_QUEUE, SYNC_QUEUE};
use lider_core::queue::{QueueSchedule, next_run_text, parse_hhmm};
use lider_core::view;

use super::kit;
use super::{Body, Cx, Dialog};
use crate::any_body;
use crate::app::App;
use crate::boxes::{Answer, Button, MsgBox, Then};
use crate::icons::icon;

const DAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

pub struct Scheduler {
    pub queue: String,
    pub sch: QueueSchedule,
    pub tab: usize,
    pub open_on: bool,
    /// "Once at" as typed (yyyy-mm-dd).
    pub date: String,
    /// The last good times (a bad one goes back to it when the field loses focus).
    good_start: String,
    good_stop: String,
    pub sel_item: Option<i64>,
}

/// Open the Scheduler on a queue.
pub fn open(app: &mut App, queue: &str) {
    let mut s = Scheduler {
        queue: String::new(),
        sch: QueueSchedule::default(),
        tab: 0,
        open_on: false,
        date: String::new(),
        good_start: String::new(),
        good_stop: String::new(),
        sel_item: None,
    };
    s.load(app, queue);
    app.open_dialog(Dialog::new("scheduler", "Scheduler", (860.0, 620.0), s));
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn valid_date(d: &str) -> bool {
    chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok() && d.len() == 10
}

fn valid_time(t: &str) -> bool {
    t.len() == 5 && parse_hhmm(t).is_some()
}

impl Scheduler {
    fn load(&mut self, app: &App, name: &str) {
        let name = if app.manager.queues.contains_key(name) {
            name
        } else {
            MAIN_QUEUE
        };
        self.queue = name.to_string();
        self.sch = app
            .manager
            .queues
            .get(name)
            .map(|q| q.schedule.clone())
            .unwrap_or_default();
        self.open_on = !self.sch.open_when_done.is_empty();
        self.date = if self.sch.once_date.is_empty() {
            today()
        } else {
            self.sch.once_date.clone()
        };
        self.good_start = self.sch.start_time.clone();
        self.good_stop = self.sch.stop_time.clone();
        self.sel_item = None;
    }

    /// The schedule as the tab shows it (once_date is "" when Daily is chosen).
    fn stored(&self) -> QueueSchedule {
        let mut s = self.sch.clone();
        s.once_date = if s.daily {
            String::new()
        } else if valid_date(&self.date) {
            self.date.clone()
        } else if !s.once_date.is_empty() {
            s.once_date
        } else {
            today()
        };
        if !self.open_on {
            s.open_when_done.clear();
        }
        if !valid_time(&s.start_time) {
            s.start_time = self.good_start.clone();
        }
        if !valid_time(&s.stop_time) {
            s.stop_time = self.good_stop.clone();
        }
        s.max_parallel = s.max_parallel.clamp(1, 64);
        s
    }

    pub fn apply(&self, app: &mut App) {
        let s = self.stored();
        if let Some(q) = app.manager.queues.get_mut(&self.queue) {
            q.schedule = s;
        }
        app.manager.save_queue(&self.queue);
        app.rebuild();
    }

    fn pick(&mut self, app: &mut App, name: &str) {
        self.apply(app);
        self.load(app, name);
    }

    fn queues(&self, app: &mut App, ui: &mut Ui, cx: &mut Cx) -> Option<String> {
        let p = cx.p;
        let mut pick = None;
        kit::caption(ui, &p, "Queues");
        kit::panel(ui, &p, |ui| {
            ui.set_min_height(440.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            for name in view::queue_names(&app.manager) {
                let count = app.manager.queue_items(&name).len();
                let sel = name == self.queue;
                ui.horizontal(|ui| {
                    let ic = if name == SYNC_QUEUE {
                        "queue-sync"
                    } else {
                        "queue"
                    };
                    ui.add(icon(ic, 16.0, &p, p.tx));
                    let w = ui.available_width() - 30.0;
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(w, 22.0), egui::Sense::hover());
                    let r = ui.interact(
                        rect,
                        ui.scope_id().with(("queue", &name)),
                        egui::Sense::click(),
                    );
                    r.widget_info(|| {
                        egui::WidgetInfo::selected(egui::Role::Button, true, sel, &name)
                    });
                    if sel {
                        ui.painter().rect_filled(rect, 4.0, p.acsel);
                    } else if r.hovered() {
                        ui.painter().rect_filled(rect, 4.0, p.track);
                    }
                    ui.painter()
                        .with_clip_rect(rect.shrink2(egui::vec2(2.0, 0.0)))
                        .text(
                            rect.left_center() + egui::vec2(4.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            &name,
                            egui::FontId::proportional(13.0),
                            if sel { p.actx } else { p.tx },
                        );
                    if r.clicked() && !sel {
                        pick = Some(name.clone());
                    }
                    if count > 0 {
                        ui.label(RichText::new(count.to_string()).small().color(p.mu));
                    }
                });
            }
        });
        pick
    }

    fn schedule_tab(&mut self, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let s = &mut self.sch;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            ui.radio_value(&mut s.periodic, false, "One-time downloading");
            ui.radio_value(&mut s.periodic, true, "Periodic synchronization");
        });
        ui.checkbox(&mut s.start_on_startup, "Start download on LID startup");
        ui.horizontal(|ui| {
            let c = ui.checkbox(&mut s.start_enabled, "Start download at");
            time_field(
                ui,
                &mut s.start_time,
                &mut self.good_start,
                s.start_enabled,
                c.id,
            );
        });
        ui.indent("start-when", |ui| {
            ui.add_enabled_ui(s.start_enabled, |ui| {
                ui.horizontal(|ui| {
                    let r = ui.radio_value(&mut s.daily, false, "Once at");
                    let e = kit::date_field(ui, &p, &mut self.date, !s.daily, r.id);
                    if e.lost_focus() && !valid_date(&self.date) {
                        self.date = if s.once_date.is_empty() {
                            today()
                        } else {
                            s.once_date.clone()
                        };
                    }
                });
                ui.radio_value(&mut s.daily, true, "Daily");
                ui.indent("days", |ui| {
                    ui.add_enabled_ui(s.daily, |ui| {
                        egui::Grid::new("days").num_columns(3).show(ui, |ui| {
                            for row in 0..3 {
                                for col in 0..3 {
                                    let i = col * 3 + row;
                                    if i < 7 {
                                        let mut on = s.days.contains(&(i as i64));
                                        if ui.checkbox(&mut on, DAYS[i]).changed() {
                                            if on {
                                                s.days.push(i as i64);
                                                s.days.sort();
                                            } else {
                                                s.days.retain(|d| *d != i as i64);
                                            }
                                        }
                                    }
                                }
                                ui.end_row();
                            }
                        });
                    });
                });
            });
        });
        if s.periodic {
            kit::row(ui, "Start again every", 140.0, |ui, by| {
                ui.add(egui::DragValue::new(&mut s.every_hours).range(0..=999))
                    .labelled_by(by);
                ui.label("hours");
                ui.add(egui::DragValue::new(&mut s.every_minutes).range(0..=59))
                    .labelled_by(by);
                ui.label("min");
            });
        }
        ui.horizontal(|ui| {
            let c = ui.checkbox(&mut s.stop_enabled, "Stop download at");
            time_field(ui, &mut s.stop_time, &mut self.good_stop, true, c.id);
        });
        ui.horizontal(|ui| {
            let c = ui.checkbox(
                &mut s.retries_enabled,
                "Number of retries for each file if downloading failed",
            );
            ui.add(egui::DragValue::new(&mut s.retries).range(0..=999))
                .labelled_by(c.id);
        });
        let c = ui.checkbox(&mut self.open_on, "Open the following file when done:");
        if self.open_on {
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                let w = ui.available_width() - 44.0;
                ui.add(
                    egui::TextEdit::singleline(&mut s.open_when_done)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(w),
                )
                .labelled_by(c.id);
                if ui.button("...").clicked()
                    && let Some(f) = rfd::FileDialog::new()
                        .set_title("Open the following file when done")
                        .pick_file()
                {
                    s.open_when_done = f.to_string_lossy().into_owned();
                }
            });
        }
        ui.checkbox(&mut s.exit_when_done, "Exit LID when done");
        ui.checkbox(&mut s.shutdown_when_done, "Turn off computer when done");
        let stored = self.stored();
        if let Some(t) = next_run_text(&stored, chrono::Local::now().naive_local()) {
            kit::accent_note(ui, &p, "clock", RichText::new(t).color(p.actx));
        }
    }

    fn files_tab(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        kit::row(ui, "Download", 70.0, |ui, by| {
            ui.add(egui::DragValue::new(&mut self.sch.max_parallel).range(1..=64))
                .labelled_by(by);
            ui.label("files at the same time");
        });
        let items: Vec<(i64, String, String, String, String)> = app
            .manager
            .queue_items(&self.queue)
            .iter()
            .map(|d| {
                let st = lider_core::fmt::status(d);
                (
                    d.id,
                    d.filename.clone(),
                    view::category_icon(&d.category),
                    lider_core::fmt::size(d.size),
                    if st.is_empty() { "Queued".into() } else { st },
                )
            })
            .collect();
        kit::panel(ui, &p, |ui| {
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("queue-files")
                        .num_columns(3)
                        .striped(true)
                        .min_col_width(90.0)
                        .show(ui, |ui| {
                            for h in ["FILE NAME", "SIZE", "STATUS"] {
                                ui.label(RichText::new(h).small().strong().color(p.mu));
                            }
                            ui.end_row();
                            for (id, name, ic, size, status) in &items {
                                ui.horizontal(|ui| {
                                    ui.add(icon(ic, 16.0, &p, p.tx));
                                    let sel = self.sel_item == Some(*id);
                                    if ui.selectable_label(sel, name).clicked() {
                                        self.sel_item = Some(*id);
                                    }
                                });
                                ui.label(RichText::new(size).monospace());
                                if status == "Queued" {
                                    kit::pill(ui, &p, "Queued", "muted");
                                } else {
                                    ui.label(status);
                                }
                                ui.end_row();
                            }
                        });
                });
        });
        ui.horizontal(|ui| {
            let b = |ui: &mut Ui, ic: &str, tip: &str| {
                ui.add(egui::Button::image(icon(ic, 14.0, &p, p.tx)))
                    .on_hover_text(tip)
                    .clicked()
            };
            let up = b(ui, "up", "Move up");
            let down = b(ui, "down-arrow", "Move down");
            let remove = b(ui, "remove", "Delete from queue");
            if let Some(id) = self.sel_item {
                if up {
                    app.manager.move_in_queue(id, -1);
                } else if down {
                    app.manager.move_in_queue(id, 1);
                } else if remove {
                    app.manager.set_queue(&[id], "");
                    self.sel_item = None;
                }
                if up || down || remove {
                    app.rebuild();
                }
            }
        });
    }
}

/// A "HH:MM" field; a bad time goes back to the last good one when it loses focus.
fn time_field(ui: &mut Ui, text: &mut String, good: &mut String, enabled: bool, by: egui::Id) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let r = ui
            .add_enabled(
                enabled,
                egui::TextEdit::singleline(text)
                    .font(egui::TextStyle::Monospace)
                    .char_limit(5)
                    .desired_width(60.0),
            )
            .labelled_by(by);
        if r.gained_focus() && valid_time(text) {
            *good = text.clone();
        }
        if r.lost_focus() {
            if valid_time(text) {
                *good = text.clone();
            } else {
                *text = good.clone();
            }
        }
    });
}

impl Body for Scheduler {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.p;
        let mut pick = None;
        let mut action: Option<&str> = None;
        kit::body(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(230.0);
                    pick = self.queues(app, ui, cx);
                    ui.horizontal(|ui| {
                        if kit::plain(ui, &p, "&New queue") {
                            action = Some("new");
                        }
                        if kit::plain(ui, &p, "&Delete") {
                            action = Some("delete");
                        }
                    });
                });
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&self.queue).size(15.0).strong());
                        let running = app
                            .manager
                            .queues
                            .get(&self.queue)
                            .is_some_and(|q| q.running);
                        if running {
                            kit::pill(ui, &p, "Running", "accent");
                        } else {
                            kit::pill(ui, &p, "Stopped", "muted");
                        }
                    });
                    kit::panel(ui, &p, |ui| {
                        kit::tab_strip(ui, &p, &["Schedule", "Files in the queue"], &mut self.tab);
                        ui.set_min_height(440.0);
                        if self.tab == 0 {
                            self.schedule_tab(ui, cx);
                        } else {
                            self.files_tab(app, ui, cx);
                        }
                    });
                });
            });
        });
        kit::footer(ui, &p, |ui| {
            if kit::icon_button(ui, &p, "play-white", "&Start now", kit::Look::Primary) {
                action = Some("start");
            }
            if kit::plain(ui, &p, "S&top") {
                action = Some("stop");
            }
            kit::right(ui, |ui| {
                if kit::plain(ui, &p, "&Close") {
                    action = Some("close");
                }
                if kit::plain(ui, &p, "&Apply") {
                    action = Some("apply");
                }
            });
        });
        if action.is_none() && cx.escape(ui) {
            action = Some("close");
        }
        if let Some(name) = pick {
            self.pick(app, &name);
        }
        match action {
            Some("new") => {
                *cx.boxx = Some(MsgBox::input(
                    "Enter queue name",
                    "Enter a name for the new queue that will be displayed in the list of queues",
                    Then::Nothing,
                ));
            }
            Some("delete") => {
                if self.queue == MAIN_QUEUE || self.queue == SYNC_QUEUE {
                    *cx.boxx = Some(MsgBox::info("LID", "The default queues cannot be deleted."));
                } else {
                    *cx.boxx = Some(
                        MsgBox::question(
                            "LID",
                            &format!("Delete queue \"{}\"?", self.queue),
                            Then::Nothing,
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
                }
            }
            Some("start") => {
                self.apply(app);
                let events = app.manager.start_queue(&self.queue);
                app.handle_events(&events);
                app.rebuild();
            }
            Some("stop") => {
                app.manager.stop_queue(&self.queue);
                app.rebuild();
            }
            Some("apply") => self.apply(app),
            Some("close") => {
                self.apply(app);
                cx.close = true;
            }
            _ => {}
        }
    }

    fn answer(&mut self, app: &mut App, a: Answer, _cx: &mut Cx) {
        match a.button {
            // New queue: a name that is not taken adds and selects it.
            "ok" => {
                let name = a.value.trim().to_string();
                if !name.is_empty() && !app.manager.queues.contains_key(&name) {
                    self.apply(app);
                    app.manager.add_queue(&name);
                    self.load(app, &name);
                    app.rebuild();
                }
            }
            "yes" => {
                let name = self.queue.clone();
                app.manager.delete_queue(&name);
                self.load(app, MAIN_QUEUE);
                app.rebuild();
            }
            _ => {}
        }
    }
    any_body!();
}
