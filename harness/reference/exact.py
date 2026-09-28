#!/usr/bin/env python3
"""Exact reference solver for kasuari's constraint systems, using rational arithmetic.

kasuari minimizes the sum, over the non-required constraints, of strength times violation, subject
to the required constraints. This solves the same linear program exactly with the simplex method on
fractions, so there is no rounding: the answers are ground truth for checking kasuari.

Commands:

  exact.py feasible FILE...
      Whether the required constraints of each system can be satisfied. For files dumped by
      `solver-harness outcomes --dump`, also says which version's outcome was right: adding the
      last constraint must succeed if the required constraints are satisfiable (or it isn't
      required), and fail with `UnsatisfiableConstraint` otherwise.

  exact.py optimum FILE...
      The exact optimal objective of each system.

  exact.py check VALUES FILE...
      Compares the solutions in VALUES (from `solver-harness values old|new FILE...`) with the exact
      optimum: the objective gap, and the largest violation of a required constraint.

Numbers are read as the exact binary value of the f64 they denote, which is what kasuari sees.
Slow for systems with more than a few hundred constraints.
"""

import sys
from fractions import Fraction

REQUIRED = Fraction(1_001_001_000)
OPERATORS = {"==": 0, ">=": 1, "<=": 2}


def number(text):
    return Fraction(float(text))


def parse(path):
    """Returns [(name, comments, constraints)], each constraint (op, strength, constant, terms)."""
    systems = []
    comments = []
    ids = {}
    for line in open(path):
        line = line.strip()
        if line.startswith("## "):
            systems.append([line[3:].strip(), [], []])
            ids = {}
            continue
        if line.startswith("#") or not line:
            if line.startswith("#"):
                (systems[-1][1] if systems else comments).append(line[1:].strip())
            continue
        if not systems:
            systems.append([path, comments, []])
        fields = line.split()
        terms = []
        for term in fields[3:]:
            var, coefficient = term.split(":")
            terms.append((ids.setdefault(var, len(ids)), number(coefficient)))
        systems[-1][2].append((OPERATORS[fields[0]], number(fields[1]), number(fields[2]), terms))
    return [tuple(s) for s in systems]


class Simplex:
    """Minimizes `costs . x` subject to `rows[i] . x == rhs[i]` and `x >= 0`, exactly."""

    def __init__(self):
        self.rows = []  # sparse rows: {column: coefficient}
        self.rhs = []
        self.costs = {}
        self.columns = 0

    def column(self, cost=0):
        c = self.columns
        self.columns += 1
        if cost:
            self.costs[c] = Fraction(cost)
        return c

    def add_row(self, row, rhs):
        row = {c: v for c, v in row.items() if v != 0}
        self.rows.append(row)
        self.rhs.append(Fraction(rhs))

    def solve(self):
        """Returns the optimal objective, or None if infeasible."""
        rows, rhs = self.rows, self.rhs
        for i in range(len(rows)):
            if rhs[i] < 0:
                rows[i] = {c: -v for c, v in rows[i].items()}
                rhs[i] = -rhs[i]
        first_artificial = self.columns
        self.basis = []
        for i in range(len(rows)):
            a = self.column()
            rows[i][a] = Fraction(1)
            self.basis.append(a)
        self.index = {}
        for i, row in enumerate(rows):
            for c in row:
                self.index.setdefault(c, set()).add(i)

        # phase 1: minimize the sum of the artificial variables
        self.reduced = {}
        self.value = sum(rhs, Fraction(0))
        for row in rows:
            for c, v in row.items():
                if c < first_artificial:
                    self.reduced[c] = self.reduced.get(c, 0) - v
        self.reduced = {c: v for c, v in self.reduced.items() if v != 0}
        self.optimize(lambda c: c < first_artificial)
        if self.value != 0:
            return None

        # drive the remaining artificial variables out of the basis, dropping redundant rows
        for i in range(len(rows)):
            if self.basis[i] is not None and self.basis[i] >= first_artificial:
                other = next((c for c in rows[i] if c < first_artificial), None)
                if other is None:
                    for c in rows[i]:
                        self.index[c].discard(i)
                    rows[i], rhs[i], self.basis[i] = {}, Fraction(0), None
                else:
                    self.pivot(i, other)
        for c in range(first_artificial, self.columns):
            for i in self.index.pop(c, ()):
                del rows[i][c]

        # phase 2: minimize the costs
        self.reduced = dict(self.costs)
        self.value = Fraction(0)
        for i, b in enumerate(self.basis):
            cost = self.costs.get(b, 0) if b is not None else 0
            if cost:
                self.value += cost * rhs[i]
                for c, v in rows[i].items():
                    self.reduced[c] = self.reduced.get(c, 0) - cost * v
        self.reduced = {c: v for c, v in self.reduced.items() if v != 0}
        self.optimize(lambda c: c < first_artificial)
        return self.value

    def optimize(self, allowed):
        # the most negative reduced cost, or the lowest column (Bland's rule, which can't cycle)
        # after 50 pivots in a row that don't decrease the objective
        degenerate = 0
        while True:
            candidates = [(v, c) for c, v in self.reduced.items() if v < 0 and allowed(c)]
            if not candidates:
                return
            entering = min(candidates)[1] if degenerate < 50 else min(c for _, c in candidates)
            leaving = None
            for i in self.index.get(entering, ()):
                a = self.rows[i][entering]
                if a > 0:
                    key = (self.rhs[i] / a, self.basis[i])
                    if leaving is None or key < leaving[0]:
                        leaving = (key, i)
            if leaving is None:
                raise RuntimeError("unbounded")
            before = self.value
            self.pivot(leaving[1], entering)
            degenerate = 0 if self.value < before else degenerate + 1

    def pivot(self, r, entering):
        rows, rhs, index = self.rows, self.rhs, self.index
        row = rows[r]
        a = row[entering]
        if a != 1:
            for c in row:
                row[c] /= a
            rhs[r] /= a
        for i in list(index[entering]):
            if i == r:
                continue
            other = rows[i]
            f = other[entering]
            for c, v in row.items():
                new = other.get(c, 0) - f * v
                if new == 0:
                    if c in other:
                        del other[c]
                        index[c].discard(i)
                else:
                    if c not in other:
                        index.setdefault(c, set()).add(i)
                    other[c] = new
            rhs[i] -= f * rhs[r]
        f = self.reduced.get(entering, 0)
        if f:
            for c, v in row.items():
                new = self.reduced.get(c, 0) - f * v
                if new == 0:
                    self.reduced.pop(c, None)
                else:
                    self.reduced[c] = new
            self.value += f * rhs[r]
        self.basis[r] = entering


def build(constraints, required_only):
    """The linear program of a system: each variable is split into two non-negative columns, and
    each non-required constraint gets error columns weighted by its strength."""
    lp = Simplex()
    variables = {}

    def var(v):
        if v not in variables:
            variables[v] = (lp.column(), lp.column())
        return variables[v]

    for op, strength, constant, terms in constraints:
        required = strength >= REQUIRED
        if required_only and not required:
            continue
        row = {}
        for v, coefficient in terms:
            plus, minus = var(v)
            row[plus] = row.get(plus, 0) + coefficient
            row[minus] = row.get(minus, 0) - coefficient
        # sum(terms) + constant (op) 0, as sum(terms) + errors - slack == -constant
        if required:
            if op != 0:
                row[lp.column()] = Fraction(-1 if op == 1 else 1)
        elif op == 0:
            row[lp.column(strength)] = Fraction(-1)
            row[lp.column(strength)] = Fraction(1)
        else:
            sign = 1 if op == 1 else -1
            row[lp.column(strength)] = Fraction(sign)
            row[lp.column()] = Fraction(-sign)
        lp.add_row(row, -constant)
    return lp


def objective(constraints, values):
    """kasuari's objective for the given values, and the largest required violation."""
    total, worst = Fraction(0), Fraction(0)
    for op, strength, constant, terms in constraints:
        lhs = constant + sum(c * values[v] for v, c in terms)
        violation = abs(lhs) if op == 0 else max(-lhs, 0) if op == 1 else max(lhs, 0)
        if strength >= REQUIRED:
            worst = max(worst, violation)
        else:
            total += strength * violation
    return total, worst


def outcome(comments, version):
    line = next((c for c in comments if c.startswith(version + ":")), None)
    return line.split(":", 1)[1].strip() if line else None


def main():
    command, args = sys.argv[1], sys.argv[2:]
    if command == "feasible":
        right = {"old": 0, "new": 0}
        judged = 0
        for path in args:
            for name, comments, constraints in parse(path):
                feasible = build(constraints, required_only=True).solve() is not None
                old, new = outcome(comments, "old"), outcome(comments, "new")
                line = f"{name}: required constraints {'satisfiable' if feasible else 'UNSATISFIABLE'}"
                if old is not None:
                    judged += 1
                    last_required = constraints[-1][1] >= REQUIRED
                    expected = "Ok" if feasible or not last_required else "UnsatisfiableConstraint"
                    for version, got in (("old", old), ("new", new)):
                        right[version] += got == expected
                    line += f"; expected {expected}, old {old}, new {new}"
                print(line)
        if judged:
            print(f"SUMMARY right: old {right['old']} of {judged}, new {right['new']} of {judged}")
    elif command == "optimum":
        for path in args:
            for name, _, constraints in parse(path):
                value = build(constraints, required_only=False).solve()
                print(f"{name}: " + ("infeasible" if value is None else f"{float(value)!r} ({value})"))
    elif command == "check":
        solutions = {}
        name = None
        for line in open(args[0]):
            line = line.strip()
            if line.startswith("## "):
                name = line[3:]
            elif line:
                solutions[name] = line if line.startswith(("FAILED", "PANIC")) else [number(x) for x in line.split()]
        worst_gap = 0.0
        for path in args[1:]:
            for name, _, constraints in parse(path):
                values = solutions.get(name)
                optimum = build(constraints, required_only=False).solve()
                if not isinstance(values, list):
                    failure = values or "missing"
                    print(f"{name}: no solution ({failure}); optimum {'infeasible' if optimum is None else float(optimum)!r}")
                    continue
                total, violation = objective(constraints, values)
                if optimum is None:
                    print(f"{name}: infeasible, but got a solution violating a required constraint by {float(violation):.3g}")
                    continue
                gap = float((total - optimum) / max(abs(optimum), 1))
                worst_gap = max(worst_gap, gap)
                print(f"{name}: objective {float(total)!r}, optimum {float(optimum)!r}, relative gap {gap:+.3e}, worst required violation {float(violation):.3g}")
        print(f"SUMMARY largest relative gap {worst_gap:.3e}")
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
