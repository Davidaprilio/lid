//! Adding downloads: "Enter new address to download", "Download File Info", "Duplicate download link",
//! "Batch download" (1.x `add.py`).

use crate::app::{App, Request};

/// "Enter new address to download" (Tasks → Add new download).
pub fn add_url(app: &mut App) {
    app.later("Enter new address to download");
}

/// An address from another start, the browser, the clipboard or a paste: the add flow (probe, the
/// duplicate rule, Download File Info).
pub fn from_request(app: &mut App, r: Request) {
    if let Request::AddUrl { url, .. } = r {
        app.later(&format!("Download File Info for {url}"));
    }
}

/// "Batch download".
pub fn batch(app: &mut App) {
    app.later("Batch download");
}
