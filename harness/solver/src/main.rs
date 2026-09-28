//! Runs two versions of kasuari (`old` and `new`, see `../setup.sh`) on the same inputs.
//!
//! Commands:
//!
//! - `replay FILE...`: solves every constraint system in the files with both versions, and requires
//!   bit-identical results: the result of every `add_constraint`, every variable's value and the
//!   changes from `fetch_changes`. For performance changes that mustn't change results.
//! - `random FIRST LAST [--mild]`: random sequences of every public operation (adding and removing
//!   constraints and edit variables, `suggest_value`), one per seed, compared bit for bit after
//!   every operation.
//! - `outcomes FIRST LAST [--mild] [--dump DIR]`: the same sequences, but only compares whether
//!   each operation succeeds, fails (and how) or panics. For changes that are allowed to change
//!   results. A sequence stops at its first disagreement (the solvers' states differ after it) or
//!   panic. With `--dump`, writes the constraints in the solver at every disagreement to DIR, to be
//!   checked with `../reference/exact.py feasible`. Edit variables aren't included: they are never
//!   required, so this is exact for whether the required constraints can be satisfied, but not for
//!   the objective.
//! - `objectives FILE...`: for every system where the two versions' values differ, compares the
//!   objective (the strength-weighted violation of the non-required constraints).
//! - `values old|new FILE...`: prints each version's values, for `../reference/exact.py check`.
//!
//! Constraint systems use the format of `../corpus/README.md`. `--mild` uses small integers and the
//! four standard strengths only; without it, values range up to 10,000 with random strengths.
//!
//! Exits with status 1 on the first difference in `replay` and `random`.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::exit;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;

/// (operator: 0 `==`, 1 `>=`, 2 `<=`; strength; constant; terms as (variable, coefficient))
type Spec = (u8, f64, f64, Vec<(usize, f64)>);

trait Api {
    type Cons: Clone;
    fn new(vars: usize) -> Self;
    fn add(&mut self, spec: &Spec) -> (String, Option<Self::Cons>);
    fn remove(&mut self, c: &Self::Cons) -> String;
    fn add_edit(&mut self, var: usize, strength: f64) -> String;
    fn remove_edit(&mut self, var: usize) -> String;
    fn suggest(&mut self, var: usize, value: f64) -> String;
    fn values(&self) -> Vec<f64>;
    /// Changes since the last call, as (variable index, value bits), sorted
    fn changes(&mut self) -> Vec<(usize, u64)>;
}

macro_rules! api {
    ($name:ident, $krate:ident) => {
        struct $name {
            solver: $krate::Solver,
            vars: Vec<$krate::Variable>,
        }
        impl Api for $name {
            type Cons = $krate::Constraint;
            fn new(vars: usize) -> Self {
                Self {
                    solver: $krate::Solver::new(),
                    vars: (0..vars).map(|_| $krate::Variable::new()).collect(),
                }
            }
            fn add(
                &mut self,
                (op, strength, constant, terms): &Spec,
            ) -> (String, Option<Self::Cons>) {
                use $krate::RelationalOperator::*;
                let op = [Equal, GreaterOrEqual, LessOrEqual][*op as usize];
                let terms = terms
                    .iter()
                    .map(|&(v, c)| $krate::Term::new(self.vars[v], c))
                    .collect();
                let expression = $krate::Expression::new(terms, *constant);
                let c = $krate::Constraint::new(expression, op, $krate::Strength::new(*strength));
                match self.solver.add_constraint(c.clone()) {
                    Ok(()) => ("Ok".into(), Some(c)),
                    Err(e) => (format!("{e:?}"), None),
                }
            }
            fn remove(&mut self, c: &Self::Cons) -> String {
                format!("{:?}", self.solver.remove_constraint(c))
            }
            fn add_edit(&mut self, var: usize, strength: f64) -> String {
                let strength = $krate::Strength::new(strength);
                format!(
                    "{:?}",
                    self.solver.add_edit_variable(self.vars[var], strength)
                )
            }
            fn remove_edit(&mut self, var: usize) -> String {
                format!("{:?}", self.solver.remove_edit_variable(self.vars[var]))
            }
            fn suggest(&mut self, var: usize, value: f64) -> String {
                format!("{:?}", self.solver.suggest_value(self.vars[var], value))
            }
            fn values(&self) -> Vec<f64> {
                self.vars
                    .iter()
                    .map(|&v| self.solver.get_value(v))
                    .collect()
            }
            fn changes(&mut self) -> Vec<(usize, u64)> {
                let vars = self.vars.clone();
                let mut changes: Vec<(usize, u64)> = self
                    .solver
                    .fetch_changes()
                    .iter()
                    .map(|(v, value)| (vars.iter().position(|x| x == v).unwrap(), value.to_bits()))
                    .collect();
                changes.sort();
                changes
            }
        }
    };
}
api!(Old, old);
api!(New, new);

const REQUIRED: f64 = 1_001_001_000.0;
const STRENGTHS: [f64; 4] = [REQUIRED, 1_000_000.0, 1_000.0, 1.0];

static OPERATIONS: AtomicUsize = AtomicUsize::new(0);
static PANICS: AtomicUsize = AtomicUsize::new(0);

// --- constraint system files -------------------------------------------------------------------

/// Parses constraint system files (see `../corpus/README.md`) into named systems. The variables
/// are renumbered from 0 in each system, in order of appearance.
fn parse(file: &str) -> Vec<(String, Vec<Spec>)> {
    let text = std::fs::read_to_string(file).unwrap_or_else(|e| panic!("{file}: {e}"));
    let mut systems: Vec<(String, Vec<Spec>)> = vec![];
    let mut ids = std::collections::HashMap::<usize, usize>::new();
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("## ") {
            systems.push((name.trim().to_string(), vec![]));
            ids.clear();
            continue;
        }
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if systems.is_empty() {
            systems.push((file.to_string(), vec![]));
        }
        let mut fields = line.split_whitespace();
        let op = match fields.next().unwrap() {
            "==" => 0,
            ">=" => 1,
            "<=" => 2,
            op => panic!("{file}: unknown operator {op}"),
        };
        let strength = fields.next().unwrap().parse().unwrap();
        let constant = fields.next().unwrap().parse().unwrap();
        let terms = fields
            .map(|t| {
                let (v, c) = t.split_once(':').unwrap();
                let next = ids.len();
                (
                    *ids.entry(v.parse().unwrap()).or_insert(next),
                    c.parse().unwrap(),
                )
            })
            .collect();
        systems
            .last_mut()
            .unwrap()
            .1
            .push((op, strength, constant, terms));
    }
    systems
}

fn var_count(specs: &[Spec]) -> usize {
    specs
        .iter()
        .flat_map(|s| s.3.iter().map(|t| t.0 + 1))
        .max()
        .unwrap_or(0)
}

fn format_spec((op, strength, constant, terms): &Spec) -> String {
    let op = ["==", ">=", "<="][*op as usize];
    let terms: Vec<String> = terms.iter().map(|(v, c)| format!("{v}:{c:?}")).collect();
    format!("{op} {strength:?} {constant:?} {}", terms.join(" "))
}

// --- running operations on both versions -------------------------------------------------------

fn panic_message(e: Box<dyn std::any::Any + Send>) -> String {
    let message = e
        .downcast_ref::<String>()
        .cloned()
        .or(e.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default();
    // the paths of the two versions differ
    message
        .split("worktrees/")
        .map(|s| s.split_once('/').map_or(s, |(_, rest)| rest))
        .collect()
}

fn run<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(f)).map_err(panic_message)
}

fn mismatch(context: &str, old: impl std::fmt::Debug, new: impl std::fmt::Debug) -> ! {
    println!("MISMATCH {context}\n  old: {old:?}\n  new: {new:?}");
    exit(1)
}

/// Runs an operation on both versions and requires identical results, including identical panics.
/// Returns false if both panicked, so the sequence has to stop.
fn both<T: PartialEq + std::fmt::Debug>(
    context: &str,
    old: impl FnOnce() -> T,
    new: impl FnOnce() -> T,
) -> bool {
    OPERATIONS.fetch_add(1, Relaxed);
    match (run(old), run(new)) {
        (Ok(a), Ok(b)) if a == b => true,
        (Err(a), Err(b)) if a == b => {
            PANICS.fetch_add(1, Relaxed);
            false
        }
        (a, b) => mismatch(context, a, b),
    }
}

fn compare_state(context: &str, old: &mut Old, new: &mut New) {
    let bits = |v: Vec<f64>| v.into_iter().map(f64::to_bits).collect::<Vec<_>>();
    let (a, b) = (bits(old.values()), bits(new.values()));
    if a != b {
        mismatch(&format!("{context}: values"), old.values(), new.values());
    }
    let (a, b) = (old.changes(), new.changes());
    if a != b {
        mismatch(&format!("{context}: changes"), a, b);
    }
}

fn replay(files: &[String]) {
    for file in files {
        let systems = parse(file);
        let mut constraints = 0;
        for (name, specs) in &systems {
            let n = var_count(specs);
            let (mut old, mut new) = (Old::new(n), New::new(n));
            for (i, spec) in specs.iter().enumerate() {
                let context = format!("{file} {name} constraint {i}");
                if !both(&context, || old.add(spec).0, || new.add(spec).0) {
                    break;
                }
            }
            compare_state(&format!("{file} {name}"), &mut old, &mut new);
            constraints += specs.len();
        }
        println!(
            "OK {file}: {} systems, {constraints} constraints identical",
            systems.len()
        );
    }
}

// --- random operation sequences ----------------------------------------------------------------

enum Op {
    Add(Spec),
    Remove(usize),
    AddEdit(usize, f64),
    RemoveEdit(usize),
    Suggest(usize, f64),
}

/// Generates the operations of a random sequence. `added` is the number of constraints added so
/// far, which the generator needs to pick one to remove.
struct Sequence {
    rng: fastrand::Rng,
    mild: bool,
    vars: usize,
    steps: usize,
}

impl Sequence {
    fn new(seed: u64, mild: bool) -> Self {
        let mut rng = fastrand::Rng::with_seed(seed);
        let vars = rng.usize(1..12);
        let steps = rng.usize(1..150);
        Self {
            rng,
            mild,
            vars,
            steps,
        }
    }

    fn number(&mut self) -> f64 {
        let rng = &mut self.rng;
        if self.mild {
            return f64::from(rng.i32(-5..=5));
        }
        match rng.u8(0..4) {
            0 => f64::from(rng.i32(-5..=5)),
            1 => f64::from(rng.i32(-10000..=10000)),
            2 => (rng.f64() - 0.5) * 200.0,
            _ => f64::from(rng.i32(0..=2)),
        }
    }

    fn next(&mut self, added: usize) -> Op {
        match self.rng.u8(0..20) {
            0..=11 => {
                let terms = (0..self.rng.usize(1..=4))
                    .map(|_| (self.rng.usize(0..self.vars), self.number()))
                    .collect();
                let op = self.rng.u8(0..3);
                let strength = if !self.mild && self.rng.u8(0..5) == 0 {
                    self.rng.f64() * 1e9
                } else {
                    STRENGTHS[self.rng.usize(0..4)]
                };
                let constant = self.number();
                Op::Add((op, strength, constant, terms))
            }
            12..=13 if added > 0 => Op::Remove(self.rng.usize(0..added)),
            14..=15 => Op::AddEdit(
                self.rng.usize(0..self.vars),
                STRENGTHS[self.rng.usize(0..4)],
            ),
            16 => Op::RemoveEdit(self.rng.usize(0..self.vars)),
            _ => {
                let var = self.rng.usize(0..self.vars);
                Op::Suggest(var, self.number())
            }
        }
    }
}

fn random(seed: u64, mild: bool) {
    let mut sequence = Sequence::new(seed, mild);
    let (mut old, mut new) = (Old::new(sequence.vars), New::new(sequence.vars));
    let mut added: Vec<(old::Constraint, new::Constraint)> = vec![];
    for step in 0..sequence.steps {
        let context = format!("seed {seed} step {step}");
        let ok = match sequence.next(added.len()) {
            Op::Add(spec) => {
                let (mut a, mut b) = (None, None);
                let ok = both(
                    &context,
                    || {
                        let (r, c) = old.add(&spec);
                        a = c;
                        r
                    },
                    || {
                        let (r, c) = new.add(&spec);
                        b = c;
                        r
                    },
                );
                if let (Some(a), Some(b)) = (a, b) {
                    added.push((a, b));
                }
                ok
            }
            Op::Remove(i) => {
                let (a, b) = added.swap_remove(i);
                both(&context, || old.remove(&a), || new.remove(&b))
            }
            Op::AddEdit(v, s) => both(&context, || old.add_edit(v, s), || new.add_edit(v, s)),
            Op::RemoveEdit(v) => both(&context, || old.remove_edit(v), || new.remove_edit(v)),
            Op::Suggest(v, x) => both(&context, || old.suggest(v, x), || new.suggest(v, x)),
        };
        if !ok {
            return;
        }
        compare_state(&context, &mut old, &mut new);
    }
}

/// Outcome of one operation: `Ok`, the error, or `PANIC ...`
fn outcome(r: Result<String, String>) -> String {
    r.unwrap_or_else(|p| format!("PANIC {p}"))
}

/// Compares only the outcomes. Returns (operations, disagreements as (step, old, new)).
fn outcomes(seed: u64, mild: bool, dump: Option<&str>) -> (usize, Vec<(usize, String, String)>) {
    let mut sequence = Sequence::new(seed, mild);
    let (mut old, mut new) = (Old::new(sequence.vars), New::new(sequence.vars));
    let mut added: Vec<(old::Constraint, new::Constraint, Spec)> = vec![];
    let mut disagreements = vec![];
    for step in 0..sequence.steps {
        let op = sequence.next(added.len());
        let (a, b, candidate) = match &op {
            Op::Add(spec) => {
                let a = run(|| old.add(spec));
                let b = run(|| new.add(spec));
                let ta = a.as_ref().map(|(s, _)| s.clone()).map_err(Clone::clone);
                let tb = b.as_ref().map(|(s, _)| s.clone()).map_err(Clone::clone);
                if let (Ok((_, Some(ca))), Ok((_, Some(cb)))) = (a, b) {
                    added.push((ca, cb, spec.clone()));
                }
                (ta, tb, Some(spec.clone()))
            }
            Op::Remove(i) => {
                let (ca, cb, _) = added.swap_remove(*i);
                (run(|| old.remove(&ca)), run(|| new.remove(&cb)), None)
            }
            Op::AddEdit(v, s) => (
                run(|| old.add_edit(*v, *s)),
                run(|| new.add_edit(*v, *s)),
                None,
            ),
            Op::RemoveEdit(v) => (
                run(|| old.remove_edit(*v)),
                run(|| new.remove_edit(*v)),
                None,
            ),
            Op::Suggest(v, x) => (
                run(|| old.suggest(*v, *x)),
                run(|| new.suggest(*v, *x)),
                None,
            ),
        };
        let (panicked, (a, b)) = (a.is_err() || b.is_err(), (outcome(a), outcome(b)));
        if a != b {
            if let (Some(dir), Some(new_spec)) = (dump, &candidate) {
                // the constraints in the solver when the operation ran, and the one being added
                let mut lines: Vec<String> = added.iter().map(|x| format_spec(&x.2)).collect();
                if lines.last() != Some(&format_spec(new_spec)) {
                    lines.push(format_spec(new_spec));
                }
                let path = format!("{dir}/seed{seed}-step{step}.txt");
                let header = format!("# seed {seed} step {step}\n# old: {a}\n# new: {b}\n");
                std::fs::write(&path, header + &lines.join("\n") + "\n").unwrap();
            }
            disagreements.push((step, a, b));
            // the two solvers are in different states now, so the rest isn't comparable
            return (step + 1, disagreements);
        }
        if panicked {
            return (step + 1, disagreements);
        }
    }
    (sequence.steps, disagreements)
}

// --- objectives --------------------------------------------------------------------------------

/// The strength-weighted violation of the non-required constraints, and the largest violation of
/// a required constraint.
fn objective(specs: &[Spec], values: &[f64]) -> (f64, f64) {
    let (mut total, mut worst_required) = (0.0, 0.0_f64);
    for (op, strength, constant, terms) in specs {
        let lhs = constant + terms.iter().map(|&(v, c)| c * values[v]).sum::<f64>();
        let violation = match op {
            0 => lhs.abs(),
            1 => (-lhs).max(0.0),
            _ => lhs.max(0.0),
        };
        if *strength >= REQUIRED {
            worst_required = worst_required.max(violation);
        } else {
            total += strength * violation;
        }
    }
    (total, worst_required)
}

/// Adds every constraint, failing on the first error or panic (Ratatui panics on any error).
fn solve<A: Api>(specs: &[Spec]) -> Result<Vec<f64>, String> {
    let mut api = A::new(var_count(specs));
    for (i, spec) in specs.iter().enumerate() {
        match run(|| api.add(spec).0) {
            Ok(r) if r == "Ok" => {}
            Ok(error) => return Err(format!("constraint {i}: {error}")),
            Err(p) => return Err(format!("constraint {i}: panic: {p}")),
        }
    }
    Ok(api.values())
}

fn objectives(files: &[String]) {
    for file in files {
        let (mut same, mut equal, mut new_better, mut old_better, mut failed) = (0, 0, 0, 0, 0);
        for (name, specs) in parse(file) {
            let (a, b) = (solve::<Old>(&specs), solve::<New>(&specs));
            let (Ok(a), Ok(b)) = (&a, &b) else {
                failed += 1;
                println!(
                    "{name}: failed: old {:?}, new {:?}",
                    a.as_ref().err(),
                    b.as_ref().err()
                );
                continue;
            };
            if a.iter()
                .zip(b.iter())
                .all(|(x, y)| x.to_bits() == y.to_bits())
            {
                same += 1;
                continue;
            }
            let (oa, ob) = (objective(&specs, &a), objective(&specs, &b));
            let relative = (ob.0 - oa.0) / oa.0.abs().max(ob.0.abs()).max(f64::MIN_POSITIVE);
            match relative {
                r if r.abs() < 1e-9 => equal += 1,
                r if r < 0.0 => new_better += 1,
                _ => old_better += 1,
            }
            println!(
                "{name}: objective old {:.12e} new {:.12e} (relative {relative:+.2e}), worst required violation old {:.1e} new {:.1e}",
                oa.0, ob.0, oa.1, ob.1
            );
        }
        println!(
            "SUMMARY {file}: {same} identical; differing values: {equal} with equal objective (within 1e-9), {new_better} new lower, {old_better} old lower; {failed} failed in either"
        );
    }
}

fn values(which: &str, files: &[String]) {
    for file in files {
        for (name, specs) in parse(file) {
            let result = match which {
                "old" => solve::<Old>(&specs),
                "new" => solve::<New>(&specs),
                _ => panic!("expected old or new"),
            };
            println!("## {name}");
            match result {
                Ok(v) => println!(
                    "{}",
                    v.iter()
                        .map(|x| format!("{x:?}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                Err(e) => println!("FAILED {e}"),
            }
        }
    }
}

fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let option = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .map(|i| args[i + 1].clone())
    };
    let seeds = || -> (u64, u64) { (args[2].parse().unwrap(), args[3].parse().unwrap()) };
    match args.get(1).map(String::as_str) {
        Some("replay") => replay(&args[2..]),
        Some("objectives") => objectives(&args[2..]),
        Some("values") => values(&args[2], &args[3..]),
        Some("random") => {
            let (first, last) = seeds();
            for seed in first..=last {
                random(seed, flag("--mild"));
            }
            println!(
                "OK random seeds {first}..={last}: {} operations identical, {} sequences ended by an identical panic",
                OPERATIONS.load(Relaxed),
                PANICS.load(Relaxed)
            );
        }
        Some("outcomes") => {
            let (first, last) = seeds();
            let dump = option("--dump");
            if let Some(dir) = &dump {
                std::fs::create_dir_all(dir).unwrap();
            }
            let (mut operations, mut all) = (0, vec![]);
            for seed in first..=last {
                let (n, d) = outcomes(seed, flag("--mild"), dump.as_deref());
                operations += n;
                all.extend(d.into_iter().map(|(step, a, b)| (seed, step, a, b)));
            }
            println!(
                "OUTCOMES seeds {first}..={last}: {operations} operations, {} disagreements",
                all.len()
            );
            for (seed, step, a, b) in all {
                println!("  seed {seed} step {step}: old {a} | new {b}");
            }
        }
        _ => {
            eprintln!("usage: see the comment at the top of src/main.rs");
            exit(2)
        }
    }
}
