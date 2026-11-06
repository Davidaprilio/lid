//! The `lider` binary. Like `lider/app.py` in 1.x, it picks the process mode from argv before any GUI
//! code runs: the native messaging host and the browser-integration CLI must never start the webview.

mod cli;
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
    // Same order and "anywhere in argv" matching as app.py:main. Browsers append their own arguments
    // (an origin, a manifest path) after --native-host; they are never URLs.
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

fn code(n: i32) -> ExitCode {
    ExitCode::from(n.clamp(0, 255) as u8)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match mode(&args) {
        Mode::NativeHost => code(lider_sys::native_host::main()),
        Mode::BrowserIntegration => code(lider_sys::browser::main(
            &args,
            &lider_sys::env::SysEnv::from_process(),
        )),
        Mode::Gui => {
            let cli = cli::parse(&args);
            if cli.help {
                print!("{}", cli::USAGE);
                return ExitCode::SUCCESS;
            }
            let socket = lider_sys::ipc::socket_name();
            // A running LID takes the URLs (or comes forward); this process then ends.
            match cli::lines_for_running(&cli) {
                None if lider_sys::ipc::is_running(&socket) => return ExitCode::SUCCESS,
                Some(lines) if lider_sys::ipc::send_lines(&lines, &socket) => {
                    return ExitCode::SUCCESS;
                }
                _ => {}
            }
            gui::run(cli, socket)
        }
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
        assert_eq!(
            mode(&args(&["--install-browser-integration", "--native-host"])),
            Mode::NativeHost
        );
    }
}
