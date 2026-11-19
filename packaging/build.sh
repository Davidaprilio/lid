#!/usr/bin/env bash
# Build LID packages into dist/.
#
#   ./packaging/build.sh appimage   AppImage (one program; the system's own OpenGL, X11/Wayland and fonts)
#   ./packaging/build.sh deb        Debian 12+/Ubuntu 22.04+ package
#   ./packaging/build.sh arch       Arch package (.pkg.tar.zst) built with makepkg from the source tarball
#   ./packaging/build.sh aur        PKGBUILD + .SRCINFO for the AUR (needs AUR_SOURCE_URL)
#   ./packaging/build.sh extension  browser extension zips; signed Firefox .xpi when AMO keys are set
#   ./packaging/build.sh windows    per-user Windows installer (on Windows, with Git Bash and NSIS's makensis)
#   ./packaging/build.sh sdist      source tarball (licence list included)
#   ./packaging/build.sh all        appimage, deb, arch and extension
#
# Settings (environment):
#   MAINTAINER       "Name <email>" written into the packages
#   PKG_LICENSE      SPDX licence id for the package metadata
#   PROJECT_URL      project home page
#   AUR_SOURCE_URL   where dist/lider-<version>.tar.gz will be downloadable from (aur target)
#   AMO_JWT_ISSUER   addons.mozilla.org API key: sign the Firefox add-on (extension target)
#   AMO_JWT_SECRET   addons.mozilla.org API secret
#   LIDER_SIGNING_KEY, LIDER_SIGNING_KEY_PASSWORD
#                    the updater's key (or the older names TAURI_SIGNING_PRIVATE_KEY, …_PASSWORD): sign the
#                    AppImage for LID's updater (lider_core::update)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PKG="$ROOT/packaging"
DIST="$ROOT/dist"
WORK="$ROOT/build/packaging"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/lider-build"
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"

# The version lives only in the workspace Cargo.toml ([workspace.package] version).
VERSION="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml")"
MAINTAINER="${MAINTAINER:-David Aprilio <david1.aprilio4@gmail.com>}"
PKG_LICENSE="${PKG_LICENSE:-MIT}"
PROJECT_URL="${PROJECT_URL:-https://github.com/Davidaprilio/lid}"
SUMMARY="Linux Internet Downloader (LID): fast multi-connection downloads"

# Pinned versions (the ones LID was tested with).
WEB_EXT="web-ext@10.7.0"
APPIMAGETOOL_URL="https://github.com/AppImage/appimagetool/releases/download/1.9.0/appimagetool-x86_64.AppImage"

log() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
TAR_FLAGS=(--sort=name --mtime="@0" --owner=0 --group=0 --numeric-owner)

mkdir -p "$DIST" "$WORK"

# ---------------------------------------------------------------- building blocks
# target/release/lider: the program.
binary() {
    [ -n "${BINARY_DONE:-}" ] && return
    log "Building lider $VERSION (cargo, release)"
    (cd "$ROOT" && cargo build --release --locked -p lider-gui)
    BINARY_DONE=1
}

# target/release/lider-pack: extension zips, system host files, licence list (packaging only).
pack() {
    if [ -z "${PACK_DONE:-}" ]; then
        (cd "$ROOT" && cargo build --release --locked -q -p lider-sys --bin lider-pack)
        PACK_DONE=1
    fi
    "$TARGET/release/lider-pack" "$@"
}

# build/packaging/third-party: THIRD-PARTY.md and the licence texts of every crate in lider.
third_party() {
    [ -n "${THIRD_PARTY_DONE:-}" ] && return
    log "Collecting third-party licences"
    rm -rf "$WORK/third-party"
    pack licenses "$WORK/third-party" >/dev/null
    THIRD_PARTY_DONE=1
}

# Make permissions predictable whatever the umask: folders 755, files 644, programs 755.
fix_modes() {
    find "$1" -type d -exec chmod 755 {} + -o -type f ! -perm -u+x -exec chmod 644 {} + -o -type f -perm -u+x -exec chmod 755 {} +
}

# Lay out the program in a filesystem tree: /usr/bin/lider, the browser helper and host manifests,
# the exported extension, the desktop entry, the icon and the README.
install_tree() {
    local root="$1"
    install -Dm755 "$TARGET/release/lider" "$root/usr/bin/lider"
    pack system-files "$root" /usr/lib/lider/lider-native-host /usr/bin/lider
    install -Dm644 "$ROOT/lider.desktop" "$root/usr/share/applications/lider.desktop"
    install -Dm644 "$ROOT/lider/ui/assets/icons/app.svg" "$root/usr/share/icons/hicolor/scalable/apps/lider.svg"
    install -Dm644 "$ROOT/README.md" "$root/usr/share/doc/lider/README.md"
    fix_modes "$root"
}

# ---------------------------------------------------------------- source tarball
# What cargo needs to build lider offline once its crates are fetched, plus the licence list.
sdist() {
    local name="lider-$VERSION" out="$DIST/lider-$VERSION.tar.gz" dir
    third_party >&2
    dir="$WORK/sdist/$name"
    rm -rf "$WORK/sdist"
    mkdir -p "$dir"
    (cd "$ROOT" && git ls-files -z --cached --others --exclude-standard -- Cargo.toml Cargo.lock crates lider/browser_extension \
        lider/ui/assets/icons README.md LICENSE lider.desktop | tar -c --null -T - -f -) | tar -x -C "$dir"
    cp -r "$WORK/third-party" "$dir/third-party"
    fix_modes "$dir"
    tar "${TAR_FLAGS[@]}" -czf "$out" -C "$WORK/sdist" "$name"
    echo "$out"
}

# ---------------------------------------------------------------- .deb
# Built on the oldest supported release (Ubuntu 22.04 in CI), so it runs on Debian 12+ and Ubuntu 22.04+.
# The window opens these at run time (OpenGL through EGL or GLX, X11 or Wayland, the keyboard maps).
DEB_DEPENDS="libegl1, libgl1, libx11-6, libxcursor1, libxi6, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libwayland-cursor0, libwayland-egl1"

build_deb() {
    local root="$WORK/deb/root" out="$DIST/lider_${VERSION}-1_amd64.deb" glibc
    binary
    third_party
    log "Building $out"
    rm -rf "$WORK/deb"
    install_tree "$root"
    cp -r "$WORK/third-party" "$root/usr/share/doc/lider/third-party"
    fix_modes "$root"
    # The newest glibc symbol the program uses decides the libc6 it needs.
    glibc="$(objdump -T "$root/usr/bin/lider" | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -Vu | tail -1)"

    { printf 'Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\nUpstream-Name: lider\n'
      printf 'Source: %s\n\n' "$PROJECT_URL"
      printf 'Files: *\nCopyright: 2026 %s\nLicense: %s\n' "$MAINTAINER" "$PKG_LICENSE"
      sed 's/^$/./; s/^/ /' "$ROOT/LICENSE"
      printf '\nFiles: usr/bin/lider\nComment: built from the Rust crates listed in\n'
      printf ' /usr/share/doc/lider/third-party/THIRD-PARTY.md, with their licence texts next to it.\n'
    } > "$root/usr/share/doc/lider/copyright"
    install -d "$root/DEBIAN"
    local size
    size="$(du -sk --exclude=DEBIAN "$root" | cut -f1)"
    sed -e "s|@VERSION@|${VERSION/-/\~}|" -e "s|@MAINTAINER@|$MAINTAINER|" -e "s|@SIZE@|$size|" \
        -e "s|@SUMMARY@|$SUMMARY|" -e "s|@URL@|$PROJECT_URL|" \
        -e "s#@DEPENDS@#libc6 (>= $glibc), $DEB_DEPENDS#" "$PKG/deb/control.in" > "$root/DEBIAN/control"
    # dpkg sorts 2.0.0~alpha.1 before 2.0.0; the file name keeps the plain version.
    (cd "$root" && find usr -type f -print0 | sort -z | xargs -0 md5sum > DEBIAN/md5sums)
    # Files under /etc (browser host manifests) are conffiles: dpkg keeps local edits on upgrade.
    (cd "$root" && find etc -type f | sort | sed 's|^|/|' > DEBIAN/conffiles)
    chmod 644 "$root/DEBIAN/"*

    local tmp="$WORK/deb/ar"
    mkdir -p "$tmp"
    echo "2.0" > "$tmp/debian-binary"
    tar "${TAR_FLAGS[@]}" -cJf "$tmp/control.tar.xz" -C "$root/DEBIAN" .
    tar "${TAR_FLAGS[@]}" -cJf "$tmp/data.tar.xz" -C "$root" ./usr ./etc
    rm -f "$out"
    (cd "$tmp" && ar rcD "$out" debian-binary control.tar.xz data.tar.xz)
    log "Done: $out ($(du -h "$out" | cut -f1))"
}

# ---------------------------------------------------------------- Arch / AUR
# pacman versions have no "-": 2.0.0-alpha.1 becomes 2.0.0alpha.1, which vercmp sorts before 2.0.0.
arch_version() { echo "${VERSION//-/}"; }

write_pkgbuild() {
    local dest="$1" source="$2" sha="$3"
    sed -e "s|@VERSION@|$(arch_version)|" -e "s|@SRCVERSION@|$VERSION|" -e "s|@MAINTAINER@|$MAINTAINER|" \
        -e "s|@LICENSE@|$PKG_LICENSE|" -e "s|@SUMMARY@|$SUMMARY|" -e "s|@URL@|$PROJECT_URL|" \
        -e "s|@SOURCE@|$source|" -e "s|@SHA256@|$sha|" "$PKG/arch/PKGBUILD.in" > "$dest/PKGBUILD"
}

build_arch() {
    local tarball sha dir="$WORK/arch"
    tarball="$(sdist)"
    sha="$(sha256sum "$tarball" | cut -d' ' -f1)"
    log "Building the Arch package with makepkg"
    rm -rf "$dir"
    mkdir -p "$dir"
    cp "$tarball" "$dir/"
    write_pkgbuild "$dir" "lider-$VERSION.tar.gz" "$sha"
    (cd "$dir" && PKGDEST="$DIST" makepkg -f --nodeps --noconfirm)
    log "Done: $(ls "$DIST"/lider-"$(arch_version)"-*-x86_64.pkg.tar.* | head -1)"
}

build_aur() {
    if [ -z "${AUR_SOURCE_URL:-}" ]; then
        echo "Set AUR_SOURCE_URL to the public download address of dist/lider-$VERSION.tar.gz, e.g." >&2
        echo "  AUR_SOURCE_URL=https://github.com/Davidaprilio/lid/releases/download/v$VERSION/lider-$VERSION.tar.gz \\" >&2
        echo "    ./packaging/build.sh aur" >&2
        exit 2
    fi
    local tarball sha dir="$DIST/aur"
    tarball="$(sdist)"
    sha="$(sha256sum "$tarball" | cut -d' ' -f1)"
    rm -rf "$dir"
    mkdir -p "$dir"
    write_pkgbuild "$dir" "lider-$VERSION.tar.gz::$AUR_SOURCE_URL" "$sha"
    (cd "$dir" && makepkg --printsrcinfo > .SRCINFO)
    log "AUR files in $dir (upload $tarball to $AUR_SOURCE_URL first)"
}

# ---------------------------------------------------------------- browser extension
build_extension() {
    local work="$WORK/extension"
    log "Building the browser extension (Chromium, Chrome Web Store, Firefox)"
    rm -rf "$work"
    mkdir -p "$work"
    rm -f "$DIST"/lid-extension-* "$DIST/firefox-updates.json"
    pack extensions "$DIST" "$work"
    log "Checking the Firefox add-on with web-ext lint"
    npx --yes "$WEB_EXT" lint --source-dir "$work/firefox" --self-hosted --output text 2>&1 | tail -15
    if [ -n "${AMO_JWT_ISSUER:-}" ] && [ -n "${AMO_JWT_SECRET:-}" ]; then
        log "Signing the Firefox add-on on addons.mozilla.org (unlisted, self-distributed)"
        npx --yes "$WEB_EXT" sign --source-dir "$work/firefox-unlisted" --artifacts-dir "$work/signed" \
            --channel unlisted --api-key "$AMO_JWT_ISSUER" --api-secret "$AMO_JWT_SECRET"
        cp "$(ls "$work"/signed/*.xpi | head -1)" "$DIST/lid-extension-firefox.xpi"
    else
        log "AMO_JWT_ISSUER/AMO_JWT_SECRET not set: no signed .xpi"
        rm -f "$DIST/firefox-updates.json"  # only useful next to a signed .xpi
    fi
    log "Done: $(cd "$DIST" && ls lid-extension-* firefox-updates.json 2>/dev/null | tr '\n' ' ')"
}

# ---------------------------------------------------------------- AppImage
# One program: egui draws the window itself, so the AppDir carries no toolkit. OpenGL, X11/Wayland, the
# keyboard maps and the fonts are the system's own (every desktop has them). yt-dlp is not bundled: LID
# offers to download it when YouTube first needs it.
build_appimage() {
    local out="$DIST/Lider-$VERSION-x86_64.AppImage" appdir="$WORK/appimage/Lider.AppDir"
    binary
    third_party
    log "Laying out the AppDir"
    rm -rf "$WORK/appimage"
    install -Dm755 "$TARGET/release/lider" "$appdir/usr/bin/lider"
    ln -s usr/bin/lider "$appdir/AppRun"
    install -Dm644 "$ROOT/lider.desktop" "$appdir/usr/share/applications/lider.desktop"
    cp "$ROOT/lider.desktop" "$appdir/lider.desktop"
    install -Dm644 "$ROOT/lider/ui/assets/icons/app.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/lider.svg"
    cp "$ROOT/lider/ui/assets/icons/app.svg" "$appdir/lider.svg"
    ln -s lider.svg "$appdir/.DirIcon"
    local licenses="$appdir/usr/share/licenses/lider"
    install -Dm644 "$ROOT/LICENSE" "$licenses/LICENSE"
    cp -r "$WORK/third-party" "$licenses/third-party"
    install -Dm644 "$ROOT/README.md" "$appdir/usr/share/doc/lider/README.md"
    fix_modes "$appdir"

    local tool="$CACHE/appimagetool-1.9.0-x86_64.AppImage"
    if [ ! -x "$tool" ]; then
        log "Downloading appimagetool 1.9.0"
        mkdir -p "$CACHE"
        curl -fsSL -o "$tool.tmp" "$APPIMAGETOOL_URL"
        chmod +x "$tool.tmp"
        mv "$tool.tmp" "$tool"
    fi
    rm -f "$out" "$out.sig"
    ARCH=x86_64 "$tool" --appimage-extract-and-run --no-appstream "$appdir" "$out" >/dev/null
    sign "$out"
    log "Done: $out ($(du -h "$out" | cut -f1))"
}

# FILE.sig for LID's updater, when the release key is in the environment.
sign() {
    if [ -n "${LIDER_SIGNING_KEY:-}${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
        log "Signing $(basename "$1") for the updater"
        pack sign "$1" >/dev/null
    fi
}

# ---------------------------------------------------------------- Windows
# packaging/windows/lider.nsi: the per-user installer, also an update over the Tauri-built 2.0 betas.
winpath() { if command -v cygpath >/dev/null; then cygpath -w "$1"; else echo "$1"; fi; }

build_windows() {
    local out="$DIST/LID-$VERSION-x64-setup.exe"
    command -v makensis >/dev/null || { echo "makensis (NSIS) is needed" >&2; exit 1; }
    binary
    third_party
    log "Building $out (makensis)"
    rm -f "$out" "$out.sig"
    makensis -V2 "-DVERSION=$VERSION" "-DVERSION_NUM=${VERSION%%-*}.0" \
        "-DEXE=$(winpath "$TARGET/release/lider.exe")" "-DROOT=$(winpath "$ROOT")" \
        "-DTHIRD_PARTY=$(winpath "$WORK/third-party")" "-DOUTFILE=$(winpath "$out")" \
        "$(winpath "$PKG/windows/lider.nsi")"
    sign "$out"
    log "Done: $out ($(du -h "$out" | cut -f1))"
}

case "${1:-}" in
    appimage) build_appimage ;;
    deb) build_deb ;;
    windows) build_windows ;;
    arch) build_arch ;;
    aur) build_aur ;;
    extension) build_extension ;;
    sdist) sdist ;;
    all) build_appimage; build_deb; build_arch; build_extension ;;
    *) sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'; exit 1 ;;
esac
