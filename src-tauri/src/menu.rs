//! The menus of the main window (`main_window.py` `_build_actions`/`_build_menus` in 1.x), described once
//! and used for both the native menu bar and the one drawn in the window. Labels keep the `&` of the
//! Alt mnemonic; ids are what `menu_action` receives.

use lider_core::manager::DownloadManager;
use lider_core::view::{ActionState, queue_names};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Item {
    Action {
        id: String,
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        shortcut: Option<&'static str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        icon: Option<&'static str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tip: Option<&'static str>,
        enabled: bool,
        /// Some for checkable items.
        #[serde(skip_serializing_if = "Option::is_none")]
        checked: Option<bool>,
    },
    Separator,
    Submenu {
        label: String,
        items: Vec<Item>,
    },
}

fn action(id: &str, label: &str) -> Item {
    Item::Action {
        id: id.into(),
        label: label.into(),
        shortcut: None,
        icon: None,
        tip: None,
        enabled: true,
        checked: None,
    }
}

impl Item {
    fn key(mut self, s: &'static str) -> Item {
        if let Item::Action { shortcut, .. } = &mut self {
            *shortcut = Some(s);
        }
        self
    }
    fn icon(mut self, name: &'static str, text: &'static str) -> Item {
        if let Item::Action { icon, tip, .. } = &mut self {
            *icon = Some(name);
            *tip = Some(text);
        }
        self
    }
    fn on(mut self, yes: bool) -> Item {
        if let Item::Action { enabled, .. } = &mut self {
            *enabled = yes;
        }
        self
    }
    fn check(mut self, yes: bool) -> Item {
        if let Item::Action { checked, .. } = &mut self {
            *checked = Some(yes);
        }
        self
    }
}

fn sub(label: &str, items: Vec<Item>) -> Item {
    Item::Submenu {
        label: label.into(),
        items,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Menu {
    pub label: String,
    pub items: Vec<Item>,
}

/// "View → Arrange files" items: label and sort key.
pub const ARRANGE: [(&str, &str); 11] = [
    ("By &Order Of Addition", "date_added"),
    ("By &File Name", "filename"),
    ("By &Size", "size"),
    ("By St&atus", "status"),
    ("By &Time Left", "time_left"),
    ("By Transfer &Rate", "rate"),
    ("By &Last Try Date", "last_try"),
    ("By &Description", "description"),
    ("By Save &Path", "save_to"),
    ("By R&eferer", "referer"),
    ("By Parent &Web Page", "parent_page"),
];

/// The five menus with the enabled and checked states of now.
pub fn menus(m: &DownloadManager, a: &ActionState) -> Vec<Menu> {
    let s = &m.settings;
    let queues = queue_names(m);
    let running = |q: &str| m.queues.get(q).is_some_and(|x| x.running);
    let tasks = vec![
        action("add", "&Add new download")
            .key("Ctrl+N")
            .icon("add-url", "Add new download address manually"),
        action("batch", "Add &batch download"),
        action("batch_clipboard", "Add batch download from &clipboard"),
        action("grabber", "Run site &grabber").icon("grabber", "Run site grabber"),
        Item::Separator,
        sub(
            "E&xport",
            vec![
                action("export_ef2", "To .ef2 export &file"),
                action("export_text", "To &text file"),
            ],
        ),
        sub(
            "&Import",
            vec![
                action("import_ef2", "&From .ef2 export file"),
                action("import_text", "From &text file"),
            ],
        ),
        Item::Separator,
        action("exit", "&Exit").key("Ctrl+Q"),
    ];
    let file = vec![
        action("stop", "&Stop Download")
            .icon("stop", "Stop the checked (or selected) downloads")
            .on(a.stop),
        action("remove", "&Remove")
            .key("Del")
            .icon(
                "delete",
                "Remove the checked (or selected) files from the list",
            )
            .on(a.remove),
        action("resume", "&Download Now")
            .icon(
                "resume",
                "Start or resume the checked (or selected) downloads",
            )
            .on(a.resume),
        action("redownload", "R&edownload").on(a.redownload),
    ];
    let downloads = vec![
        action("pause_all", "&Pause All"),
        action("stop_all", "&Stop All")
            .icon("stop-all", "Stop all downloads")
            .on(a.stop_all),
        Item::Separator,
        action("delete_completed", "&Delete All Completed")
            .icon(
                "delete-completed",
                "Remove all completed files from the list",
            )
            .on(a.delete_completed),
        Item::Separator,
        action("find", "Find (Ctrl-F)")
            .key("Ctrl+F")
            .icon("search", "Find"),
        action("find_next", "Find Next (F3)").key("F3"),
        Item::Separator,
        action("scheduler", "S&cheduler").icon("scheduler", "Scheduler and download queues"),
        sub(
            "Start &queue",
            queues
                .iter()
                .map(|q| action(&format!("start_queue:{q}"), q).on(!running(q)))
                .collect(),
        ),
        sub(
            "S&top queue",
            queues
                .iter()
                .map(|q| action(&format!("stop_queue:{q}"), q).on(running(q)))
                .collect(),
        ),
        Item::Separator,
        sub(
            "Speed &Limiter",
            vec![
                action("limiter_on", "Turn o&n").check(s.speed_limiter_on),
                action("limiter_off", "Turn of&f").check(!s.speed_limiter_on),
                Item::Separator,
                action("limiter_settings", "S&ettings..."),
            ],
        ),
        Item::Separator,
        action("options", "&Options").icon(
            "options",
            "Integration, file types, save folders, connections, proxy and others",
        ),
    ];
    let theme = |key: &str| !s.follow_system && s.theme == key;
    let view = vec![
        action("hide_categories", "Hide &categories").check(!s.show_categories),
        action("details", "&Details pane")
            .key("F9")
            .check(s.show_details),
        action("statusbar", "Status &bar").check(s.show_statusbar),
        Item::Separator,
        sub(
            "&Arrange files",
            ARRANGE
                .iter()
                .map(|(l, k)| action(&format!("arrange:{k}"), l).check(s.sort_key == *k))
                .collect(),
        ),
        sub(
            "&Toolbar",
            vec![
                action("toolbar_labels", "Icons and &labels").check(s.toolbar_labels),
                action("toolbar_icons", "&Icons only").check(!s.toolbar_labels),
            ],
        ),
        sub(
            "&Menu bar",
            vec![
                action("menubar:native", "&Native (desktop menu)").check(s.menubar == "native"),
                action("menubar:custom", "In the &window").check(s.menubar == "custom"),
                action("menubar:both", "&Both").check(s.menubar == "both"),
            ],
        ),
        action("columns", "C&ustomize URL List..."),
        sub(
            "T&heme",
            vec![
                action("theme:system", "Follow the system").check(s.follow_system),
                Item::Separator,
                action("theme:ember", "Ember").check(theme("ember")),
                action("theme:ember-dark", "Ember Dark").check(theme("ember-dark")),
                action("theme:dark", "Dark").check(theme("dark")),
            ],
        ),
        sub(
            "Font",
            vec![
                action("font_choose", "Choose Font..."),
                action("font_reset", "Reset to default Font"),
            ],
        ),
        Item::Separator,
        action("appearance", "A&ppearance..."),
    ];
    let help = vec![action("about", "&About LID")];
    [
        ("&Tasks", tasks),
        ("&File", file),
        ("&Downloads", downloads),
        ("&View", view),
        ("&Help", help),
    ]
    .into_iter()
    .map(|(label, items)| Menu {
        label: label.into(),
        items,
    })
    .collect()
}

/// The menu as Tauri's native menu bar.
pub fn native<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    menus: &[Menu],
) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{
        CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, Submenu,
        SubmenuBuilder,
    };

    fn accel(s: &str) -> String {
        s.replace("Ctrl+", "CmdOrCtrl+").replace("Del", "Delete")
    }
    fn build<R: tauri::Runtime>(
        app: &tauri::AppHandle<R>,
        label: &str,
        items: &[Item],
    ) -> tauri::Result<Submenu<R>> {
        let mut b = SubmenuBuilder::new(app, label);
        for it in items {
            b = match it {
                Item::Separator => b.item(&PredefinedMenuItem::separator(app)?),
                Item::Submenu { label, items } => b.item(&build(app, label, items)?),
                Item::Action {
                    id,
                    label,
                    shortcut,
                    enabled,
                    checked,
                    ..
                } => match checked {
                    Some(c) => {
                        let mut i = CheckMenuItemBuilder::with_id(id.as_str(), label)
                            .enabled(*enabled)
                            .checked(*c);
                        if let Some(k) = shortcut {
                            i = i.accelerator(accel(k));
                        }
                        b.item(&i.build(app)?)
                    }
                    None => {
                        let mut i = MenuItemBuilder::with_id(id.as_str(), label).enabled(*enabled);
                        if let Some(k) = shortcut {
                            i = i.accelerator(accel(k));
                        }
                        b.item(&i.build(app)?)
                    }
                },
            };
        }
        b.build()
    }
    let mut bar = MenuBuilder::new(app);
    for m in menus {
        bar = bar.item(&build(app, &m.label, &m.items)?);
    }
    bar.build()
}
