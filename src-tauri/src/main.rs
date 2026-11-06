//! The `lider` binary. Like `lider/app.py` in 1.x, it picks the process mode from argv before any GUI
//! code runs: the native messaging host and the browser-integration CLI must never start the webview.

mod gui;

use std::process::ExitCode;

/// What this process does, decided from argv alone.
#[derive(Debug, PartialEq, Eq)]
enum Mode {
    NativeHost,
    BrowserIntegration,
    Gui,
}

fn mode(args: &[String]) -> Mode {
    // Same order and "anywhere in argv" matching as app.py:main.
    if args.iter().any(|a| a == "--native-host") {
        Mode::NativeHost
    } else if args
        .iter()
        .any(|a| a == "--install-browser-integration" || a == "--uninstall-browser-integration")
    {
        Mode::BrowserIntegration
    } else {
        Mode::Gui
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match mode(&args) {
        // Phase 4 of docs/tauri/PLAN.md ports native_host.py and browser.py.
        Mode::NativeHost | Mode::BrowserIntegration => {
            eprintln!("lider {}: this mode is not ported yet", lider_core::VERSION);
            ExitCode::FAILURE
        }
        Mode::Gui => gui::run(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn dispatch_matches_app_py() {
        assert_eq!(mode(&args(&[])), Mode::Gui);
        assert_eq!(mode(&args(&["https://x/y.zip", "--minimized"])), Mode::Gui);
        // Browsers append the origin or manifest path after the flag.
        assert_eq!(
            mode(&args(&["--native-host", "chrome-extension://abc/"])),
            Mode::NativeHost
        );
        assert_eq!(mode(&args(&["x", "--native-host"])), Mode::NativeHost);
        assert_eq!(
            mode(&args(&["--install-browser-integration"])),
            Mode::BrowserIntegration
        );
        assert_eq!(
            mode(&args(&["--uninstall-browser-integration"])),
            Mode::BrowserIntegration
        );
        // --native-host wins over the integration flags, as in app.py.
        assert_eq!(
            mode(&args(&["--install-browser-integration", "--native-host"])),
            Mode::NativeHost
        );
    }
}
