//! "LID options" with its seven tabs (1.x `options.OptionsDialog`).

use crate::app::App;

/// Open Options on a tab ("" for the first, "Appearance" from View → Appearance...).
pub fn open(app: &mut App, tab: &str) {
    let _ = tab;
    app.later("LID options");
}
