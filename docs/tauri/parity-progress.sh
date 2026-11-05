#!/bin/sh
# Open, done and changed parity items per checklist. Cut-over needs "open" to be 0 everywhere.
cd "$(dirname "$0")/parity" || exit 1
printf '%-32s %6s %6s %8s\n' file open done changed
for f in *.md; do
    printf '%-32s %6s %6s %8s\n' "$f" \
        "$(grep -c -- '- \[ \]' "$f")" "$(grep -c -- '- \[x\]' "$f")" "$(grep -c -- '- \[~\]' "$f")"
done
