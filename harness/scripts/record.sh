#!/bin/bash
# usage: record.sh SET [OUT]
#
# Records the constraint systems ratatui builds for a set of layouts, using worktrees/record (NEW
# with recording), to OUT (default corpus/generated/SET.txt). The systems don't depend on the
# solver's version, only on ratatui's.
set -uo pipefail
source "$(dirname "$0")/common.sh"
SET=$1; OUT=${2:-$HARNESS/corpus/generated/$SET.txt}
build record
mkdir -p "$(dirname "$OUT")"
"$(layouts record)" record "$SET" 2>&1 >/dev/null | sed -n 's/^KASREC //p' > "$OUT"
echo "$OUT: $(grep -c '^## ' "$OUT") systems, $(grep -vc '^#' "$OUT") constraints"
