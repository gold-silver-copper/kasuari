#!/usr/bin/env python3
"""Shrinks a layout while it still fails in the same way.

usage: minimize.py [old|new] SPEC panic|invalid|timeout:SECONDS

Removes chunks of constraints (halves, then quarters, and so on down to single ones), then
replaces constraints with simpler ones and shrinks the area, keeping every change that still
fails. Every try runs `layouts-VERSION case SPEC` in a new process. For `panic` and `invalid`,
tries are stopped after 5 times the original case's time (at least 1 s): a variant that takes
much longer isn't what's being looked for.
"""
import subprocess, sys, time

HARNESS = __file__.rsplit("/scripts/", 1)[0]
version, spec, kind = sys.argv[1], sys.argv[2], sys.argv[3]
binary = f"{HARNESS}/layouts/{version}/target/release/layouts-{version}"
limit = float(kind.split(":")[1]) if kind.startswith("timeout") else 600
tries = 0


def fails(spec):
    global tries
    tries += 1
    try:
        out = subprocess.run([binary, "case", spec], capture_output=True, text=True, timeout=limit).stdout
    except subprocess.TimeoutExpired:
        return kind.startswith("timeout")
    result = next((l for l in out.splitlines() if l.startswith("RESULT")), "")
    return (kind == "panic" and "PANIC" in result) or (kind == "invalid" and "INVALID" in result)


def split(spec):
    head, constraints = spec.rsplit(":", 1)
    items = []
    for c in constraints.split(","):
        c, _, n = c.partition("*")
        items += [c] * int(n or 1)
    return head, items


def join(head, items):
    return head + ":" + ",".join(items)


start = time.time()
assert fails(spec), "the spec doesn't fail in that way"
if not kind.startswith("timeout"):
    limit = max(1.0, 5 * (time.time() - start))
head, items = split(spec)

changed = True
while changed:
    changed = False
    # remove chunks, largest first
    chunk = max(1, len(items) // 2)
    while chunk >= 1:
        i = 0
        while i < len(items) and len(items) > 1:
            candidate = items[:i] + items[i + chunk:]
            if candidate and fails(join(head, candidate)):
                items = candidate
                changed = True
                print(f"{len(items)} constraints ({tries} tries)", flush=True)
            else:
                i += chunk
        chunk //= 2
    # simpler constraints
    for i in range(len(items)):
        for simpler in ["Min(0)", "Length(0)", "Fill(1)", "Min(1)", "Length(1)"]:
            if items[i] != simpler and fails(join(head, items[:i] + [simpler] + items[i + 1:])):
                items[i] = simpler
                changed = True
                break
    # smaller area, no spacing
    d, flex, spacing, rect = head.split(":")
    x, y, w, h = map(int, rect.split(","))
    size = h if d == "v" else w
    for smaller in [1, 2, 5, 10, 20, 50, 100, 200]:
        if smaller < size:
            rect2 = f"{x},{y},{w},{smaller}" if d == "v" else f"{x},{y},{smaller},{h}"
            if fails(join(f"{d}:{flex}:{spacing}:{rect2}", items)):
                head, rect = f"{d}:{flex}:{spacing}:{rect2}", rect2
                changed = True
                print(f"size {smaller} ({tries} tries)", flush=True)
                break
    if spacing != "0" and fails(join(f"{d}:{flex}:0:{rect}", items)):
        head = f"{d}:{flex}:0:{rect}"
        changed = True

print(f"{len(items)} constraints after {tries} tries in {time.time() - start:.0f} s")
print(join(head, items))
