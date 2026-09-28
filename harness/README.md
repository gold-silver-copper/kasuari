# kasuari solver harness

Tools for checking that changes to kasuari are correct and faster: comparing two versions side by
side, an exact reference solver, Ratatui's layouts including every known hard case, random search,
and benchmarks that work on a loaded machine. `NOTES.md` has what's been found so far, including the
approaches that didn't work.

This directory is its own set of crates; it isn't part of the kasuari crate or its CI.

## Setup

```sh
./setup.sh OLD NEW          # e.g. ./setup.sh fix/deterministic-pivoting fix/harris-ratio-test
```

checks out the two versions to compare as git worktrees in `worktrees/` (any commit, branch or
tag), plus `worktrees/record`, a copy of NEW that prints every constraint it's given. The scripts
build what they need; rebuild after running `setup.sh` again. Needs Rust, Python 3, and network
access the first time (Ratatui is a git dependency at a fixed commit).

## Workflows

**A change that mustn't change results** (performance):

```sh
./scripts/compare.sh identical     # every recorded layout and 20,000 random operation sequences, bit for bit
./scripts/bench.sh typical 40      # also: random, extreme, big, large, cols, hard
```

**A change that is allowed to change results** (correctness, robustness):

```sh
./scripts/compare.sh different     # outcome disagreements judged exactly, objectives, exact optimum, hard cases
./scripts/hard.sh new              # just the hard cases, against their expectations
```

**Looking for new failures**:

```sh
./scripts/search.sh new realistic 60 2000       # kinds: realistic, extreme, big (with Fill(0)), bignz
./scripts/minimize.py new 'SPEC' panic          # or invalid, or timeout:SECONDS
layouts/new/target/release/layouts-new case 'SPEC'
```

A failure found this way is worth adding to `HARD` in `layouts/src/main.rs` and recording into
`corpus/hard/` with `scripts/record.sh named`.

## The tools

| Tool | What it does |
| --- | --- |
| `solver/` (`solver-harness`) | Runs `old` and `new` on the same inputs: `replay` (recorded systems, bit for bit), `random` (random sequences of every public operation, bit for bit), `outcomes` (the same sequences, comparing only success and failure, and dumping disagreements), `objectives`, `values`. See the top of `src/main.rs`. |
| `reference/exact.py` | Solves the same linear program as kasuari exactly, with fractions: `feasible`, `optimum`, and `check` (a version's solution against the exact optimum). Slow above a few hundred constraints (about 1 minute for 650). |
| `layouts/` (`layouts-old`, `layouts-new`, `layouts-record`) | Ratatui layouts with one version: `case` (one layout, with CPU time and validity), `hard` (the hard cases), `bench`, `outputs`, `record`, `search`. One source, built against each worktree. |
| `scripts/` | `compare.sh`, `hard.sh`, `bench.sh`, `search.sh`, `record.sh`, `minimize.py` |
| `corpus/` | Recorded constraint systems (see `corpus/README.md`) |

Layouts are written as specs like `v:Start:0:0,0,30,72:Min(3)*24` (direction, flex, spacing, area,
constraints; see `Case::parse`).

## Measuring

- Speed is per-thread CPU time (`clock_gettime(CLOCK_THREAD_CPUTIME_ID)`, with `getrusage` for the
  whole process), and `bench.sh` compares the two versions in the same round, alternating which runs
  first. Ratios between versions are reliable on a busy machine; absolute times aren't.
- Hangs are detected with timeouts on separate processes. `search.sh` and the `case` command print
  what they're about to solve, so a hang can be identified.
- The `release` profile keeps debug assertions and overflow checks on; `bench.sh` uses the `perf`
  profile, without them.
- `minimize.py` runs a process per try; with a version that is slow on the variants (like #1 on
  `Fill(0)` layouts) it takes hours.
