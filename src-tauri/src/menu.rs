//! The native (GTK) menu bar built from the menu description in `lider_core::menu`.

pub use lider_core::menu::*;

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
