# LID on other operating systems: the ports

Everything LID asks of the operating system goes through four interfaces (ports) in
`crates/lider-sys/src/platform/mod.rs`. Each OS has one adapter that implements all of them; the engine,
the manager, the window, the dialogs and the browser helper only use the ports.

```
lider-core (engine, manager, queues, store, view)      ── portable; per-OS file details in fsio.rs and paths.rs
lider-sys
  browser.rs, native_host.rs, sh.rs                    ── portable
  platform/mod.rs      ports: Instance, Desktop, BrowserHosts, Shell; Os = the adapter of this build
  platform/linux/      the Linux adapter (what 1.x and 2.x do)
  platform/unsupported.rs   every other OS for now: compiles, services say "not supported yet"
src-tauri                                              ── uses `lider_sys::platform::prelude::*` only
```

Adding an OS means adding `platform/<os>/` with a struct that implements `Platform` (all four ports),
and pointing `Os` at it under `#[cfg(target_os = "<os>")]`. Nothing else changes.

## What each port needs

| Port | Linux (done) | Windows (to do) | macOS (to do) |
|---|---|---|---|
| `Instance`: one LID per user, lines to the running one | abstract Unix socket `lider-<uid>`, `SO_PEERCRED` | named pipe `\\.\pipe\lider-<user SID>`, owner check on the pipe | Unix socket in `$TMPDIR`, `getpeereid` |
| `Desktop`: menu entry, autostart | `.desktop` files that follow a moved AppImage | Start-menu shortcut, `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (mostly the installer's job) | Login Items (`SMAppService`); the menu is the `.app` in /Applications |
| `BrowserHosts`: where browsers find the native host | manifest files in each browser's NativeMessagingHosts folder, a `/bin/sh` wrapper | manifest file + registry key `HKCU\Software\<vendor>\<browser>\NativeMessagingHosts\io.github.davidaprilio.lid` per browser, an `.exe` (or `.bat`) host | manifest files in `~/Library/Application Support/<browser>/NativeMessagingHosts` |
| `Shell`: open, show in folder, open with, detached start, power off, dark mode, global menu, fonts | xdg-open, FileManager1 (D-Bus), `setsid`, `systemctl poweroff`, settings portal, AppMenu registrar, fontconfig | `ShellExecuteW`, `explorer /select,`, `DETACHED_PROCESS`, `shutdown /s /t 0`, `AppsUseLightTheme`, none, DirectWrite | `open`, `open -R`, `open -a`, `osascript` shut down, `AppleInterfaceStyle`, the app menu bar, Core Text |

Already portable, outside the ports:
- part files written in place: `lider_core::fsio::write_all_at` (`pwrite` / `seek_write`);
- LID's folders without the XDG variables: `lider_core::paths::default_homes` (Windows `AppData`,
  macOS `~/Library`);
- yt-dlp lookup: `fsio::is_executable` (`.exe` on Windows).

Packaging per OS is separate from the ports: AppImage, deb and Arch for Linux (`packaging/build.sh`);
Windows would add the Tauri MSI/NSIS bundle (it needs `src-tauri/icons/icon.ico`), macOS a signed and
notarised `.app`/DMG.

## Checks

- `cargo test --workspace` on Linux runs the adapter's tests (`platform/linux/*`).
- The "Windows" job in `.github/workflows/rust.yml` compiles `lider-core` and `lider-sys` for Windows, so
  the ports and the portable code keep building there while the adapter is written.
