#!/usr/bin/env bash
# Build LID packages into dist/.
#
#   ./packaging/build.sh appimage   self-contained AppImage (WebKitGTK and yt-dlp bundled)
#   ./packaging/build.sh deb        Debian 12+/Ubuntu 22.04+ package (system WebKitGTK, bundled yt-dlp)
#   ./packaging/build.sh arch       Arch package (.pkg.tar.zst) built with makepkg from the source tarball
#   ./packaging/build.sh aur        PKGBUILD + .SRCINFO for the AUR (needs AUR_SOURCE_URL)
#   ./packaging/build.sh extension  browser extension zips; signed Firefox .xpi when AMO keys are set
#   ./packaging/build.sh sdist      source tarball (built frontend and licence list included)
#   ./packaging/build.sh all        appimage, deb, arch and extension
#
# Settings (environment):
#   MAINTAINER       "Name <email>" written into the packages
#   PKG_LICENSE      SPDX licence id for the package metadata
#   PROJECT_URL      project home page
#   AUR_SOURCE_URL   where dist/lider-<version>.tar.gz will be downloadable from (aur target)
#   AMO_JWT_ISSUER   addons.mozilla.org API key: sign the Firefox add-on (extension target)
#   AMO_JWT_SECRET   addons.mozilla.org API secret
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
YTDLP_VERSION="2026.08.19"
YTDLP_SHA256="58162f9bfdc27458ea47bfcb311cf47028f17d8154a8bf7d689861d46399230a"  # yt-dlp_linux
WEB_EXT="web-ext@10.7.0"
APPIMAGETOOL_URL="https://github.com/AppImage/appimagetool/releases/download/1.9.0/appimagetool-x86_64.AppImage"

log() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
TAR_FLAGS=(--sort=name --mtime="@0" --owner=0 --group=0 --numeric-owner)

mkdir -p "$DIST" "$WORK"

# ---------------------------------------------------------------- building blocks
# The frontend (ui/dist), built once per run.
frontend() {
    [ -n "${FRONTEND_DONE:-}" ] && return
    log "Building the frontend (pnpm)"
    pnpm --dir "$ROOT/ui" install --frozen-lockfile >/dev/null
    pnpm --dir "$ROOT/ui" build >/dev/null
    FRONTEND_DONE=1
}

# target/release/lider: the production program (the frontend is embedded).
binary() {
    [ -n "${BINARY_DONE:-}" ] && return
    frontend
    log "Building lider $VERSION (cargo, release)"
    (cd "$ROOT" && cargo build --release --locked -p lider --features tauri/custom-protocol)
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

# build/packaging/third-party: THIRD-PARTY.md and the licence texts of every crate and npm package in lider.
third_party() {
    [ -n "${THIRD_PARTY_DONE:-}" ] && return
    log "Collecting third-party licences"
    pnpm --dir "$ROOT/ui" install --frozen-lockfile >/dev/null
    rm -rf "$WORK/third-party"
    pack licenses "$WORK/third-party" >/dev/null
    THIRD_PARTY_DONE=1
}

# The pinned standalone yt-dlp, downloaded once and checked.
ytdlp() {
    local f="$CACHE/yt-dlp_linux-$YTDLP_VERSION"
    mkdir -p "$CACHE"
    if [ ! -x "$f" ]; then
        log "Downloading yt-dlp $YTDLP_VERSION" >&2
        curl -fsSL -o "$f.tmp" "https://github.com/yt-dlp/yt-dlp/releases/download/$YTDLP_VERSION/yt-dlp_linux"
        echo "$YTDLP_SHA256  $f.tmp" | sha256sum -c --quiet
        chmod 755 "$f.tmp"
        mv "$f.tmp" "$f"
    fi
    echo "$f"
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
# What cargo needs to build lider offline once its crates are fetched, plus the built frontend and the
# licence list (so the Arch build needs neither Node.js nor the npm packages).
sdist() {
    local name="lider-$VERSION" out="$DIST/lider-$VERSION.tar.gz" dir
    frontend >&2
    third_party >&2
    dir="$WORK/sdist/$name"
    rm -rf "$WORK/sdist"
    mkdir -p "$dir"
    (cd "$ROOT" && git ls-files -z --cached --others --exclude-standard -- Cargo.toml Cargo.lock crates src-tauri ui lider/browser_extension \
        lider/ui/assets/icons README.md LICENSE lider.desktop | tar -c --null -T - -f -) | tar -x -C "$dir"
    cp -r "$ROOT/ui/dist" "$dir/ui/dist"
    cp -r "$WORK/third-party" "$dir/third-party"
    fix_modes "$dir"
    tar "${TAR_FLAGS[@]}" -czf "$out" -C "$WORK/sdist" "$name"
    echo "$out"
}

# ---------------------------------------------------------------- .deb
# Built on the oldest supported release (Ubuntu 22.04 in CI), so it runs on Debian 12+ and Ubuntu 22.04+.
DEB_DEPENDS="libwebkit2gtk-4.1-0, libgtk-3-0, libayatana-appindicator3-1 | libappindicator3-1"

build_deb() {
    local root="$WORK/deb/root" out="$DIST/lider_${VERSION}-1_amd64.deb" glibc
    binary
    third_party
    log "Building $out"
    rm -rf "$WORK/deb"
    install_tree "$root"
    install -Dm755 "$(ytdlp)" "$root/usr/lib/lider/yt-dlp"
    cp -r "$WORK/third-party" "$root/usr/share/doc/lider/third-party"
    fix_modes "$root"
    # The newest glibc symbol the program uses decides the libc6 it needs.
    glibc="$(objdump -T "$root/usr/bin/lider" | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -Vu | tail -1)"

    { printf 'Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\nUpstream-Name: lider\n'
      printf 'Source: %s\n\n' "$PROJECT_URL"
      printf 'Files: *\nCopyright: 2026 %s\nLicense: %s\n' "$MAINTAINER" "$PKG_LICENSE"
      sed 's/^$/./; s/^/ /' "$ROOT/LICENSE"
      printf '\nFiles: usr/bin/lider\nComment: built from the Rust crates and npm packages listed in\n'
      printf ' /usr/share/doc/lider/third-party/THIRD-PARTY.md, with their licence texts next to it.\n'
      printf '\nFiles: usr/lib/lider/yt-dlp\nCopyright: yt-dlp contributors\nLicense: Unlicense\n'
      printf 'Comment: the standalone yt-dlp %s from https://github.com/yt-dlp/yt-dlp\n' "$YTDLP_VERSION"
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
# The Tauri bundler lays out the AppDir with WebKitGTK and its helper processes (linuxdeploy and its GTK
# plugin); LID then adds what 1.x had in its AppImage (yt-dlp, the scalable icon, its own desktop entry,
# the licences) and packs it with appimagetool.
build_appimage() {
    local out="$DIST/Lider-$VERSION-x86_64.AppImage" appdir
    frontend
    third_party
    log "Building the AppDir (cargo tauri build --bundles appimage)"
    # The Tauri CLI runs "pkg-config" from PATH; honour PKG_CONFIG when it is set.
    local path="$PATH"
    if [ -n "${PKG_CONFIG:-}" ]; then
        mkdir -p "$WORK/bin"
        ln -sf "$(command -v "$PKG_CONFIG")" "$WORK/bin/pkg-config"
        path="$WORK/bin:$PATH"
    fi
    (cd "$ROOT" && PATH="$path" cargo tauri build --bundles appimage --config '{"build":{"beforeBuildCommand":""}}' >/dev/null)
    appdir="$(ls -d "$TARGET"/release/bundle/appimage/*.AppDir | head -1)"
    [ -d "$appdir" ] || { echo "no AppDir from the Tauri bundler" >&2; exit 1; }
    rm -rf "$WORK/appimage"
    mkdir -p "$WORK/appimage"
    cp -a "$appdir" "$WORK/appimage/Lider.AppDir"
    appdir="$WORK/appimage/Lider.AppDir"

    install -Dm755 "$(ytdlp)" "$appdir/usr/lib/lider/yt-dlp"
    rm -f "$appdir"/*.desktop "$appdir"/usr/share/applications/*.desktop
    install -Dm644 "$ROOT/lider.desktop" "$appdir/usr/share/applications/lider.desktop"
    cp "$ROOT/lider.desktop" "$appdir/lider.desktop"
    install -Dm644 "$ROOT/lider/ui/assets/icons/app.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/lider.svg"
    rm -f "$appdir/.DirIcon" "$appdir"/*.png "$appdir"/*.svg
    cp "$ROOT/lider/ui/assets/icons/app.svg" "$appdir/lider.svg"
    ln -sf lider.svg "$appdir/.DirIcon"
    local licenses="$appdir/usr/share/licenses/lider"
    install -Dm644 "$ROOT/LICENSE" "$licenses/LICENSE"
    cp -r "$WORK/third-party" "$licenses/third-party"
    system_licences "$appdir" "$licenses/system-libraries"

    local tool="$CACHE/appimagetool-1.9.0-x86_64.AppImage"
    if [ ! -x "$tool" ]; then
        log "Downloading appimagetool 1.9.0"
        mkdir -p "$CACHE"
        curl -fsSL -o "$tool.tmp" "$APPIMAGETOOL_URL"
        chmod +x "$tool.tmp"
        mv "$tool.tmp" "$tool"
    fi
    rm -f "$out"
    ARCH=x86_64 "$tool" --appimage-extract-and-run --no-appstream "$appdir" "$out" >/dev/null
    log "Done: $out ($(du -h "$out" | cut -f1))"
}

# The licence files of the distribution packages whose libraries the AppDir carries (dpkg on the
# Ubuntu builder, pacman on Arch), and a list of the libraries.
system_licences() {
    local appdir="$1" dest="$2" lib pkg
    mkdir -p "$dest"
    declare -A seen=()
    {
        printf '# System libraries in this AppImage\n\nEach library keeps its own licence; the texts are in the folders next to this file.\n\n'
        printf '| Library | Package |\n|---|---|\n'
        while IFS= read -r lib; do
            pkg=""
            if command -v dpkg >/dev/null; then
                pkg="$(dpkg -S "$(basename "$lib")" 2>/dev/null | head -1 | cut -d: -f1)"
                [ -n "$pkg" ] && [ -z "${seen[$pkg]:-}" ] && [ -f "/usr/share/doc/$pkg/copyright" ] &&
                    install -Dm644 "/usr/share/doc/$pkg/copyright" "$dest/$pkg/copyright"
            elif command -v pacman >/dev/null; then
                # Plugin modules keep their folder (usr/lib/gdk-pixbuf-2.0/…); libraries sit in /usr/lib, or
                # in a subfolder that linuxdeploy flattened.
                local cand
                for cand in "/${lib#"$appdir"/}" "/usr/lib/$(basename "$lib")" \
                            "$(find /usr/lib -name "$(basename "$lib")" -print -quit 2>/dev/null)"; do
                    [ -e "$cand" ] && pkg="$(pacman -Qqo "$cand" 2>/dev/null | head -1)" && [ -n "$pkg" ] && break
                done
                [ -n "$pkg" ] && [ -z "${seen[$pkg]:-}" ] && [ -d "/usr/share/licenses/$pkg" ] &&
                    cp -rL "/usr/share/licenses/$pkg" "$dest/$pkg"
            fi
            [ -n "$pkg" ] && seen[$pkg]=1
            printf '| %s | %s |\n' "$(basename "$lib")" "${pkg:-?}"
        done < <(find "$appdir/usr/lib" -name '*.so*' -type f | sort)
    } > "$dest/README.md"
}

case "${1:-}" in
    appimage) build_appimage ;;
    deb) build_deb ;;
    arch) build_arch ;;
    aur) build_aur ;;
    extension) build_extension ;;
    sdist) sdist ;;
    all) build_appimage; build_deb; build_arch; build_extension ;;
    *) sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'; exit 1 ;;
esac
