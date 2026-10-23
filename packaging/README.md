# Packaging LID (Linux Internet Downloader)

One script builds every format into `dist/`:

```sh
./packaging/build.sh appimage   # dist/Lider-<ver>-x86_64.AppImage
./packaging/build.sh deb        # dist/lider_<ver>-1_amd64.deb
./packaging/build.sh arch       # dist/lider-<ver>-1-any.pkg.tar.zst
./packaging/build.sh aur        # dist/aur/PKGBUILD + .SRCINFO
./packaging/build.sh extension  # dist/lid-extension-*.zip (+ signed .xpi with AMO keys)
./packaging/build.sh all        # appimage + deb + arch + extension
```

You can set these in the environment:

| Variable | Default | Used for |
|---|---|---|
| `MAINTAINER` | `David Aprilio <david1.aprilio4@gmail.com>` | deb `Maintainer`, PKGBUILD header |
| `PKG_LICENSE` | `MIT` | licence field (must match `LICENSE`) |
| `PROJECT_URL` | `https://github.com/Davidaprilio/lid` | deb `Homepage`, PKGBUILD `url` |
| `AUR_SOURCE_URL` | — | public address of `dist/lider-<ver>.tar.gz` (aur target only) |
| `AMO_JWT_ISSUER`, `AMO_JWT_SECRET` | — | addons.mozilla.org API key; signs the Firefox `.xpi` (extension target only) |

## The three formats

| | AppImage | .deb | Arch / AUR |
|---|---|---|---|
| Runs on | any x86_64 distro with glibc ≥ 2.34 | Debian 12+, Ubuntu 22.04+ (amd64) | Arch, CachyOS, Manjaro, EndeavourOS |
| Python | bundled (3.12) | system (3.10+) | system |
| PySide6 | bundled (6.11.1) | bundled (6.11.1) | from the distribution |
| yt-dlp | bundled (2026.8.19) | bundled (2026.8.19); a `pip install --user` copy wins | `optdepends` |
| ffmpeg, node/deno/bun | from the system | `Recommends: ffmpeg, nodejs \| deno` | `optdepends` |
| Size | ~90 MB | ~65 MB | ~230 KB |

ffmpeg and a JavaScript runtime are only needed for YouTube. Without ffmpeg, LID offers
just the formats that need no merging. Without a JavaScript runtime, YouTube may withhold some
streams.

Ubuntu 24.04 ships no PySide6 at all, so the .deb carries its own PySide6 (abi3 wheels, which
work with any Python 3.10+) in `/usr/lib/lider`, next to a bundled yt-dlp. To use a newer yt-dlp
than the bundled one, run `pip install --user -U yt-dlp`. LID prefers your copy over the bundled
one and over an older distro package.

## Install

```sh
# AppImage
chmod +x Lider-1.0-x86_64.AppImage && ./Lider-1.0-x86_64.AppImage
# (no FUSE? ./Lider-1.0-x86_64.AppImage --appimage-extract-and-run)

# Debian / Ubuntu
sudo apt install ./lider_1.0-1_amd64.deb

# Arch (local package)
sudo pacman -U lider-1.0-1-any.pkg.tar.zst
```

## Releases from GitHub Actions

`.github/workflows/release.yml` builds everything on GitHub:

1. Set the version in `lider/__init__.py`, commit, then tag and push:
   ```sh
   git tag v1.0 && git push origin v1.0
   ```
2. The workflow runs the tests (the tag must match `__version__`) and then builds three things in parallel:
   * the AppImage and the `.deb` on Ubuntu
   * the Arch package on an Arch container
   * the AUR files (`lider-<ver>-aur.tar.gz`, holding `PKGBUILD` and `.SRCINFO`)
   * the browser extension files (see below)
3. It publishes a GitHub release with every file and a `SHA256SUMS` file. The AUR files already
   point at that release's `lider-<ver>.tar.gz`.

To check a release before it is public, create it as a draft first. The workflow then only adds the
files to that draft:

```sh
git tag v1.1.1 && git push origin v1.1.1
gh release create v1.1.1 --draft --verify-tag --title "LID 1.1.1" --generate-notes
```

To build the packages without publishing anything, use "Run workflow" on the Actions tab, or
`gh workflow run release.yml`.

## Publishing to the AUR

1. Build the source tarball and upload it somewhere public, such as a GitHub release:
   `./packaging/build.sh sdist` → `dist/lider-1.0.tar.gz`.
2. Generate the AUR files with that address:
   ```sh
   AUR_SOURCE_URL=https://github.com/Davidaprilio/lid/releases/download/v1.0/lider-1.0.tar.gz \
   ./packaging/build.sh aur
   ```
   `dist/aur/` now holds `PKGBUILD` and `.SRCINFO`. The checksum is that of the tarball from
   step 1, so upload exactly that file.
3. Push them to the AUR (needs an AUR account with your SSH key):
   ```sh
   git clone ssh://aur@aur.archlinux.org/lider.git aur-lider
   cp dist/aur/PKGBUILD dist/aur/.SRCINFO aur-lider/
   cd aur-lider && git add PKGBUILD .SRCINFO && git commit -m "lider 1.0-1" && git push
   ```
   Users then install with `yay -S lider` or `paru -S lider`.

## Browser extension

`./packaging/build.sh extension` needs Python and Node.js (it runs `npx web-ext@10.7.0`). The
extension's version is always `__version__`.

| File | What it is | Where it goes |
|---|---|---|
| `lid-extension-chromium.zip` | full extension with the manifest `key`, so its id is always `hljibhdjbmpphbjhbfgiejnijpeobbne` | GitHub release; users unzip it and *Load unpacked* |
| `lid-extension-chromium-store.zip` | no `key`, no YouTube button (the Chrome Web Store does not accept YouTube downloaders) | upload to the Chrome Web Store / Edge Add-ons |
| `lid-extension-firefox.zip` | full Firefox extension, unsigned | upload to addons.mozilla.org as a listed add-on, or *Load Temporary Add-on* |
| `lid-extension-firefox.xpi` | the same, with `update_url`, signed by Mozilla (unlisted channel) | GitHub release; Firefox installs it with one click |
| `firefox-updates.json` | update manifest the signed `.xpi` checks | GitHub release (`releases/latest/download/firefox-updates.json`) |

The build lints both Firefox variants with `web-ext lint`. The `.xpi` and `firefox-updates.json`
are only produced when `AMO_JWT_ISSUER` and `AMO_JWT_SECRET` are set, because Firefox refuses
unsigned add-ons outside about:debugging.

### Firefox: signed .xpi on every release

1. Sign in at <https://addons.mozilla.org/developers/> and create an API key under
   *Tools → Manage API Keys*. Accept the developer agreement.
2. Store the key as repository secrets:
   ```sh
   gh secret set AMO_JWT_ISSUER   # "JWT issuer", like user:12345:678
   gh secret set AMO_JWT_SECRET   # "JWT secret"
   ```
3. Push a `v*` tag. The `extension` job signs the add-on (unlisted, so it is not reviewed or listed
   on AMO) and the release gets `lid-extension-firefox.xpi` and `firefox-updates.json`. Installed
   copies update themselves from `firefox-updates.json`.
4. Set `FIREFOX_SIGNED_XPI = True` in `lider/browser.py`, so the *Browser integration* dialog links
   the `.xpi` instead of the unsigned zip.

The secrets are only passed to tag builds; "Run workflow" builds stay unsigned. Mozilla never signs
the same version twice, so bump `__version__` before tagging again.

To list LID on addons.mozilla.org as well, upload `lid-extension-firefox.zip` there as a listed
add-on. The code is not minified or bundled, so AMO needs no separate source upload. Then set
`FIREFOX_ADDONS_URL` in `lider/browser.py` so the *Browser integration* dialog links to it.

### Chrome Web Store / Edge Add-ons

1. Register at <https://chrome.google.com/webstore/devconsole> (one-time fee) or
   <https://partner.microsoft.com/dashboard/microsoftedge/> (free).
2. Upload `lid-extension-chromium-store.zip`. Under *Privacy*, explain the permissions:
   `nativeMessaging` (talks to the LID app), `downloads` (hands downloads to LID), `cookies`
   (downloads behind a login), `contextMenus`, `scripting` (*Download all links*), `storage`;
   no data leaves the computer.
3. The store gives the extension its own id. Either:
   * keep the current id: on the **first** Chrome Web Store upload, put the private key in the zip
     root as `key.pem` (`zip -j lid-extension-chromium-store.zip /path/to/key.pem`). Never commit
     that key.
   * or add the store id to `CHROMIUM_EXTENSION_IDS` in `lider/browser.py`, so the LID helper
     accepts it.
4. After the listing is public, set `CHROME_WEB_STORE_URL` in `lider/browser.py`.

## How each build works

* **AppImage:**
  * `uv` downloads a portable CPython (python-build-standalone).
  * pip installs the pinned PySide6-Essentials and yt-dlp into it, and the `lider` package is copied in.
  * Unused parts are removed: the Python test suite, Tk, QML, and the Qt designer tools.
  * `AppRun` starts `python3 -m lider`, and `appimagetool` 1.9.0 packs the AppDir. Both tools are cached in `~/.cache/lider-build`.
* **deb:**
  * The package is assembled with `tar` + `ar` (the standard `.deb` layout), so it builds on any distro without `dpkg-deb`.
  * The app goes to `/usr/lib/lider/lider`, PySide6 to `/usr/lib/lider/vendor` and yt-dlp to `/usr/lib/lider/ytdlp`.
  * `/usr/bin/lider` puts those folders on the import path.
  * `Depends` lists the system libraries Qt needs, including `libxcb-cursor0` for X11.
  * `postinst` byte-compiles the app and `prerm` removes the caches.
* **arch:**
  * `packaging/arch/PKGBUILD.in` is filled in, and `makepkg` builds from the source tarball.
  * The code goes to the system Python's `site-packages`, byte-compiled.

## Licences in the packages

* **All packages:** `LICENSE` (MIT) goes to `/usr/share/licenses/lider/` (Arch, AppImage) or
  `/usr/share/doc/lider/copyright` (deb).
* **AppImage:** it bundles Python, PySide6/Qt (LGPL-3.0), shiboken6 and yt-dlp, so their licence texts
  are copied to `usr/share/licenses/lider/third-party/`. Qt stays as separate shared libraries
  inside the AppImage, which is how the LGPL lets users swap them.
