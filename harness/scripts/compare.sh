#!/bin/bash
# usage: compare.sh identical|different
#
# Compares old and new (see ../setup.sh) on everything:
#
# - identical: for changes that mustn't change results (performance). The recorded constraint
#   systems of every set, and 10,000 random operation sequences (twice: extreme and small values),
#   must give bit-identical results. Exits with status 1 otherwise.
# - different: for changes that are allowed to change results. Compares outcomes of the random
#   sequences and judges every disagreement exactly (reference/exact.py feasible), compares the
#   objectives on the recorded systems, checks both versions against the exact optimum on the hard
#   cases' systems, and runs the hard cases with both.
set -uo pipefail
source "$(dirname "$0")/common.sh"
MODE=$1
build solver
S=$(solver)
GEN="$HARNESS/corpus/generated"
for set in typical random extreme big large hard; do
  [ -s "$GEN/$set.txt" ] || "$HARNESS/scripts/record.sh" "$set"
done
if [ "$MODE" = identical ]; then
  "$S" replay "$GEN"/{typical,random,extreme,big,large,hard}.txt "$HARNESS"/corpus/hard/*.txt || exit 1
  seq 0 9 | xargs -P 5 -I{} sh -c "'$S' random \$(({}*1000+1)) \$(({}*1000+1000)) | tail -1" || exit 1
  seq 0 9 | xargs -P 5 -I{} sh -c "'$S' random \$(({}*1000+1)) \$(({}*1000+1000)) --mild | tail -1" || exit 1
  exit 0
fi
OUT="$HARNESS/out/$(date +%Y%m%d-%H%M%S)"; mkdir -p "$OUT"
echo "== outcomes of random operation sequences (dumps in $OUT)"
for mild in "" --mild; do
  "$S" outcomes 1 10000 $mild --dump "$OUT/dump$mild" | head -1
  ls "$OUT/dump$mild"/*.txt >/dev/null 2>&1 && python3 "$HARNESS/reference/exact.py" feasible "$OUT/dump$mild"/*.txt | tail -1
done
echo "== objectives on the recorded systems"
"$S" objectives "$GEN"/{typical,random,extreme,big,large,hard}.txt | grep '^SUMMARY'
echo "== exact optimum on the hard systems (slow)"
for v in old new; do
  "$S" values $v "$HARNESS"/corpus/hard/*.txt > "$OUT/values-$v.txt"
  echo "$v: $(python3 "$HARNESS/reference/exact.py" check "$OUT/values-$v.txt" "$HARNESS"/corpus/hard/*.txt | tee "$OUT/exact-$v.txt" | tail -1)"
done
echo "== hard cases"
for v in old new; do echo "-- $v"; "$HARNESS/scripts/hard.sh" $v; done
