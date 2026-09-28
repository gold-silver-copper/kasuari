# Constraint systems

## Format

One constraint per line, in the order they were added to the solver:

```text
OPERATOR STRENGTH CONSTANT VARIABLE:COEFFICIENT...
```

meaning `CONSTANT + sum(COEFFICIENT * VARIABLE) OPERATOR 0` with that strength, where OPERATOR is
`==`, `>=` or `<=`, and `1001001000.0` is `Strength::REQUIRED`. Numbers are Rust's shortest
round-trip formatting of the `f64` values, and are read back as exactly the same `f64`.

A line `## NAME` starts a new system in the same file; lines starting with `#` are comments. The
variable ids are arbitrary (recordings use the solver's global ids): the tools renumber them from 0
in each system, in order of appearance.

This is also the format of kasuari's `tests/layouts` (on the PR branches), except that the tools
there don't renumber.

## Contents

- `hard/*.txt`: the named hard cases of `layouts-new hard` whose systems are small enough (at most
  about 700 constraints) for `reference/exact.py`, recorded with `scripts/record.sh named`. Each file
  starts with the layout's spec.
- `hard/large/*.txt`: the larger ones (`[Min(1); 100]`, 30-40 dense columns), for `replay` and
  `objectives`.
- `hard/satisfiability-disagreements.txt`: 22 sets of required constraints on which two versions
  disagreed about satisfiability in random operation sequences (see NOTES.md, problem 2). 14 are
  satisfiable and 8 aren't (`reference/exact.py feasible`).
- `generated/` (not committed, about 20 MB): the sets of `layouts-new sets`, recorded by
  `scripts/record.sh SET`. They're generated from fixed seeds, so they're the same every time for
  the same Ratatui commit.
