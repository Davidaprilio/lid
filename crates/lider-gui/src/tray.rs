//! The tray icon (1.x `_build_tray`): a click opens LID; its menu has "Open LID", "Add new download", the
//! speed limiter and "Exit". On Linux it is a StatusNotifierItem over D-Bus (ksni, no GTK); on Windows the
//! shell's notification-area icon (tray-icon). What the user picks comes back as a [`TrayEvent`].

use std::sync::mpsc::Sender;

/// What the icon asks the window to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEvent {
    /// A click on the icon or "Open LID".
    Open,
    /// A menu action id of the main window (`add`, `limiter_on`, `limiter_off`, `exit`).
    Run(String),
}

/// Wakes the window when the icon sends something (it may be hidden and idle).
pub type Wake = std::sync::Arc<dyn Fn() + Send + Sync>;

/// The icon as RGBA pixels, from the app icon.
fn pixels() -> Option<(u32, u32, Vec<u8>)> {
    let img = image::load_from_memory(include_bytes!("../icons/64x64.png")).ok()?;
    let rgba = img.to_rgba8();
    Some((rgba.width(), rgba.height(), rgba.into_raw()))
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{TrayEvent, Wake};
    use ksni::blocking::TrayMethods;
    use ksni::menu::{CheckmarkItem, StandardItem, SubMenu};
    use std::sync::mpsc::Sender;

    struct Item {
        tx: Sender<TrayEvent>,
        wake: Wake,
        tip: String,
        limiter_on: bool,
        icon: Vec<ksni::Icon>,
    }

    impl Item {
        fn send(&self, e: TrayEvent) {
            let _ = self.tx.send(e);
            (self.wake)();
        }
    }

    fn standard(label: &str, e: TrayEvent) -> ksni::MenuItem<Item> {
        StandardItem {
            label: label.into(),
            activate: Box::new(move |t: &mut Item| t.send(e.clone())),
            ..Default::default()
        }
        .into()
    }

    impl ksni::Tray for Item {
        fn id(&self) -> String {
            "lider".into()
        }
        fn title(&self) -> String {
            "LID".into()
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            self.icon.clone()
        }
        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: "LID".into(),
                description: self.tip.clone(),
                ..Default::default()
            }
        }
        fn activate(&mut self, _x: i32, _y: i32) {
            self.send(TrayEvent::Open);
        }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            let on = self.limiter_on;
            vec![
                standard("Open LID", TrayEvent::Open),
                standard("_Add new download", TrayEvent::Run("add".into())),
                ksni::MenuItem::Separator,
                SubMenu {
                    label: "Speed Limiter".into(),
                    submenu: vec![
                        CheckmarkItem {
                            label: "Turn on".into(),
                            checked: on,
                            activate: Box::new(|t: &mut Item| {
                                t.send(TrayEvent::Run("limiter_on".into()))
                            }),
                            ..Default::default()
                        }
                        .into(),
                        CheckmarkItem {
                            label: "Turn off".into(),
                            checked: !on,
                            activate: Box::new(|t: &mut Item| {
                                t.send(TrayEvent::Run("limiter_off".into()))
                            }),
                            ..Default::default()
                        }
                        .into(),
                    ],
                    ..Default::default()
                }
                .into(),
                ksni::MenuItem::Separator,
                standard("_Exit", TrayEvent::Run("exit".into())),
            ]
        }
    }

    pub struct Tray(ksni::blocking::Handle<Item>);

    impl Tray {
        pub fn start(tx: Sender<TrayEvent>, wake: Wake, limiter_on: bool) -> Option<Tray> {
            // SNI wants ARGB32 in network byte order.
            let icon = super::pixels()
                .map(|(w, h, rgba)| {
                    let data = rgba
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .flat_map(|p| [p[3], p[0], p[1], p[2]])
                        .collect();
                    vec![ksni::Icon {
                        width: w as i32,
                        height: h as i32,
                        data,
                    }]
                })
                .unwrap_or_default();
            let item = Item {
                tx,
                wake,
                tip: String::new(),
                limiter_on,
                icon,
            };
            // No StatusNotifierWatcher (a desktop without a tray): no icon, so closing never hides LID.
            item.spawn().ok().map(Tray)
        }

        pub fn set_tooltip(&self, text: &str) {
            if self.0.update(|t| t.tip != text).unwrap_or(false) {
                let text = text.to_string();
                self.0.update(|t| t.tip = text);
            }
        }

        pub fn set_limiter(&self, on: bool) {
            if self.0.update(|t| t.limiter_on != on).unwrap_or(false) {
                self.0.update(|t| t.limiter_on = on);
            }
        }
    }

    impl Drop for Tray {
        fn drop(&mut self) {
            self.0.shutdown().wait();
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::{TrayEvent, Wake};
    use std::sync::mpsc::Sender;
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
    use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    pub struct Tray {
        icon: TrayIcon,
        on: CheckMenuItem,
        off: CheckMenuItem,
        tip: std::cell::RefCell<String>,
    }

    impl Tray {
        /// Created on the window's thread (the shell icon needs its message loop).
        pub fn start(tx: Sender<TrayEvent>, wake: Wake, limiter_on: bool) -> Option<Tray> {
            let menu = Menu::new();
            let open = MenuItem::with_id("tray_open", "Open LID", true, None);
            let add = MenuItem::with_id("add", "&Add new download", true, None);
            let on = CheckMenuItem::with_id("limiter_on", "Turn on", true, limiter_on, None);
            let off = CheckMenuItem::with_id("limiter_off", "Turn off", true, !limiter_on, None);
            let limiter = Submenu::with_items("Speed Limiter", true, &[&on, &off]).ok()?;
            let exit = MenuItem::with_id("exit", "&Exit", true, None);
            menu.append_items(&[
                &open,
                &add,
                &PredefinedMenuItem::separator(),
                &limiter,
                &PredefinedMenuItem::separator(),
                &exit,
            ])
            .ok()?;
            let (w, h, rgba) = super::pixels()?;
            let icon = TrayIconBuilder::new()
                .with_tooltip("LID")
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false)
                .with_icon(tray_icon::Icon::from_rgba(rgba, w, h).ok()?)
                .build()
                .ok()?;
            let (t1, w1) = (tx.clone(), wake.clone());
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                let id = e.id().0.clone();
                let _ = t1.send(if id == "tray_open" {
                    TrayEvent::Open
                } else {
                    TrayEvent::Run(id)
                });
                w1();
            }));
            TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = e
                {
                    let _ = tx.send(TrayEvent::Open);
                    wake();
                }
            }));
            Some(Tray {
                icon,
                on,
                off,
                tip: Default::default(),
            })
        }

        pub fn set_tooltip(&self, text: &str) {
            if *self.tip.borrow() != text {
                let _ = self.icon.set_tooltip(Some(text));
                *self.tip.borrow_mut() = text.to_string();
            }
        }

        pub fn set_limiter(&self, on: bool) {
            self.on.set_checked(on);
            self.off.set_checked(!on);
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    use super::{TrayEvent, Wake};
    use std::sync::mpsc::Sender;

    pub struct Tray;

    impl Tray {
        pub fn start(_tx: Sender<TrayEvent>, _wake: Wake, _limiter_on: bool) -> Option<Tray> {
            None
        }
        pub fn set_tooltip(&self, _text: &str) {}
        pub fn set_limiter(&self, _on: bool) {}
    }
}

pub use imp::Tray;

/// Start the icon; None when the desktop has no tray.
pub fn start(tx: Sender<TrayEvent>, wake: Wake, limiter_on: bool) -> Option<Tray> {
    Tray::start(tx, wake, limiter_on)
}
