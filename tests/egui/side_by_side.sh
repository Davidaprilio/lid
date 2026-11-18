#!/bin/sh
# The main window of the Tauri build (left) and of the egui build (right) in the three themes, on copies of
# the 1.2 golden profile under Xvfb: OUT_DIR/<theme>.png.
#
#     tests/egui/side_by_side.sh OUT_DIR
#
# Needs release builds of both (see tests/egui/perf.sh), xvfb-run and ImageMagick (/usr/bin).
set -eu
cd "$(dirname "$0")/../.."
out="$(realpath -m "${1:?usage: tests/egui/side_by_side.sh OUT_DIR}")"
mkdir -p "$out"
work="$(mktemp -d "${TMPDIR:-/tmp}/lider-sbs.XXXXXX")"
trap 'rm -rf "$work"' EXIT

cat > "$work/one.sh" <<'SH'
#!/bin/sh
# one.sh BINARY PROFILE-DIR OUT.png
export XDG_CONFIG_HOME="$2/config" XDG_DATA_HOME="$2/data" XDG_CACHE_HOME="$2/cache" HOME="$2"
export LIDER_SOCKET_NAME="lider-sbs-$$" GDK_BACKEND=x11
"$1" >/dev/null 2>&1 &
pid=$!
w=$(timeout 60 xdotool search --sync --onlyvisible --name "Linux Internet Downloader" | head -1)
sleep 5
/usr/bin/import -window "$w" "$3"
kill "$pid"
SH
chmod +x "$work/one.sh"

for theme in ember ember-dark dark; do
    for name in lider lider-egui; do
        rm -rf "$work/p"; mkdir -p "$work/p"
        cp -r tests/golden/v1.2/config tests/golden/v1.2/data "$work/p/"
        python3 -I -c 'import json,sys; p=sys.argv[1]; s=json.load(open(p)); s.update(theme=sys.argv[2], follow_system=False, show_details=True); json.dump(s,open(p,"w"))' \
            "$work/p/config/lider/settings.json" "$theme"
        env -u WAYLAND_DISPLAY PATH="/usr/bin:$PATH" xvfb-run -a -s "-screen 0 1280x800x24" \
            "$work/one.sh" "$PWD/target/release/$name" "$work/p" "$work/$name.png"
    done
    /usr/bin/magick "$work/lider.png" "$work/lider-egui.png" -background '#808080' -splice 12x0 +append "$out/$theme.png"
    echo "$out/$theme.png"
done
