//! Command-line arguments of GUI mode (argparse in `app.py` in 1.x).

pub const USAGE: &str = "usage: lider [-h] [--minimized] [urls ...]

LID: Linux Internet Downloader

positional arguments:
  urls         addresses to download

options:
  -h, --help   show this help message and exit
  --minimized  start hidden in the system tray
";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Cli {
    pub urls: Vec<String>,
    pub minimized: bool,
    pub help: bool,
}

/// URLs and flags. Other options (1.x passed them on to Qt) are ignored.
pub fn parse(args: &[String]) -> Cli {
    let mut cli = Cli::default();
    let mut only_urls = false;
    for a in args {
        match a.as_str() {
            _ if only_urls => cli.urls.push(a.clone()),
            "--" => only_urls = true,
            "-h" | "--help" => cli.help = true,
            "--minimized" => cli.minimized = true,
            s if s.starts_with('-') => {}
            _ => cli.urls.push(a.clone()),
        }
    }
    cli
}

/// What a second start does when LID already runs (decision Q1: `--minimized` alone does nothing).
pub fn lines_for_running(cli: &Cli) -> Option<Vec<String>> {
    if cli.urls.is_empty() && cli.minimized {
        None
    } else {
        Some(cli.urls.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn urls_and_flags() {
        let c = parse(&a(&[
            "https://x/1.zip",
            "--minimized",
            "-platform",
            "https://x/2.zip",
        ]));
        assert_eq!(c.urls, ["https://x/1.zip", "https://x/2.zip"]); // toolkit options are ignored
        assert!(c.minimized && !c.help);
        assert!(parse(&a(&["-h"])).help);
        assert_eq!(parse(&a(&["--", "--weird-name"])).urls, ["--weird-name"]);
    }

    #[test]
    fn minimized_alone_does_not_show_a_running_lid() {
        assert_eq!(lines_for_running(&parse(&a(&["--minimized"]))), None);
        assert_eq!(lines_for_running(&parse(&a(&[]))), Some(vec![]));
        assert_eq!(
            lines_for_running(&parse(&a(&["u", "--minimized"]))),
            Some(vec!["u".into()])
        );
    }
}
