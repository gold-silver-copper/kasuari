# Notes on kasuari's correctness and performance

What was found while fixing kasuari's hangs and nondeterminism (gold-silver-copper/kasuari PRs #1,
#2 and #3), including what was tried and didn't work. Numbers are per-thread CPU time, measured
on a loaded machine with paired rounds (see `scripts/bench.sh`), so ratios are more reliable than
absolute times.

## State of the branches

| Branch | PR | Changes results? | Status |
| --- | --- | --- | --- |
| `fix/deterministic-pivoting` | #1 | yes | pivot choices independent of hash order; Bland fallback after 200 degenerate pivots |
| `perf/solver` | #2, on #1 | no (bit-identical) | fixed hasher: about 20% faster |
| `fix/harris-ratio-test` | #3, on #1 (draft) | yes | Harris' ratio test: fixes the 40-column table and `Fill(0)` failures |

The harness's hard cases (`layouts-new hard`) record what `fix/harris-ratio-test` does.

## Why the solver hung and gave random results

The solver stores its tableau in `hashbrown` maps, whose default hasher is seeded randomly for every
map. Every pivot choice took the first match in iteration order, so each call took a different path
through the simplex method:

- with several equally good solutions, a different one could come back on each call (finding A)
- the number of pivots varied wildly, and some paths cycled: calls hung or took minutes
- rounding errors varied, so some calls failed with `ObjectiveUnbounded` or returned wrong values

It wasn't slowness: `[Min(3); 24]` in 72 rows hung in 49 of 50 processes on 0.4.12, and takes about
5 ms with #1. #1 chooses by value (most negative reduced cost, smallest ratio), with the lowest
symbol id breaking ties.

Results of #1 on the related reports (0.4.12 from crates.io vs #1, fresh hash seeds per process):

| Report | 0.4.12 | #1 |
| --- | --- | --- |
| `ratatui/ratatui#1855`, `[Min(3); 24]` in 72 rows, 10 calls x 50 processes | 49 of 50 processes hung, 1 panic | 500 of 500, one result |
| same, `[Min(3); 16]` | 27 of 50 hung | 500 of 500 |
| `[Min(1); N]` in N rows, N = 25-200 | hangs at every N, some panics | all finish, one result each |
| `ratatui/kasuari#18`, 20 `Min(m)` columns, `SpaceAround`, every width 0-300 | 9-11 of 12 processes hung | 12 of 12, one result |
| finding A (header and footer that don't fit) | two results, about 50/50 | one result |
| finding B (extreme column values) | all 20 processes hung | one result |
| `regression_18` from `ratatui/kasuari#24` | didn't hang here, 9.8 s | 4.9 s |

`[Min(1); 1000]` still doesn't finish (see "Ratatui's constraints" below).

## What was tried and didn't work

For PR #1 (making it deterministic without hangs or panics):

- **Bland's rule alone** (always the lowest id entering): never cycles, but about 25% slower on
  typical layouts, and the `bland-slow` hard case takes about 4 s and then fails with
  `ObjectiveUnbounded` (0.24 s with the most negative reduced cost).
- **The most negative reduced cost alone**: cycles forever on the `degenerate` hard case: the
  objective stays exactly the same while the same pivots repeat every ~30,000 steps.
- **The hybrid** (most negative, Bland after N degenerate pivots in a row): N = 10 fails
  `bland-slow` with `ObjectiveUnbounded`; 50 and 200 both work; 200 was chosen.
- **Relative tolerance for cancellation in `Row::insert_symbol`** (drop a sum within 1e-12 of its
  terms): a 60-constraint layout still failed with `ObjectiveUnbounded`. The objective kept a
  coefficient of -1.74e-8, left over from cancelling terms around 1.2e9.
- **Treating "no leaving row" as rounding and skipping that entering symbol**: fixed that failure but
  made 60-constraint layouts 190-350 ms each instead of about 15 ms, and typical layouts 3x slower.
- **Dropping objective coefficients below 1e-9 of the largest when no leaving row is found**: the
  next failure had a coefficient of -9.3e4 against 2.7e13, above any reasonable threshold.
- **`Vec` instead of `BTreeSet` for the infeasible rows**: no difference in speed (ratio 1.002).

For PR #3 (the 40-column table: invalid after about 90 s with #1):

- Diagnosis: 201 pivots divided by a coefficient below 1e-6 of its row's scale (several around
  3e-8); after constraint 282 the objective went negative (-5.1e-4), which is impossible; the Bland
  fallback took over for 25 pivots, one of them pivoting on 1e-8 with a reduced cost of -1.9e-8.
- **Ignoring reduced costs within a relative tolerance** (1e-15 to 1e-10) when choosing the entering
  symbol: 40 columns still timed out at every tolerance; 1e-12 and larger made 30 and 36 columns time
  out too; 1e-10 made 30 columns invalid.
- **Counting objective decreases within 1e-12 as degenerate**: no change.
- **Harris' ratio test** (tolerance 1e-9, largest pivot among the rows within the relaxed bound):
  works. 1e-7 gave the same results. This is PR #3.
- **Harris plus setting slightly negative row constants back to 0**: no meaningful difference
  (1 of 2,000 extreme layouts differed; 172 vs 164 outcome disagreements with #1), so left out.

For PR #2 (performance):

- Profile (`sample` on macOS): `Row::insert_symbol` 34-52% and `Row::substitute` 14-26% on typical
  and random layouts; `Row::substitute` 82% on `[Min(1); 200]`, because every pivot calls it on every
  row, mostly to find the row doesn't contain the entering symbol.
- **Fixed hasher** (`IdHasher`, a folded multiply; `Symbol` hashes only its id): bit-identical,
  about 0.80x the time on typical, random and larger layouts, 0.65x on `[Min(1); 200]`. This is PR #2.
- **Column index** (which rows contain each symbol, so `substitute` and the ratio test only visit
  those): bit-identical, and 4.9x faster than #1 on `[Min(1); 200]` (1.25 s vs 6.15 s; 4.48 s with
  the hasher only), but 1.31x the hasher-only time on typical layouts, 1.11x #1's time on random
  layouts, and slower on dense tables (10-30 `Min` columns) at every size, as their rows are dense.
  It needs to switch on only for large sparse systems. The code (`src/tableau.rs`, and changes to
  `row.rs` and `solver.rs`) is in the session transcript; it was lost from disk.

## Known bugs and open problems

1. **`Solver::add_edit_variable` panics.** It calls `self.add_constraint(cn.clone()).unwrap()`,
   which can fail with `ObjectiveUnbounded`. Random operation sequences hit it in 44-54 of 200 runs
   on 0.4.12 and 37 of 200 with #1. Not fixed anywhere.
2. **Wrong answers after removals and edits.** On the 22 required-constraint systems of
   `corpus/hard/satisfiability-disagreements.txt`, #1 and a Harris variant each answered "can these
   be satisfied?" correctly only 11 times in the operation sequences where they came up. Solved from
   scratch (just adding the constraints), both answer all 22 correctly. With #1 vs #3's current
   disagreements: with small integers, both are right 18 of 18 from scratch but 10 and 8 of 18 in the
   sequences. So the errors come from the incremental paths (`remove_constraint`,
   `get_marker_leaving_row`, edit variables, `suggest_value` and `dual_optimize`), not the core solve.
3. **Numerical robustness with extreme values.** With values up to 10,000 and random strengths up to
   1e9, #1 and #3 are each right about satisfiability in only 9 of 22 disagreement systems even from
   scratch, with 6 internal errors each. `near_zero` is an absolute 1e-8, while Ratatui's coefficients
   reach about 1e9 (strengths) times 100 (its precision multiplier). Ratatui's layouts didn't hit this
   in any search (about 240,000 random layouts clean with #3: 120,000 realistic, 120,000 extreme and 600 with 60 constraints).
4. **`Fill(0)`** is encoded by Ratatui as a weight of 1e-6 (gold-silver-copper/ratatui#4). With #1 the
   `fill0` hard case fails with `ObjectiveUnbounded` and `fill0-overlap` returns overlapping segments;
   #3 solves both correctly and optimally. Changing 1e-6 to 1e-3 in Ratatui also fixes `fill0`; 1e-2
   breaks Ratatui's `case_09_zero_fill1` test.
5. **Layout spacing outside the `i16` range** panics or has the wrong sign in Ratatui
   (gold-silver-copper/ratatui#5). Not a solver problem.
6. **Dense layouts are slow in every version**: 40 `Min(3)` columns with `SpaceAround` take about
   2-4 s with #3, 44 about 50 s, 60 about 2 minutes, `[Min(1); 200]` about 8 s. See below.

## Ratatui's constraints

Outside `Flex::Legacy`, Ratatui's `configure_fill_constraints` adds an equality for every pair of
`Fill` and `Min` segments, so N segments give about N²/2 constraints: 40 columns about 1,900, 200
segments about 21,700, 1,000 about 500,000. The pivot count stays around 1-5 per constraint, but each
pivot touches every row, so time grows much faster than N². No solver change fixes
`[Min(1); 1000]`; Ratatui needs fewer constraints (e.g. relating each segment to one reference
segment or its neighbour instead of every other one, which has to be checked for giving the same
layouts when constraints conflict). `scripts/record.sh` and `solver-harness objectives` can compare
the systems before and after such a change.

## Directions for a more correct and faster solver

In rough order of value:

1. Fix the incremental paths (problem 2): reproduce with `solver-harness outcomes --mild --dump`,
   judge with `reference/exact.py feasible`, and compare with solving from scratch.
2. Fix `add_edit_variable`'s `unwrap` (problem 1).
3. Scale-aware tolerances instead of the absolute `near_zero` (problem 3), checked against
   `reference/exact.py check` on the recorded systems.
4. Fewer constraints from Ratatui for `Fill`/`Min` (see above).
5. A cheaper row representation (`insert_symbol` dominates the profile) and an adaptive column index.

## Other upstream context

- `ratatui/kasuari#24` (draft, from July 2025) started choosing the lowest id for the entering
  symbol and leaving row, and has the `regression_18` test. A maintainer suggested keeping symbols in
  id order there, which is what #1 does.
- A commenter on `ratatui/ratatui#1855` made the hang go away with a fixed hasher for the rows map
  and a custom `Hash` for `Symbol`, without knowing why: it fixed the iteration order.
- Nothing has been posted upstream; everything is in gold-silver-copper's forks.

## Latest comparison: #1 (old) vs #3 (new)

`scripts/compare.sh different` with `./setup.sh fix/deterministic-pivoting fix/harris-ratio-test`
(the final hard-case step was stopped; `scripts/hard.sh` results for both are above and in PR #3):

- Random operation sequences: 164 disagreements in 661,373 operations (extreme values), 53 in
  747,676 (small integers). Judged exactly: extreme, #1 right 9 of 22 and #3 right 11 of 22; small
  integers, #1 right 10 of 18 and #3 right 8 of 18 (see problem 2).
- Recorded systems, where values differ: typical 33 equal objective / 70 with #1 lower; random 562
  equal / 5 #3 lower / 1 #1 lower; extreme 531 equal / 1 #1 lower; big 37 equal; large 8 equal / 23
  #1 lower; hard 448 equal / 2 #3 lower / 760 #1 lower / 1 failed (fill0, with #1). "Lower" beyond 1e-9
  is at most about 1.8e-6 relative, with the same rendered output.
- Exact optimum on `corpus/hard`: #3's largest relative gap is 3.2e-12 on every system it solves,
  including `fill0` and `fill0-overlap`. #1's is 0.23 on `fill0-overlap` (a required constraint
  violated by 1.95e4, i.e. an invalid layout), and it fails `fill0` with `ObjectiveUnbounded`.
- Both answer all 22 `satisfiability-disagreements` correctly when solving from scratch.
