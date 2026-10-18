# Packaging LID (Linux Internet Downloader)

One script builds every format into `dist/`:

```sh
./packaging/build.sh appimage   # dist/Lider-<ver>-x86_64.AppImage
./packaging/build.sh deb        # dist/lider_<ver>-1_amd64.deb
./packaging/build.sh arch       # dist/lider-<ver>-1-any.pkg.tar.zst
./packaging/build.sh aur        # dist/aur/PKGBUILD + .SRCINFO
./packaging/build.sh all        # appimage + deb + arch
```

You can set these in the environment:

| Variable | Default | Used for |
|---|---|---|
| `MAINTAINER` | `David Aprilio <david1.aprilio4@gmail.com>` | deb `Maintainer`, PKGBUILD header |
| `PKG_LICENSE` | `MIT` | licence field (must match `LICENSE`) |
| `PROJECT_URL` | `https://github.com/Davidaprilio/lid` | deb `Homepage`, PKGBUILD `url` |
| `AUR_SOURCE_URL` | — | public address of `dist/lider-<ver>.tar.gz` (aur target only) |

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
3. It publishes a GitHub release with every file and a `SHA256SUMS` file. The AUR files already
   point at that release's `lider-<ver>.tar.gz`.

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
