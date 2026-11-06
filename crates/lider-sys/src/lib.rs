//! LID's side of the operating system: the single instance, the browser helper (native messaging host),
//! browser setup, the application menu and desktop services. Nothing here may start the GUI; the native
//! host must start fast.
//!
//! The OS-specific code sits behind the ports in [`platform`]; [`browser`] and [`native_host`] are the
//! same everywhere.

pub mod browser;
pub mod native_host;
pub mod platform;
pub mod sh;
