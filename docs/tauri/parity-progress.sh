#!/bin/sh
# Open, done and changed parity items per checklist (bulleted "- [ ]" and numbered "1. [ ]" items).
# Cut-over needs "open" to be 0 everywhere.
cd "$(dirname "$0")/parity" || exit 1
count() { grep -cE "^[[:space:]]*(-|[0-9]+\.) \[$1\]" "$2"; }
printf '%-32s %6s %6s %8s\n' file open done changed
for f in *.md; do
    printf '%-32s %6s %6s %8s\n' "$f" "$(count ' ' "$f")" "$(count x "$f")" "$(count '~' "$f")"
done
