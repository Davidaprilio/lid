#!/usr/bin/env bash
# Build LID packages into dist/.
#
#   ./packaging/build.sh appimage   self-contained AppImage (bundled Python, PySide6, yt-dlp)
#   ./packaging/build.sh deb        Debian 12+/Ubuntu 22.04+ package (system Python, bundled PySide6)
#   ./packaging/build.sh arch       Arch package (.pkg.tar.zst) built with makepkg
#   ./packaging/build.sh aur        PKGBUILD + .SRCINFO for the AUR (needs AUR_SOURCE_URL)
#   ./packaging/build.sh all        appimage, deb and arch
#
# Settings (environment):
#   MAINTAINER       "Name <email>" written into the packages
#   PKG_LICENSE      SPDX licence id for the package metadata
#   PROJECT_URL      project home page
#   AUR_SOURCE_URL   where dist/lider-<version>.tar.gz will be downloadable from (aur target)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PKG="$ROOT/packaging"
DIST="$ROOT/dist"
WORK="$ROOT/build/packaging"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/lider-build"

VERSION="$(sed -n 's/^__version__ = "\(.*\)"/\1/p' "$ROOT/lider/__init__.py")"
MAINTAINER="${MAINTAINER:-David Aprilio <david1.aprilio4@gmail.com>}"
PKG_LICENSE="${PKG_LICENSE:-MIT}"
PROJECT_URL="${PROJECT_URL:-https://github.com/Davidaprilio/lid}"
SUMMARY="Linux Internet Downloader (LID): fast multi-connection downloads"

# Pinned versions for the AppImage (the ones LID was tested with).
APPIMAGE_PYTHON="3.12"
APPIMAGE_PYSIDE="PySide6-Essentials==6.11.1"
APPIMAGE_YTDLP="yt-dlp==2026.8.19"
APPIMAGETOOL_URL="https://github.com/AppImage/appimagetool/releases/download/1.9.0/appimagetool-x86_64.AppImage"

log() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }

# ---------------------------------------------------------------- source tarball
sdist() {
    local name="lider-$VERSION" out="$DIST/lider-$VERSION.tar.gz"
    mkdir -p "$DIST" "$WORK"
    rm -rf "${WORK:?}/$name"
    mkdir -p "$WORK/$name"
    cp -r "$ROOT/lider" "$ROOT/pyproject.toml" "$ROOT/README.md" "$ROOT/LICENSE" "$ROOT/lider.desktop" "$WORK/$name/"
    find "$WORK/$name" -name __pycache__ -prune -exec rm -rf {} +
    tar --sort=name --mtime="@0" --owner=0 --group=0 --numeric-owner \
        -czf "$out" -C "$WORK" "$name"
    echo "$out"
}

# Copy the application into a filesystem tree: <root>/<python-dir>/lider, launcher, desktop entry, icon.
install_tree() {
    local root="$1" pydir="$2" python="$3"
    install -d "$root$pydir" "$root/usr/bin"
    cp -r "$ROOT/lider" "$root$pydir/"
    find "$root$pydir/lider" -name __pycache__ -prune -exec rm -rf {} +
    find "$root$pydir/lider" -type d -exec chmod 755 {} + -o -type f -exec chmod 644 {} +
    printf '#!%s\nimport sys\nfrom lider.app import main\nsys.exit(main())\n' "$python" > "$root/usr/bin/lider"
    chmod 755 "$root/usr/bin/lider"
    install -Dm644 "$ROOT/lider.desktop" "$root/usr/share/applications/lider.desktop"
    install -Dm644 "$ROOT/lider/ui/assets/icons/app.svg" "$root/usr/share/icons/hicolor/scalable/apps/lider.svg"
    install -Dm644 "$ROOT/README.md" "$root/usr/share/doc/lider/README.md"
}

# Remove the parts of a PySide6 install that LID never loads (QML, designer tools...).
trim_pyside() {
    local site="$1"
    rm -rf "$site/PySide6/Qt/qml" "$site/PySide6/Qt/translations/qtwebengine"* \
           "$site/PySide6/"{assistant,designer,linguist,lupdate,lrelease,qmlformat,qmllint,qmlls,qmlcachegen} \
           "$site/PySide6/"{rcc,uic,balsam,balsamui,svgtoqml}
}

# Copy the licence texts found in the *.dist-info folders of <site>... into <dest>/<package>/.
copy_licenses() {
    local dest="$1" site info pkg file
    shift
    for site in "$@"; do
        for info in "$site"/*.dist-info; do
            pkg="$(basename "$info")"
            pkg="${pkg%%-[0-9]*}"
            while IFS= read -r -d '' file; do
                install -Dm644 "$file" "$dest/$pkg/$(basename "$file")"
            done < <(find "$info" -type f \( -iname 'LICENSE*' -o -iname 'COPYING*' -o -path '*/licenses/*' \) -print0)
        done
    done
}

# ---------------------------------------------------------------- .deb
# Self-contained for Debian 12+/Ubuntu 22.04+: system Python 3.10+, PySide6 and yt-dlp carried in
# /usr/lib/lider. PySide6's abi3 wheels work with every Python 3.10+, and Ubuntu 24.04 has no PySide6.
DEB_DEPENDS="python3 (>= 3.10), libc6 (>= 2.34), libgl1, libegl1, libfontconfig1, libfreetype6, \
libdbus-1-3, libglib2.0-0t64 | libglib2.0-0, libxkbcommon0, libxkbcommon-x11-0, libx11-xcb1, \
libxcb-cursor0, libxcb-icccm4, libxcb-image0, libxcb-keysyms1, libxcb-randr0, libxcb-render-util0, \
libxcb-shape0, libxcb-xinerama0, libxcb-xkb1"

build_deb() {
    local root="$WORK/deb/root" out="$DIST/lider_${VERSION}-1_amd64.deb" lib=/usr/lib/lider
    log "Building $out"
    rm -rf "$WORK/deb"
    install_tree "$root" "$lib" /usr/bin/python3
    cat > "$root/usr/bin/lider" <<'EOF'
#!/usr/bin/python3
import site
import sys

sys.path[0:0] = ["/usr/lib/lider", "/usr/lib/lider/vendor"]
# A yt-dlp the user installed with "pip install --user" wins; the bundled one beats an older distro copy.
user = site.getusersitepackages()
sys.path.insert(sys.path.index(user) + 1 if user in sys.path else 2, "/usr/lib/lider/ytdlp")
from lider.app import main

sys.exit(main())
EOF
    log "Bundling $APPIMAGE_PYSIDE (abi3 wheels) and $APPIMAGE_YTDLP"
    python3 -m pip install --quiet --no-cache-dir --disable-pip-version-check --no-compile \
        --target "$root$lib/vendor" --only-binary=:all: --implementation cp --abi abi3 \
        --python-version 3.10 --platform manylinux_2_34_x86_64 "$APPIMAGE_PYSIDE"
    python3 -m pip install --quiet --no-cache-dir --disable-pip-version-check --no-compile --no-deps \
        --target "$root$lib/ytdlp" "$APPIMAGE_YTDLP"
    rm -rf "$root$lib"/{vendor,ytdlp}/bin
    trim_pyside "$root$lib/vendor"
    copy_licenses "$root/usr/share/doc/lider/third-party" "$root$lib/vendor" "$root$lib/ytdlp"
    install -Dm644 -t "$root/usr/share/doc/lider/third-party/PySide6" "$PKG/licenses/"*
    find "$root$lib" -type d -exec chmod 755 {} +

    { printf 'Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\nUpstream-Name: lider\n\n'
      printf 'Files: *\nCopyright: 2026 %s\nLicense: %s\n' "$MAINTAINER" "$PKG_LICENSE"
      sed 's/^$/./; s/^/ /' "$ROOT/LICENSE"
      printf '\nFiles: usr/lib/lider/vendor/* usr/lib/lider/ytdlp/*\n'
      printf 'Comment: PySide6/Qt (LGPL-3.0), shiboken6 and yt-dlp (Unlicense); see /usr/share/doc/lider/third-party/\n'
    } > "$root/usr/share/doc/lider/copyright"
    install -d "$root/DEBIAN"
    local size
    size="$(du -sk --exclude=DEBIAN "$root" | cut -f1)"
    sed -e "s|@VERSION@|$VERSION|" -e "s|@MAINTAINER@|$MAINTAINER|" -e "s|@SIZE@|$size|" \
        -e "s|@SUMMARY@|$SUMMARY|" -e "s|@URL@|${PROJECT_URL:-https://example.invalid/lider}|" \
        -e "s#@DEPENDS@#$DEB_DEPENDS#" "$PKG/deb/control.in" > "$root/DEBIAN/control"
    [ -n "$PROJECT_URL" ] || sed -i '/^Homepage:/d' "$root/DEBIAN/control"
    install -m755 "$PKG/deb/postinst" "$PKG/deb/prerm" "$root/DEBIAN/"
    (cd "$root" && find usr -type f -print0 | sort -z | xargs -0 md5sum > DEBIAN/md5sums)

    local tmp="$WORK/deb/ar"
    mkdir -p "$tmp"
    echo "2.0" > "$tmp/debian-binary"
    local tarflags=(--sort=name --mtime="@0" --owner=0 --group=0 --numeric-owner)
    tar "${tarflags[@]}" -cJf "$tmp/control.tar.xz" -C "$root/DEBIAN" .
    tar "${tarflags[@]}" -cJf "$tmp/data.tar.xz" -C "$root" ./usr
    rm -f "$out"
    (cd "$tmp" && ar rcD "$out" debian-binary control.tar.xz data.tar.xz)
    log "Done: $out ($(du -h "$out" | cut -f1))"
}

# ---------------------------------------------------------------- Arch / AUR
write_pkgbuild() {
    local dest="$1" source="$2" sha="$3"
    sed -e "s|@VERSION@|$VERSION|" -e "s|@MAINTAINER@|$MAINTAINER|" -e "s|@LICENSE@|$PKG_LICENSE|" \
        -e "s|@SUMMARY@|$SUMMARY|" -e "s|@URL@|$PROJECT_URL|" -e "s|@SOURCE@|$source|" -e "s|@SHA256@|$sha|" \
        "$PKG/arch/PKGBUILD.in" > "$dest/PKGBUILD"
}

build_arch() {
    local tarball sha dir="$WORK/arch"
    tarball="$(sdist)"
    sha="$(sha256sum "$tarball" | cut -d' ' -f1)"
    log "Building Arch package with makepkg"
    rm -rf "$dir"
    mkdir -p "$dir"
    cp "$tarball" "$dir/"
    write_pkgbuild "$dir" "lider-\$pkgver.tar.gz" "$sha"
    (cd "$dir" && PKGDEST="$DIST" makepkg -f --nodeps --noconfirm)
    log "Done: $(ls "$DIST"/lider-"$VERSION"-*-any.pkg.tar.* | head -1)"
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
    write_pkgbuild "$dir" "lider-\$pkgver.tar.gz::$AUR_SOURCE_URL" "$sha"
    (cd "$dir" && makepkg --printsrcinfo > .SRCINFO)
    log "AUR files in $dir (upload $tarball to $AUR_SOURCE_URL first)"
}

# ---------------------------------------------------------------- AppImage
build_appimage() {
    local appdir="$WORK/appimage/Lider.AppDir" out="$DIST/Lider-$VERSION-x86_64.AppImage"
    log "Building $out"
    rm -rf "$WORK/appimage"
    mkdir -p "$appdir/usr" "$CACHE"

    log "Portable Python $APPIMAGE_PYTHON (python-build-standalone via uv)"
    UV_PYTHON_INSTALL_DIR="$CACHE/python" uv python install "$APPIMAGE_PYTHON" >/dev/null
    local pyhome
    pyhome="$(UV_PYTHON_INSTALL_DIR="$CACHE/python" uv python find --managed-python "$APPIMAGE_PYTHON")"
    pyhome="$(cd "$(dirname "$pyhome")/.." && pwd -P)"
    cp -a "$pyhome" "$appdir/usr/python"
    # This copy belongs to the AppImage alone, so uv's "externally managed" marker does not apply.
    rm -f "$appdir"/usr/python/lib/python*/EXTERNALLY-MANAGED
    local py="$appdir/usr/python/bin/python3"

    log "Installing $APPIMAGE_PYSIDE and $APPIMAGE_YTDLP"
    "$py" -m pip install --quiet --no-cache-dir --disable-pip-version-check "$APPIMAGE_PYSIDE" "$APPIMAGE_YTDLP"
    local site
    site="$("$py" -c 'import sysconfig; print(sysconfig.get_paths()["purelib"])')"
    cp -r "$ROOT/lider" "$site/"

    log "Trimming parts LID does not use"
    rm -rf "$appdir/usr/python/lib/python$APPIMAGE_PYTHON/"{test,idlelib,tkinter,turtledemo,ensurepip} \
           "$appdir/usr/python/lib/"{libtcl*,libtk*,tcl*,tk*,itcl*,thread*} "$appdir/usr/python/include"
    trim_pyside "$site"
    "$py" -m pip uninstall --quiet -y pip 2>/dev/null || true
    find "$appdir" -name __pycache__ -prune -exec rm -rf {} +
    "$py" -m compileall -q "$site/lider" "$site/yt_dlp" >/dev/null
    local licenses="$appdir/usr/share/licenses/lider"
    install -Dm644 "$ROOT/LICENSE" "$licenses/LICENSE"
    install -Dm644 "$appdir/usr/python/lib/python$APPIMAGE_PYTHON/LICENSE.txt" "$licenses/third-party/python/LICENSE.txt"
    copy_licenses "$licenses/third-party" "$site"
    install -Dm644 -t "$licenses/third-party/PySide6" "$PKG/licenses/"*

    install -m755 "$PKG/appimage/AppRun" "$appdir/AppRun"
    sed 's/^Exec=.*/Exec=lider %U/' "$ROOT/lider.desktop" > "$appdir/lider.desktop"
    cp "$ROOT/lider/ui/assets/icons/app.svg" "$appdir/lider.svg"
    ln -sf lider.svg "$appdir/.DirIcon"
    install -Dm644 "$ROOT/lider.desktop" "$appdir/usr/share/applications/lider.desktop"
    install -Dm644 "$ROOT/lider/ui/assets/icons/app.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/lider.svg"

    local tool="$CACHE/appimagetool-1.9.0-x86_64.AppImage"
    if [ ! -x "$tool" ]; then
        log "Downloading appimagetool 1.9.0"
        curl -fsSL -o "$tool.tmp" "$APPIMAGETOOL_URL"
        chmod +x "$tool.tmp"
        mv "$tool.tmp" "$tool"
    fi
    mkdir -p "$DIST"
    rm -f "$out"
    ARCH=x86_64 "$tool" --appimage-extract-and-run --no-appstream "$appdir" "$out" >/dev/null
    log "Done: $out ($(du -h "$out" | cut -f1))"
}

case "${1:-}" in
    appimage) build_appimage ;;
    deb) build_deb ;;
    arch) build_arch ;;
    aur) build_aur ;;
    sdist) sdist ;;
    all) build_appimage; build_deb; build_arch ;;
    *) sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'; exit 1 ;;
esac
