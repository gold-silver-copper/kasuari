#!/bin/bash
# usage: hard.sh [old|new] [NAME...]
#
# Runs the hard cases (`layouts-new hard`) in separate processes with their time limits, and
# compares each with its expectation. Exits with status 1 if any is worse than expected.
set -uo pipefail
source "$(dirname "$0")/common.sh"
V=${1:-new}; shift || true
build "$V"
BIN=$(layouts "$V")
worse=0
printf "%-14s %-9s %-10s %9s  %s\n" case expected got "cpu (ms)" detail
while IFS=$'\t' read -r name spec limit expected note; do
  if [ $# -gt 0 ] && [[ ! " $* " == *" $name "* ]]; then continue; fi
  out=$(timeout "$limit" "$BIN" case "$name" 2>&1); code=$?
  if [ $code = 124 ]; then got=timeout; cpu=">${limit}000"; detail=""
  else
    result=$(grep '^RESULT' <<<"$out")
    cpu=$(sed -n 's/^CPU per call: median \([0-9.]*\) ms.*/\1/p' <<<"$out")
    case $result in
      "RESULT VALID; calls agree") got=valid ;;
      "RESULT PANIC"*) got=panic ;;
      *) got=invalid ;;
    esac
    detail=${result#RESULT }
  fi
  if [ "$got" = "$expected" ]; then mark=""; elif [ "$got" = valid ]; then mark=" (better than expected)"; else mark=" WORSE THAN EXPECTED"; worse=1; fi
  printf "%-14s %-9s %-10s %9s  %s\n" "$name" "$expected" "$got$mark" "$cpu" "${detail:0:90}"
done < <("$BIN" hard)
exit $worse
