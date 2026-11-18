//! "Scheduler" (1.x `scheduler.SchedulerDialog`).

use crate::app::App;

/// Open the Scheduler on a queue.
pub fn open(app: &mut App, queue: &str) {
    let _ = queue;
    app.later("The Scheduler");
}
