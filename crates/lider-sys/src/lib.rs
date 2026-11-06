//! Linux integration of LID: single-instance IPC, the native messaging host, browser setup and
//! desktop entries. Nothing here may start the GUI; the native host must start fast.

pub mod browser;
pub mod desktop;
pub mod env;
pub mod ipc;
pub mod native_host;
pub mod sh;
