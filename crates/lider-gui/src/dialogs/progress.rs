//! The progress window of a download (1.x `progress.ProgressDialog`).

use crate::app::App;

/// The window's label for a download.
pub fn label(id: i64) -> String {
    format!("progress-{id}")
}

/// "Options on completion" of a progress window (show Download complete, exit, turn off).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletionOptions {
    pub show_complete: bool,
    pub exit: bool,
    pub shutdown: bool,
}

/// Open (or bring forward) the progress window of `id`, unless Options turn it off (`force` ignores that).
pub fn open(app: &mut App, id: i64, force: bool) {
    let _ = (app, id, force);
}

/// What the open progress window of `id` asks for when the download completes (None without one).
pub fn completion_options(app: &mut App, id: i64) -> Option<CompletionOptions> {
    let _ = (app, id);
    None
}
