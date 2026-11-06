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
  platform/windows/    the Windows adapter (named pipe, registry, Explorer)
  platform/unsupported.rs   every other OS for now (macOS): compiles, services say "not supported yet"
src-tauri                                              ── uses `lider_sys::platform::prelude::*` only
```

Adding an OS means adding `platform/<os>/` with a struct that implements `Platform` (all four ports),
and pointing `Os` at it under `#[cfg(target_os = "<os>")]`. Nothing else changes.

## What each port needs

| Port | Linux (done) | Windows (done, needs a try on a real machine) | macOS (to do) |
|---|---|---|---|
| `Instance`: one LID per user, lines to the running one | abstract Unix socket `lider-<uid>`, `SO_PEERCRED` | named pipe `\\.\pipe\lider-<USERNAME>` (tokio; first instance only, local clients only; the default pipe security lets only this user write) | Unix socket in `$TMPDIR`, `getpeereid` |
| `Desktop`: menu entry, autostart | `.desktop` files that follow a moved AppImage | the per-user installer adds the Start-menu entry; autostart is the `LID` value in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` | Login Items (`SMAppService`); the menu is the `.app` in /Applications |
| `BrowserHosts`: where browsers find the native host | manifest files in each browser's NativeMessagingHosts folder, a `/bin/sh` wrapper | a manifest per family in `%LOCALAPPDATA%\lider\native-host` + registry key `HKCU\Software\<vendor>\<browser>\NativeMessagingHosts\io.github.davidaprilio.lid` (Chrome, Chromium, Brave, Edge, Firefox); the manifest names `lider.exe`, whose browser-given arguments select host mode | manifest files in `~/Library/Application Support/<browser>/NativeMessagingHosts` |
| `Shell`: open, show in folder, open with, detached start, power off, dark mode, global menu, fonts | xdg-open, FileManager1 (D-Bus), `setsid`, `systemctl poweroff`, settings portal, AppMenu registrar, fontconfig | `explorer.exe <path>`, `explorer /select,`, `DETACHED_PROCESS`, `shutdown /s /t 0`, `AppsUseLightTheme`, none, the registry font list (`Consolas` for mono) | `open`, `open -R`, `open -a`, `osascript` shut down, `AppleInterfaceStyle`, the app menu bar, Core Text |

Already portable, outside the ports:
- part files written in place: `lider_core::fsio::write_all_at` (`pwrite` / `seek_write`);
- LID's folders without the XDG variables: `lider_core::paths::default_homes` (Windows `AppData`,
  macOS `~/Library`);
- yt-dlp lookup: `fsio::is_executable` (`.exe` on Windows).

Packaging per OS is separate from the ports: AppImage, deb and Arch for Linux (`packaging/build.sh`);
Windows has the Tauri NSIS bundle (per-user, `src-tauri/icons/icon.ico`), macOS would need a signed and
notarised `.app`/DMG.

## Checks

- `cargo test --workspace` on Linux runs the adapter's tests (`platform/linux/*`).
- The `windows` job in `.github/workflows/rust.yml` runs the core and adapter tests on Windows (named
  pipe, registry hosts, autostart), clippy, and builds the per-user NSIS installer as an artifact
  (`LID_<version>_x64-setup.exe`, about 5 MB). Not yet: yt-dlp.exe in the installer, signing, and the
  installer in the release workflow.
