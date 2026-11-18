//! "Check hash" (1.x `checkhash.CheckHashDialog`).

use crate::app::App;

pub fn open(app: &mut App, id: i64) {
    let _ = id;
    app.later("Check hash");
}
