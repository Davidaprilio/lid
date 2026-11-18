//! "Download All Links" and the grabber (1.x `add.LinksDialog`, `main_window._grabber`).

use serde_json::{Map, Value};

use crate::app::App;

/// Links to choose from: (address, text, kind) each; the chosen ones go to the Main queue.
pub fn open(
    app: &mut App,
    items: Vec<(String, String, String)>,
    _referer: String,
    _parent_page: String,
    _headers: Map<String, Value>,
) {
    app.later(&format!("Download All Links ({} links)", items.len()));
}

/// The grabber: an address whose page's links are listed.
pub fn grabber(app: &mut App) {
    app.later("The grabber");
}
