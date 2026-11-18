//! "Browser integration" (1.x `browsers.BrowserIntegrationDialog`).

use crate::app::App;

pub fn open(app: &mut App) {
    app.later("Browser integration");
}
