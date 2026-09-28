//! The solver stores its tableau in hash maps whose iteration order is random. These tests check
//! that the solution doesn't depend on that order.
//!
//! The constraints in `tests/layouts` were recorded from the layouts that Ratatui builds for the
//! cases described at the top of each file, and are replayed here in the same order.

use kasuari::RelationalOperator::{self, Equal, GreaterOrEqual, LessOrEqual};
use kasuari::{Constraint, Expression, Solver, Strength, Term, Variable};

/// A constraint as (operator, strength, constant, [(variable index, coefficient)])
type RecordedConstraint = (RelationalOperator, f64, f64, Vec<(usize, f64)>);

/// Parses a file of recorded constraints, one per line: `operator strength constant
/// variable:coefficient...`. Lines starting with `#` are ignored.
fn parse(layout: &str) -> Vec<RecordedConstraint> {
    layout
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let mut fields = line.split(' ');
            let op = match fields.next().unwrap() {
                "==" => Equal,
                ">=" => GreaterOrEqual,
                "<=" => LessOrEqual,
                op => panic!("unknown operator {op}"),
            };
            let strength = fields.next().unwrap().parse().unwrap();
            let constant = fields.next().unwrap().parse().unwrap();
            let terms = fields
                .map(|term| {
                    let (index, coefficient) = term.split_once(':').unwrap();
                    (index.parse().unwrap(), coefficient.parse().unwrap())
                })
                .collect();
            (op, strength, constant, terms)
        })
        .collect()
}

/// Solves the constraints with a new solver, returning the values of the variables.
fn solve(constraints: &[RecordedConstraint]) -> Vec<f64> {
    let variable_count = constraints
        .iter()
        .flat_map(|(_, _, _, terms)| terms.iter().map(|(index, _)| index + 1))
        .max()
        .unwrap_or(0);
    let variables: Vec<Variable> = (0..variable_count).map(|_| Variable::new()).collect();
    let mut solver = Solver::new();
    for (op, strength, constant, terms) in constraints {
        let terms = terms
            .iter()
            .map(|&(index, coefficient)| Term::new(variables[index], coefficient))
            .collect();
        let expression = Expression::new(terms, *constant);
        solver
            .add_constraint(Constraint::new(expression, *op, Strength::new(*strength)))
            .unwrap();
    }
    variables.iter().map(|&v| solver.get_value(v)).collect()
}

/// Asserts that solving the constraints repeatedly always gives the same solution.
fn assert_deterministic(layout: &str, runs: usize) {
    let constraints = parse(layout);
    let first = solve(&constraints);
    for _ in 1..runs {
        assert_eq!(solve(&constraints), first);
    }
}

/// When constraints conflict and several solutions are equally good, the solver must always pick
/// the same one.
#[test]
fn conflicting_constraints_have_a_deterministic_solution() {
    assert_deterministic(include_str!("layouts/header_and_footer.txt"), 100);
}

/// These used to occasionally take minutes, fail with `ObjectiveUnbounded`, or give a different
/// solution, depending on the order in which the solver visited its rows.
#[test]
fn realistic_values_are_solved_consistently() {
    assert_deterministic(include_str!("layouts/realistic_values.txt"), 100);
}

/// Constraint values far larger than the available space made failures more likely.
#[test]
fn extreme_values_are_solved_consistently() {
    assert_deterministic(include_str!("layouts/extreme_values.txt"), 100);
}

/// Many pivots in a row that don't decrease the objective must not make the solver cycle.
#[test]
fn degenerate_pivots_do_not_cycle() {
    solve(&parse(include_str!("layouts/degenerate.txt")));
}
