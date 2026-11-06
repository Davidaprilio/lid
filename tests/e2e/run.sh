#!/bin/sh
# Run the UI scenarios (ui/src/dev/e2e.ts) in the real app on a copy of the 1.2 golden profile.
#
#     tests/e2e/run.sh            # builds the app with its frontend first; runs the main and dialogs scenarios
#     LIDER_BIN=path tests/e2e/run.sh
#
# The real home is never touched: settings, data, cache and the instance socket are temporary.
# Needs a display (or run it under xvfb-run).
set -eu
cd "$(dirname "$0")/../.."
if [ -z "${LIDER_BIN:-}" ]; then
    cargo tauri build --debug --no-bundle >/dev/null
    LIDER_BIN=target/debug/lider
fi
status=0
for scenario in ${LIDER_E2E_SCENARIOS:-main dialogs}; do
    work="$(mktemp -d "${TMPDIR:-/tmp}/lider-e2e.XXXXXX")"
    cp -r tests/golden/v1.2/config "$work/config"
    cp -r tests/golden/v1.2/data "$work/data"
    mkdir -p "$work/cache"
    XDG_CONFIG_HOME="$work/config" XDG_DATA_HOME="$work/data" XDG_CACHE_HOME="$work/cache" \
        LIDER_SOCKET_NAME="lider-e2e-$$-$scenario" LIDER_E2E="$scenario" LIDER_E2E_OUT="$work/result.txt" \
        timeout 180 "$LIDER_BIN" >"$work/app.log" 2>&1 || true
    echo "== $scenario"
    if [ ! -f "$work/result.txt" ]; then
        echo "FAIL: no result (see $work/app.log)"
        status=1
        continue
    fi
    cat "$work/result.txt"
    head -n 1 "$work/result.txt" | grep -qx PASS || status=1
done
exit "$status"
