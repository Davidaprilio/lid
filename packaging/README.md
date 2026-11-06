# Packaging LID (Linux Internet Downloader)

One script builds every format into `dist/`:

```sh
./packaging/build.sh appimage   # dist/Lider-<ver>-x86_64.AppImage
./packaging/build.sh deb        # dist/lider_<ver>-1_amd64.deb
./packaging/build.sh arch       # dist/lider-<ver>-1-x86_64.pkg.tar.zst
./packaging/build.sh aur        # dist/aur/PKGBUILD + .SRCINFO
./packaging/build.sh extension  # dist/lid-extension-*.zip (+ signed .xpi with AMO keys)
./packaging/build.sh sdist      # dist/lider-<ver>.tar.gz
./packaging/build.sh all        # appimage + deb + arch + extension
```

It needs Rust (stable, 1.90 or newer), pnpm and Node.js, and the development files of WebKitGTK 4.1,
GTK 3 and libayatana-appindicator (`libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev` on
Debian/Ubuntu, `webkit2gtk-4.1 libayatana-appindicator` on Arch). The AppImage also needs the Tauri CLI
(`cargo install tauri-cli --version "~2.12" --locked`), and `arch`/`aur` need `makepkg`. If another
`pkg-config` shadows the system one (Linuxbrew), run the script with `PKG_CONFIG=/usr/bin/pkg-config`.

You can set these in the environment:

| Variable | Default | Used for |
|---|---|---|
| `MAINTAINER` | `David Aprilio <david1.aprilio4@gmail.com>` | deb `Maintainer`, PKGBUILD header |
| `PKG_LICENSE` | `MIT` | licence field (must match `LICENSE`) |
| `PROJECT_URL` | `https://github.com/Davidaprilio/lid` | deb `Homepage`, PKGBUILD `url` |
| `AUR_SOURCE_URL` | — | public address of `dist/lider-<ver>.tar.gz` (aur target only) |
| `AMO_JWT_ISSUER`, `AMO_JWT_SECRET` | — | addons.mozilla.org API key; signs the Firefox `.xpi` (extension target only) |

The version is set only in the workspace `Cargo.toml` (`[workspace.package] version`). Package formats
spell a pre-release their own way: `2.0.0-alpha.1` is `2.0.0~alpha.1-1` in the deb and `2.0.0alpha.1-1` in
the Arch package, so both sort it before `2.0.0`. File names keep the plain version.

## The three formats

| | AppImage | .deb | Arch / AUR |
|---|---|---|---|
| Runs on | any x86_64 distro with glibc ≥ the build host's (Ubuntu 22.04 in CI) | Debian 12+, Ubuntu 22.04+ (amd64) | Arch, CachyOS, Manjaro, EndeavourOS |
| WebKitGTK, GTK | bundled | from the system (`Depends`) | from the system (`depends`) |
| yt-dlp | bundled (2026.08.19) | bundled (2026.08.19) | `optdepends` |
| ffmpeg, node/deno/bun | from the system | `Recommends: ffmpeg, nodejs \| deno` | `optdepends` |
| Application menu | offered on first start, written to `~/.local/share/applications` | installed | installed |
| Size | ~150 MB | ~45 MB | ~8 MB |

ffmpeg and a JavaScript runtime are only needed for YouTube. Without ffmpeg, LID offers just the formats
that need no merging. Without a JavaScript runtime, YouTube may withhold some streams.

LID looks for yt-dlp in this order: `~/.local/bin/yt-dlp` (your own copy, so `pip install --user -U yt-dlp`
or a downloaded `yt-dlp` updates it without waiting for LID), the copy the package ships in
`/usr/lib/lider/yt-dlp` (the AppImage keeps it at the same place inside), `yt-dlp` on PATH, then
`python3 -m yt_dlp`.

## Install

```sh
# AppImage
chmod +x Lider-2.0.0-x86_64.AppImage && ./Lider-2.0.0-x86_64.AppImage
# (no FUSE? ./Lider-2.0.0-x86_64.AppImage --appimage-extract-and-run)

# Debian / Ubuntu
sudo apt install ./lider_2.0.0-1_amd64.deb

# Arch (local package)
sudo pacman -U lider-2.0.0-1-x86_64.pkg.tar.zst
```

Settings, the download list and the browser setup of LID 1.x are used as they are: 2.x reads and writes
the same files in `~/.config/lider` and `~/.local/share/lider`.

## Releases from GitHub Actions

`.github/workflows/release.yml` builds everything on GitHub:

1. Set the version in `Cargo.toml` (`[workspace.package]`), commit, then tag and push:
   ```sh
   git tag v2.0.0 && git push origin v2.0.0
   ```
2. The workflow runs the tests (the tag must match the version) and then builds in parallel:
   * the AppImage and the `.deb` on Ubuntu 22.04, the oldest supported release
   * the Arch package and the AUR files (`lider-<ver>-aur.tar.gz`, holding `PKGBUILD` and `.SRCINFO`) in
     an Arch container
   * the browser extension files (see below)
3. It publishes a GitHub release with every file and a `SHA256SUMS` file. The AUR files already point at
   that release's `lider-<ver>.tar.gz`. A version with a `-` (`v2.0.0-alpha.1`) becomes a pre-release, so
   `releases/latest`, which the extension and 1.x link to, stays on the last stable release.

To check a release before it is public, create it as a draft first. The workflow then only adds the files
to that draft:

```sh
git tag v2.0.0 && git push origin v2.0.0
gh release create v2.0.0 --draft --verify-tag --title "LID 2.0.0" --generate-notes
```

If a tag has to move to a newer commit, its release is kept and only its files are replaced:
`git tag -f -a v2.0.0 -m "LID 2.0.0" && git push -f origin v2.0.0`.

To build the packages without publishing anything, use "Run workflow" on the Actions tab, or
`gh workflow run release.yml`.

## Publishing to the AUR

1. Build the source tarball and upload it somewhere public, such as a GitHub release:
   `./packaging/build.sh sdist` → `dist/lider-2.0.0.tar.gz`. The tarball is reproducible: the same commit
   gives the same bytes.
2. Generate the AUR files with that address:
   ```sh
   AUR_SOURCE_URL=https://github.com/Davidaprilio/lid/releases/download/v2.0.0/lider-2.0.0.tar.gz \
   ./packaging/build.sh aur
   ```
   `dist/aur/` now holds `PKGBUILD` and `.SRCINFO`. The checksum is that of the tarball from step 1, so
   upload exactly that file (the release workflow does both).
3. Push them to the AUR (needs an AUR account with your SSH key). The checkout lives next to this
   repository in `../aur-lider`:
   ```sh
   git clone ssh://aur@aur.archlinux.org/lider.git ../aur-lider   # once
   cp dist/aur/PKGBUILD dist/aur/.SRCINFO ../aur-lider/
   cd ../aur-lider && git add PKGBUILD .SRCINFO && git commit -m "lider 2.0.0-1" && git push
   ```
   Users then install with `yay -S lider` or `paru -S lider`.

The PKGBUILD builds with cargo only: the tarball carries the built frontend (`ui/dist`) and the licence
list (`third-party/`), so neither Node.js nor the npm packages are needed. `prepare()` fetches the crates
with `cargo fetch --locked`; `build()` runs offline (`--frozen`).

## Browser extension

`./packaging/build.sh extension` needs Rust and Node.js (it runs `npx web-ext@10.7.0`). The extension has
its own version, `EXTENSION_VERSION` in `crates/lider-sys/src/browser.rs` (1.0.0 for the first store
release), apart from LID's version. Raise it whenever the extension changes. The extension source is in
`lider/browser_extension/` and is compiled into `lider`.

| File | What it is | Where it goes |
|---|---|---|
| `lid-extension-chromium.zip` | full extension with the manifest `key`, so its id is always `hljibhdjbmpphbjhbfgiejnijpeobbne` | GitHub release; users unzip it and *Load unpacked* |
| `lid-extension-chromium-store.zip` | no `key`, no YouTube button (the Chrome Web Store does not accept YouTube downloaders) | upload to the Chrome Web Store / Edge Add-ons |
| `lid-extension-firefox.zip` | full Firefox extension, unsigned | upload to addons.mozilla.org as a listed add-on, or *Load Temporary Add-on* |
| `lid-extension-firefox.xpi` | the same, with `update_url`, signed by Mozilla (unlisted channel) | GitHub release; Firefox installs it with one click |
| `firefox-updates.json` | update manifest the signed `.xpi` checks | GitHub release (`releases/latest/download/firefox-updates.json`) |

The build lints the Firefox add-on with `web-ext lint`. The `.xpi` and `firefox-updates.json` are only
produced when `AMO_JWT_ISSUER` and `AMO_JWT_SECRET` are set, because Firefox refuses unsigned add-ons
outside about:debugging.

### Firefox: signed .xpi on every release

1. Sign in at <https://addons.mozilla.org/developers/> and create an API key under
   *Tools → Manage API Keys*. Accept the developer agreement.
2. Store the key as repository secrets:
   ```sh
   gh secret set AMO_JWT_ISSUER   # "JWT issuer", like user:12345:678
   gh secret set AMO_JWT_SECRET   # "JWT secret"
   ```
3. Push a `v*` tag. The `extension` job signs the add-on (unlisted, so it is not reviewed or listed on
   AMO) and the release gets `lid-extension-firefox.xpi` and `firefox-updates.json`. Installed copies
   update themselves from `firefox-updates.json`.
4. Set `FIREFOX_SIGNED_XPI = true` in `crates/lider-sys/src/browser.rs`, so the *Browser integration*
   window links the `.xpi` instead of the unsigned zip.

The secrets are only passed to tag builds; "Run workflow" builds stay unsigned. Mozilla never signs the
same version twice, so with these keys set, raise `EXTENSION_VERSION` before every tag.

To list LID on addons.mozilla.org as well, upload `lid-extension-firefox.zip` there as a listed add-on.
The code is not minified or bundled, so AMO needs no separate source upload. Then set
`FIREFOX_ADDONS_URL` in `crates/lider-sys/src/browser.rs` so the *Browser integration* window links to it.

### Chrome Web Store / Edge Add-ons

For Edge Add-ons everything Partner Center asks for is ready in
[`store/edge/listing.md`](store/edge/listing.md): texts to paste, permission justifications,
certification notes, the [privacy policy](store/edge/privacy.md) and the images in
[`store/edge/assets/`](store/edge/assets). Start with its step 0: the store id has to be in a LID release
before the extension is submitted.

1. Register at <https://chrome.google.com/webstore/devconsole> (one-time fee) or
   <https://partner.microsoft.com/dashboard/microsoftedge/> (free).
2. Upload `lid-extension-chromium-store.zip`. Under *Privacy*, explain the permissions: `nativeMessaging`
   (talks to the LID app), `downloads` (hands downloads to LID), `cookies` (downloads behind a login),
   `contextMenus`, `scripting` (*Download all links*), `storage`; no data leaves the computer.
3. The store gives the extension its own id. Either:
   * keep the current id: on the **first** Chrome Web Store upload, put the private key in the zip root as
     `key.pem` (`zip -j lid-extension-chromium-store.zip /path/to/key.pem`). Never commit that key.
   * or add the store id to `CHROMIUM_EXTENSION_IDS` in `crates/lider-sys/src/browser.rs`, so the LID
     helper accepts it.
4. After the listing is public, set `CHROME_WEB_STORE_URL` in `crates/lider-sys/src/browser.rs`.

## How each build works

* **The program:** `cargo build --release --locked -p lider --features tauri/custom-protocol` after
  `pnpm build` in `ui/`. The frontend, the extension and the icons are compiled into the one `lider`
  binary, which is also the browser helper (`lider --native-host`).
* **`lider-pack`** (`crates/lider-sys/src/bin/lider-pack.rs`, a build tool that is never installed) writes
  the extension zips, the system host files (`/usr/lib/lider/lider-native-host`, the manifests under
  `/etc` and `/usr/lib/mozilla`, the exported extension in `/usr/share/lider/browser-extension`) and the
  licence list.
* **AppImage:**
  * `cargo tauri build --bundles appimage` lays out the AppDir: WebKitGTK with its helper processes, GTK
    and its modules, through linuxdeploy and its GTK plugin (its `AppRun` sets up GTK and starts `lider`).
  * The script then adds yt-dlp, LID's own desktop entry and scalable icon, and the licences, and packs
    the AppDir with `appimagetool` 1.9.0 (cached in `~/.cache/lider-build`, like yt-dlp).
* **deb:**
  * The package is assembled with `tar` + `ar` (the standard `.deb` layout), so it builds on any distro
    without `dpkg-deb`, and with reproducible tar flags.
  * `Depends` names WebKitGTK 4.1, GTK 3 and an appindicator library; the `libc6` version is the newest
    glibc symbol the binary uses, so build it on the oldest release it should run on.
  * The browser host manifests under `/etc` are conffiles. No maintainer scripts are needed.
* **arch:** `packaging/arch/PKGBUILD.in` is filled in, and `makepkg` builds from the source tarball, as
  the AUR does.

## Licences in the packages

* **All packages:** `LICENSE` (MIT) goes to `/usr/share/licenses/lider/` (Arch, AppImage) or
  `/usr/share/doc/lider/copyright` (deb, DEP-5).
* **Third-party code in `lider`:** the Rust crates and npm packages it is built from, with their licence
  texts and a `THIRD-PARTY.md` table, go to `third-party/` next to the LICENSE (`/usr/share/doc/lider/`
  in the deb).
* **AppImage:** it also carries WebKitGTK, GTK and their libraries as separate shared libraries, which is
  how the LGPL lets users swap them. `usr/share/licenses/lider/system-libraries/` lists each library with
  the licence files of the distribution package it came from.
* **yt-dlp** (deb, AppImage) is public domain (Unlicense).
