#!/bin/sh
# Start-up time, memory and idle CPU of LID, on copies of the 1.2 golden profile under Xvfb (software
# OpenGL, so the CPU figures are higher than on a real desktop).
#
#     tests/egui/perf.sh [RUNS]
#     COMPARE_BIN=path/to/other/lider tests/egui/perf.sh   # next to another build (e.g. the Tauri beta)
#
# Needs `cargo build --release -p lider-gui`, xvfb-run and xdotool. Memory is the PSS of the program and
# every process it started (a WebView's helpers), 8 s after its window showed.
set -eu
cd "$(dirname "$0")/../.."
runs="${1:-3}"
work="$(mktemp -d "${TMPDIR:-/tmp}/lider-perf.XXXXXX")"
trap 'rm -rf "$work"' EXIT

cat > "$work/one.sh" <<'SH'
#!/bin/sh
# one.sh BINARY PROFILE-DIR: prints "ms pss_kib cpu_percent"
bin="$1"; dir="$2"
export XDG_CONFIG_HOME="$dir/config" XDG_DATA_HOME="$dir/data" XDG_CACHE_HOME="$dir/cache" HOME="$dir"
export LIDER_SOCKET_NAME="lider-perf-$$" GDK_BACKEND=x11
tree() { echo "$1"; for c in $(cat /proc/"$1"/task/*/children 2>/dev/null); do tree "$c"; done; }
start=$(date +%s%N)
"$bin" >/dev/null 2>&1 &
pid=$!
timeout 60 xdotool search --sync --onlyvisible --name "Linux Internet Downloader" >/dev/null 2>&1
shown=$(date +%s%N)
sleep 8
ticks() { t=0; for p in $(tree "$pid"); do set -- $(cut -d' ' -f14,15 /proc/"$p"/stat 2>/dev/null || echo 0 0); t=$((t + $1 + $2)); done; echo "$t"; }
pss=0
for p in $(tree "$pid"); do
    v=$(awk '/^Pss:/ {print $2}' /proc/"$p"/smaps_rollup 2>/dev/null || echo 0)
    pss=$((pss + ${v:-0}))
done
a=$(ticks); sleep 10; b=$(ticks)
hz=$(getconf CLK_TCK)
cpu=$(echo "$a $b $hz" | awk '{printf "%.1f", ($2 - $1) / $3 / 10 * 100}')
kill "$pid" 2>/dev/null; sleep 1; pkill -P "$pid" 2>/dev/null || true
echo "$(( (shown - start) / 1000000 )) $pss $cpu"
SH
chmod +x "$work/one.sh"

for bin in "$PWD/target/release/lider" ${COMPARE_BIN:+"$(realpath "$COMPARE_BIN")"}; do
    [ -x "$bin" ] || { echo "missing $bin"; exit 1; }
    name="$([ "$bin" = "$PWD/target/release/lider" ] && echo lider || echo compared)"
    for i in $(seq "$runs"); do
        rm -rf "$work/p"; mkdir -p "$work/p"
        cp -r tests/golden/v1.2/config tests/golden/v1.2/data "$work/p/"
        r=$(env -u WAYLAND_DISPLAY PATH="/usr/bin:$PATH" xvfb-run -a -s "-screen 0 1280x800x24" "$work/one.sh" "$bin" "$work/p")
        echo "$name $r"
    done
done | awk '
    { ms[$1] += $2; pss[$1] += $3; cpu[$1] += $4; n[$1]++ }
    END {
        printf "%-11s %10s %12s %10s\n", "window", "shown (ms)", "memory (MiB)", "idle CPU %"
        for (k in n) printf "%-11s %10.0f %12.1f %10.1f\n", k, ms[k]/n[k], pss[k]/n[k]/1024, cpu[k]/n[k]
    }'
for b in target/release/lider ${COMPARE_BIN:+"$COMPARE_BIN"}; do
    printf '%-40s %6.1f MiB %4d shared libraries\n' "$b" "$(stat -c %s "$b" | awk '{print $1/1048576}')" \
        "$(ldd "$b" 2>/dev/null | grep -c '=>')"
done
