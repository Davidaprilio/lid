#!/bin/sh
# A screenshot of the egui window (crates/lider-gui) on a copy of the 1.2 golden profile, under Xvfb, so the
# user's own session and home are never touched.
#
#     tests/egui/shot.sh OUT.png ['{"theme": "ember", "show_details": true}'] [WIDTHxHEIGHT]
#
# LIDER_RUN="action;action" (debug builds) runs menu actions at start, e.g. LIDER_RUN=options for a dialog.
#
# The JSON is merged into the copy's settings.json. Needs xvfb-run, xdotool and ImageMagick's import
# (/usr/bin).
set -eu
cd "$(dirname "$0")/../.."
out="$(realpath -m "${1:?usage: tests/egui/shot.sh OUT.png [settings-json] [WxH]}")"
patch="${2:-{\}}"
size="${3:-1100x700}"
work="$(mktemp -d "${TMPDIR:-/tmp}/lider-egui-shot.XXXXXX")"
trap 'rm -rf "$work"' EXIT
cp -r tests/golden/v1.2/config tests/golden/v1.2/data "$work/"
python3 -I -c 'import json,sys; p=sys.argv[1]; s=json.load(open(p)); s.update(json.loads(sys.argv[2])); json.dump(s,open(p,"w"),indent=2)' \
    "$work/config/lider/settings.json" "$patch"
cargo build -q -p lider-gui
cat > "$work/run.sh" <<SH
env -u WAYLAND_DISPLAY XDG_CONFIG_HOME=$work/config XDG_DATA_HOME=$work/data XDG_CACHE_HOME=$work/cache \
    LIDER_SOCKET_NAME=lider-egui-shot-\$\$ $PWD/target/debug/lider-egui > $work/app.log 2>&1 &
pid=\$!
sleep 5
# No window manager under Xvfb: move the dialog windows beside the main window and raise them.
x=120
for w in \$(xdotool search --onlyvisible --name . 2>/dev/null); do
    case "\$(xdotool getwindowname \$w)" in "Linux Internet Downloader"*) continue ;; esac
    xdotool windowmove \$w \$x 60 windowraise \$w
    x=\$((x + 60))
done
sleep 1
/usr/bin/import -window root "$out"
kill \$pid
SH
PATH="/usr/bin:$PATH" xvfb-run -a -s "-screen 0 ${size}x24" sh "$work/run.sh"
echo "$out"
