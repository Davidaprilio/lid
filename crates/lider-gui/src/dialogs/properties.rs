//! "File Properties" (1.x `properties.PropertiesDialog`).

use crate::app::App;

pub fn open(app: &mut App, id: i64) {
    let _ = id;
    app.later("File Properties");
}
