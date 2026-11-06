//! The file operations that differ between Unix and Windows; the engine and the yt-dlp lookup use these
//! instead of the OS extension traits.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

/// Write all of `buf` at `offset` without moving a shared cursor, so connections can write the same file
/// at once (`pwrite` on Unix, `WriteFile` with an offset on Windows).
pub fn write_all_at(file: &File, buf: &[u8], offset: u64) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.write_all_at(buf, offset)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut done = 0;
        while done < buf.len() {
            match file.seek_write(&buf[done..], offset + done as u64) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => done += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

/// Open options for a part file: `rw-r--r--` on Unix (1.x), the defaults elsewhere.
pub fn part_file_options() -> OpenOptions {
    let mut o = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o644);
    }
    o
}

/// A program that can be started: an executable file on Unix, a `.exe`/`.cmd`/`.bat`/`.com` on Windows.
pub fn is_executable(p: &Path) -> bool {
    let Ok(m) = p.metadata() else { return false };
    if !m.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        m.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
            ["exe", "cmd", "bat", "com"].contains(&e.to_ascii_lowercase().as_str())
        })
    }
}
