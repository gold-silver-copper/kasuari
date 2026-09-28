#!/bin/bash
# usage: search.sh [old|new] KIND SEEDS N [TIMEOUT]
#
# Random layout search: seeds 1..SEEDS, N layouts each (kinds: realistic, extreme, big, bignz),
# each seed in its own process (6 in parallel) with a timeout (default 120 s). Each layout is solved
# 3 times. Reports panics, calls that disagree, invalid areas and hangs, with the spec of the first
# few, for `case` or `minimize.py`.
set -uo pipefail
source "$(dirname "$0")/common.sh"
V=$1; KIND=$2; SEEDS=$3; N=$4; T=${5:-120}
build "$V"
BIN=$(layouts "$V"); D=$(mktemp -d)
# (arguments instead of -I, which hits macOS's command length limit)
seq 1 "$SEEDS" | xargs -P 6 -n 1 sh -c 'timeout "$1" "$2" search "$3" "$6" "$4" > "$5/$6.out" 2>/dev/null; echo $? > "$5/$6.code"' _ "$T" "$BIN" "$KIND" "$N" "$D"
python3 - "$D" <<'PY'
import sys, glob, os, re
d = sys.argv[1]
totals = {"panics": [], "disagree": [], "invalid": []}
hung = []
for code_file in glob.glob(d + "/*.code"):
    out = open(code_file[:-5] + ".out").read().splitlines()
    specs = {l.split(" ")[1]: l.split(" ", 2)[2] for l in out if l.startswith("CASE ")}
    if open(code_file).read().strip() != "0":
        hung.append(out[-1].split(" ", 2)[2] if out else "?")
        continue
    kind_seed = next(iter(specs)).rsplit(":", 1)[0] if specs else "?"
    for k, v in re.findall(r"(\w+)=\[([^\]]*)\]", out[-1]):
        for i in filter(None, v.split(", ")):
            totals[k].append(specs[f"{kind_seed}:{i}"])
print(f"hung: {len(hung)}, " + ", ".join(f"{k}: {len(v)}" for k, v in totals.items()))
for k, v in [("hung", hung)] + list(totals.items()):
    for spec in v[:3]:
        print(f"  {k}: {spec}")
PY
