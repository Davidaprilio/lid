#!/bin/sh
# Screenshots of every window and message box in the three themes (LIDER_E2E=shots), for the side-by-side
# review with 1.x (tests/e2e/shots_1x.py). Runs the app under Xvfb on a copy of the 1.2 golden profile.
#
#     tests/e2e/shots.sh OUT_DIR            # builds the app first
#     LIDER_BIN=path tests/e2e/shots.sh OUT_DIR
set -eu
cd "$(dirname "$0")/../.."
out="$(realpath -m "${1:?usage: tests/e2e/shots.sh OUT_DIR}")"
mkdir -p "$out"
if [ -z "${LIDER_BIN:-}" ]; then
    cargo tauri build --debug --no-bundle >/dev/null
    LIDER_BIN=target/debug/lider
fi
work="$(mktemp -d "${TMPDIR:-/tmp}/lider-shots.XXXXXX")"
cp -r tests/golden/v1.2/config "$work/config"
cp -r tests/golden/v1.2/data "$work/data"
mkdir -p "$work/cache" "$work/www"
head -c 65536 /dev/zero > "$work/www/archive.zip"
port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
python3 -m http.server "$port" --bind 127.0.0.1 --directory "$work/www" >/dev/null 2>&1 &
server=$!
trap 'kill $server 2>/dev/null' EXIT
XDG_CONFIG_HOME="$work/config" XDG_DATA_HOME="$work/data" XDG_CACHE_HOME="$work/cache" \
    LIDER_SOCKET_NAME="lider-shots-$$" LIDER_E2E=shots LIDER_E2E_OUT="$work/result.txt" \
    LIDER_E2E_SHOTS="$out" LIDER_SHOTS_PORT="$port" GDK_BACKEND=x11 PATH="/usr/bin:$PATH" \
    xvfb-run -a -s "-screen 0 1600x1000x24" timeout 600 "$LIDER_BIN" >"$work/app.log" 2>&1 || true
cat "$work/result.txt" 2>/dev/null || { echo "FAIL: no result (see $work/app.log)"; exit 1; }
ls "$out" | wc -l
