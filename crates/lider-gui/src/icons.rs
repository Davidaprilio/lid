//! LID's "Signal" icon set (lider/ui/assets/icons), recoloured per theme like icons.py: the ink follows
//! the text colour, the set's blue becomes the theme's accent. Some icons keep their own colours.

use egui::{Color32, Image, ImageSource, Vec2};
use include_dir::{Dir, include_dir};

use crate::theme::{Palette, hex_text};

static ICONS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../lider/ui/assets/icons");
const FIXED: [&str; 7] = [
    "add-white",
    "play-white",
    "finished",
    "shield-check",
    "delete",
    "shield-alert",
    "app",
];

fn svg(name: &str) -> &'static str {
    let key = match name {
        "download" | "downloads" => "folder-download",
        "folder-open" => "folder",
        n => n,
    };
    ICONS
        .get_file(format!("{key}.svg"))
        .or_else(|| ICONS.get_file("folder.svg"))
        .and_then(|f| f.contents_utf8())
        .unwrap_or("")
}

/// The icon `name` at `size` points; `ink` is the colour of its lines (the text colour around it).
pub fn icon(name: &str, size: f32, p: &Palette, ink: Color32) -> Image<'static> {
    let mut text = svg(name).to_string();
    let fixed = FIXED.contains(&name);
    if !fixed {
        text = text
            .replace("#1E293B", &hex_text(ink))
            .replace("#2563EB", &hex_text(p.ac));
    }
    // One image per (icon, colours): egui caches by this address.
    let uri = format!(
        "bytes://lid-icon/{name}-{}-{}.svg",
        &hex_text(ink)[1..],
        &hex_text(p.ac)[1..]
    );
    Image::new(ImageSource::Bytes {
        uri: uri.into(),
        bytes: text.into_bytes().into(),
    })
    .fit_to_exact_size(Vec2::splat(size))
}
