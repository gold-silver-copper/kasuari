#!/bin/bash
# usage: bench.sh SET ROUNDS
#
# Compares the speed of old and new on a set of layouts (`layouts-new sets`). Each round runs both
# in fresh processes, one after the other, alternating which goes first, and compares the per-thread
# CPU time (clock_gettime(CLOCK_THREAD_CPUTIME_ID)) of the two runs of the same round, which works
# on a loaded machine.
set -uo pipefail
source "$(dirname "$0")/common.sh"
SET=$1; R=$2
build old new
OUT=$(mktemp)
for r in $(seq 1 "$R"); do
  if [ $((r % 2)) = 0 ]; then order="old new"; else order="new old"; fi
  for v in $order; do
    res=$(timeout 600 "$(layouts_bench $v)" bench "$SET" 2>/dev/null)
    if [ $? = 124 ]; then echo "$r $v HUNG" >> "$OUT"; else echo "$r $v $res" >> "$OUT"; fi
  done
done
python3 - "$OUT" "$SET" <<'PY'
import sys, re, statistics
rows = [l.split(" ", 2) for l in open(sys.argv[1]).read().splitlines()]
t = {}
for r, v, res in rows:
    m = re.search(r"thread_cpu_us_per_layout=([\d.]+)", res)
    t.setdefault(r, {})[v] = float(m[1]) if m else None
for v in ("old", "new"):
    xs = [x[v] for x in t.values() if x.get(v) is not None]
    hung = sum(1 for x in t.values() if x.get(v) is None)
    print(f"{sys.argv[2]}: {v} median {statistics.median(xs):.1f} us/layout (min {min(xs):.1f}) over {len(xs)} runs, {hung} hung")
ratios = [x["new"] / x["old"] for x in t.values() if x.get("old") and x.get("new")]
q = statistics.quantiles(ratios, n=4) if len(ratios) > 1 else [ratios[0]] * 3
print(f"{sys.argv[2]}: new/old per round: median {statistics.median(ratios):.3f}, middle half {q[0]:.3f}-{q[2]:.3f}, new faster in {sum(r < 1 for r in ratios)} of {len(ratios)}")
PY
