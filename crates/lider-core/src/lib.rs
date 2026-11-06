//! LID core. Like `lider/core/` in 1.x it never touches the GUI: the app drives it by polling.

/// Short name shown in the UI.
pub const APP_NAME: &str = "LID";
/// Long name, used in the window title and the About box.
pub const APP_LONG_NAME: &str = "Linux Internet Downloader";
/// Package, command, data folder and desktop file name.
pub const PACKAGE_NAME: &str = "lider";
/// The version of LID, from the workspace `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_1x() {
        assert_eq!(APP_NAME, "LID");
        assert_eq!(APP_LONG_NAME, "Linux Internet Downloader");
        assert_eq!(PACKAGE_NAME, "lider");
        assert!(VERSION.starts_with("2."));
    }
}
