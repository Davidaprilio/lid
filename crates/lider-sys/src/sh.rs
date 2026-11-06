//! Shell quoting as Python's `shlex.quote` / `shlex.join` (the wrapper scripts and `LIDER_LAUNCH`).

/// `shlex.quote`: unchanged when it only has safe characters, else in single quotes.
pub fn quote(s: &str) -> String {
    if s.is_empty() {
        return "''".into();
    }
    let safe = |c: char| c.is_ascii_alphanumeric() || "@%+=:,./-_".contains(c);
    if s.chars().all(safe) {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

/// `shlex.join`.
pub fn join<S: AsRef<str>>(args: &[S]) -> String {
    args.iter()
        .map(|a| quote(a.as_ref()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Start a program detached from LID (own session, no stdio).
pub fn spawn_detached(program: &str, args: &[String]) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let mut c = Command::new(program);
    c.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: setsid is async-signal-safe.
    unsafe {
        c.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    c.spawn().map(|_| ())
}

/// `shlex.split` (POSIX rules); `None` for unbalanced quotes.
pub fn split(s: &str) -> Option<Vec<String>> {
    shlex::split(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_python() {
        assert_eq!(quote("/usr/bin/lider"), "/usr/bin/lider");
        assert_eq!(
            quote("/opt/apps/Lider 1.0.AppImage"),
            "'/opt/apps/Lider 1.0.AppImage'"
        );
        assert_eq!(quote("it's"), "'it'\"'\"'s'");
        assert_eq!(quote(""), "''");
        assert_eq!(join(&["/a b/LID", "--minimized"]), "'/a b/LID' --minimized");
        assert_eq!(
            split("'/a b/LID' --minimized").unwrap(),
            ["/a b/LID", "--minimized"]
        );
    }
}
