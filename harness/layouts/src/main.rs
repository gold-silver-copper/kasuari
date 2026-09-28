//! Ratatui layouts solved with one version of kasuari. The same source is built three times (see
//! `../old`, `../new` and `../record`), once for each worktree of `../../setup.sh`.
//!
//! Commands:
//!
//! - `case CASE [N]`: solves a case N times (default 1) in this process, and prints the CPU time,
//!   the areas, whether they're valid, and whether all N calls agreed. CASE is the name of a hard
//!   case (see `hard`) or a spec (see `Case::parse`).
//! - `hard`: lists the hard cases, as `name<TAB>spec<TAB>time limit<TAB>expectation<TAB>note`.
//! - `bench SET`: solves every layout of a set once, and prints the per-thread and per-process CPU
//!   time per layout.
//! - `outputs SET`: prints the areas of every layout of a set, one per line.
//! - `record SET`: solves every layout of a set, printing `KASREC ## name` to stderr before each.
//!   Built against `worktrees/record`, this records the constraint systems (see
//!   `../../scripts/record.sh`).
//! - `search KIND SEED N`: solves N random layouts of a kind (`realistic`, `extreme`, `big`,
//!   `bignz`) 3 times each, and reports panics, calls that disagree, and invalid areas. Prints each
//!   case's spec before solving it, so a hang can be identified (see `../../scripts/search.sh`).
//! - `sets`: lists the sets.

use std::collections::BTreeMap;
use std::panic::catch_unwind;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Constraint::{self, Fill, Length, Max, Min, Percentage, Ratio};
use ratatui_core::layout::{Flex, Layout, Rect};
use ratatui_core::widgets::Widget;
use ratatui_widgets::table::{Row, Table};

const FLEX: [Flex; 7] = [
    Flex::Legacy,
    Flex::Start,
    Flex::End,
    Flex::Center,
    Flex::SpaceBetween,
    Flex::SpaceAround,
    Flex::SpaceEvenly,
];

/// A layout: `Layout::{horizontal,vertical}(constraints).flex(flex).spacing(spacing).split(area)`.
/// A `Table` case renders a one-row table with these column constraints instead, at every width
/// from 0 to `area.width`.
#[derive(Clone, Debug)]
struct Case {
    vertical: bool,
    table: bool,
    flex: Flex,
    spacing: i16,
    area: Rect,
    constraints: Vec<Constraint>,
}

impl Case {
    /// Parses `DIR:FLEX:SPACING:X,Y,W,H:CONSTRAINTS`, where DIR is `h`, `v` or `table`, and
    /// CONSTRAINTS is a comma-separated list like `Min(3)*24,Fill(1),Ratio(1/3)`. e.g.
    /// `v:Start:0:0,0,30,72:Min(3)*24`.
    fn parse(spec: &str) -> Case {
        let parts: Vec<&str> = spec.splitn(5, ':').collect();
        assert_eq!(
            parts.len(),
            5,
            "expected DIR:FLEX:SPACING:X,Y,W,H:CONSTRAINTS, got {spec}"
        );
        let flex = FLEX
            .into_iter()
            .find(|f| format!("{f:?}") == parts[1])
            .unwrap_or_else(|| panic!("unknown flex {}", parts[1]));
        let r: Vec<u16> = parts[3].split(',').map(|x| x.parse().unwrap()).collect();
        let mut constraints = vec![];
        for item in parts[4].split(',') {
            let (item, count) = item
                .split_once('*')
                .map_or((item, 1), |(c, n)| (c, n.parse().unwrap()));
            let (kind, value) = item.trim_end_matches(')').split_once('(').unwrap();
            let c = match kind {
                "Min" => Min(value.parse().unwrap()),
                "Max" => Max(value.parse().unwrap()),
                "Length" => Length(value.parse().unwrap()),
                "Percentage" => Percentage(value.parse().unwrap()),
                "Fill" => Fill(value.parse().unwrap()),
                "Ratio" => {
                    let (a, b) = value.split_once('/').unwrap();
                    Ratio(a.parse().unwrap(), b.parse().unwrap())
                }
                _ => panic!("unknown constraint {item}"),
            };
            constraints.extend(std::iter::repeat_n(c, count));
        }
        Case {
            vertical: parts[0] == "v",
            table: parts[0] == "table",
            flex,
            spacing: parts[2].parse().unwrap(),
            area: Rect::new(r[0], r[1], r[2], r[3]),
            constraints,
        }
    }

    fn spec(&self) -> String {
        let dir = if self.table {
            "table"
        } else if self.vertical {
            "v"
        } else {
            "h"
        };
        let constraints: Vec<String> = self
            .constraints
            .iter()
            .map(|c| match c {
                Ratio(a, b) => format!("Ratio({a}/{b})"),
                c => format!("{c:?}"),
            })
            .collect();
        let a = self.area;
        format!(
            "{dir}:{:?}:{}:{},{},{},{}:{}",
            self.flex,
            self.spacing,
            a.x,
            a.y,
            a.width,
            a.height,
            constraints.join(",")
        )
    }

    fn layout(&self) -> Layout {
        let c = self.constraints.iter().copied();
        let layout = if self.vertical {
            Layout::vertical(c)
        } else {
            Layout::horizontal(c)
        };
        layout.flex(self.flex).spacing(self.spacing)
    }

    /// The areas, as (position, size) along the layout's direction.
    fn solve(&self) -> Vec<(u16, u16)> {
        if self.table {
            // the rendered line at every width, hashed
            let lines: Vec<String> = (0..=self.area.width)
                .map(|w| self.render_table(w))
                .collect();
            let hash = lines
                .join("\n")
                .bytes()
                .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
                    (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
                });
            return vec![((hash >> 48) as u16, hash as u16)];
        }
        self.layout()
            .split(self.area)
            .iter()
            .map(|r| {
                if self.vertical {
                    (r.y, r.height)
                } else {
                    (r.x, r.width)
                }
            })
            .collect()
    }

    fn render_table(&self, width: u16) -> String {
        let cells: Vec<String> = (0..self.constraints.len()).map(|i| i.to_string()).collect();
        let table = Table::new([Row::new(cells)], self.constraints.clone())
            .flex(self.flex)
            .column_spacing(self.spacing.max(0) as u16);
        let area = Rect::new(0, 0, width, 1);
        let mut buf = Buffer::empty(area);
        Widget::render(table, area, &mut buf);
        (0..width).map(|x| buf[(x, 0)].symbol()).collect()
    }

    /// Checks the areas: inside the layout's area, in order without overlapping (unless the
    /// spacing is negative), and, when only `Min` and `Length` constraints are used and their sizes
    /// fit, every `Min` segment at least its minimum.
    fn validate(&self, areas: &[(u16, u16)]) -> Result<(), String> {
        if self.table {
            return Ok(());
        }
        let (start, size) = if self.vertical {
            (self.area.y, self.area.height)
        } else {
            (self.area.x, self.area.width)
        };
        let end = u32::from(start) + u32::from(size);
        for (i, &(p, s)) in areas.iter().enumerate() {
            if p < start || u32::from(p) + u32::from(s) > end {
                return Err(format!("segment {i} ({p}, {s}) is outside {start}..{end}"));
            }
        }
        if self.spacing >= 0 {
            for (i, w) in areas.windows(2).enumerate() {
                if u32::from(w[0].0) + u32::from(w[0].1) > u32::from(w[1].0) {
                    return Err(format!(
                        "segments {i} and {} overlap: {:?} {:?}",
                        i + 1,
                        w[0],
                        w[1]
                    ));
                }
            }
        }
        let only_min_length = self
            .constraints
            .iter()
            .all(|c| matches!(c, Min(_) | Length(_)));
        let needed: u32 = self
            .constraints
            .iter()
            .map(|c| match c {
                Min(v) | Length(v) => u32::from(*v),
                _ => 0,
            })
            .sum::<u32>()
            + self.spacing.max(0) as u32 * (self.constraints.len().saturating_sub(1) as u32);
        if only_min_length && needed <= u32::from(size) {
            for (i, (c, &(_, s))) in self.constraints.iter().zip(areas).enumerate() {
                if let Min(m) = c {
                    if s < *m {
                        return Err(format!(
                            "segment {i} is {s}, less than its Min({m}), though they fit"
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

/// The hard cases: (name, spec, time limit in seconds, expectation, note). The expectation is
/// what fix/harris-ratio-test (the best version so far) does: `valid` (valid areas within the time
/// limit), or a known problem that isn't fixed yet: `panic`, `invalid` or `timeout`. The time
/// limits are generous (several times the time with fix/harris-ratio-test) to allow for a loaded
/// machine.
const HARD: &[(&str, &str, u32, &str, &str)] = &[
    (
        "i1855-24",
        "v:Start:0:0,0,30,72:Min(3)*24",
        10,
        "valid",
        "ratatui/ratatui#1855: minimums filling the area exactly; hung in 49 of 50 runs on 0.4.12",
    ),
    (
        "i1855-16",
        "v:Start:0:0,0,30,48:Min(3)*16",
        10,
        "valid",
        "ratatui/ratatui#1855",
    ),
    (
        "min1-25",
        "v:Start:0:0,0,10,25:Min(1)*25",
        10,
        "valid",
        "ratatui/ratatui#1855 (joshka)",
    ),
    (
        "min1-100",
        "v:Start:0:0,0,10,100:Min(1)*100",
        30,
        "valid",
        "about 0.5 s",
    ),
    (
        "min1-200",
        "v:Start:0:0,0,10,200:Min(1)*200",
        120,
        "valid",
        "about 8 s: ratatui adds a constraint for every pair of Min segments",
    ),
    (
        "min1-1000",
        "v:Start:0:0,0,10,1000:Min(1)*1000",
        60,
        "timeout",
        "about 500,000 constraints; needs fewer constraints from ratatui",
    ),
    (
        "table20",
        "table:SpaceAround:1:0,0,300,1:Min(3)*20",
        600,
        "valid",
        "ratatui/kasuari#18 (estokes): 20 Min columns, rendered at every width 0-300",
    ),
    (
        "finding-a",
        "v:Start:0:0,0,10,5:Length(2),Length(3),Length(2),Min(0),Length(1),Length(2),Length(1)",
        10,
        "valid",
        "ratatui-widgettable finding A: header and footer that don't fit; random result on 0.4.12",
    ),
    (
        "finding-b",
        "h:Start:0:29,0,31,1:Max(36),Fill(17949),Fill(65535),Fill(65535),Fill(28),Min(43850),Fill(38208),Fill(43),Length(0),Length(0)",
        10,
        "valid",
        "ratatui-widgettable finding B: extreme values; hung or panicked on 0.4.12",
    ),
    (
        "realistic",
        "h:SpaceAround:2:0,0,6,1:Fill(1),Min(13),Ratio(3/1),Percentage(10),Percentage(39),Percentage(74),Ratio(3/2),Percentage(67),Length(3)",
        10,
        "valid",
        "only ordinary values, but hung or panicked every few thousand calls on 0.4.12",
    ),
    (
        "degenerate",
        "h:Legacy:1:0,0,334,1:Fill(0),Max(8),Max(8),Fill(2),Length(1),Max(8),Fill(2),Length(5),Min(3),Min(5),Fill(1),Min(4),Min(7),Min(8),Fill(3),Min(0),Fill(1),Fill(1),Fill(1),Length(9),Min(1),Fill(2),Length(5),Max(3),Fill(0),Length(7),Fill(2),Min(5),Length(3),Min(9),Max(3),Length(4),Min(7),Fill(2),Fill(3),Length(3),Min(9),Fill(3),Length(4),Max(5),Fill(3),Min(8),Min(0),Length(6),Fill(4),Fill(1),Fill(0),Fill(0),Max(0),Max(9),Fill(2),Length(1),Max(0),Length(7),Fill(0),Length(7),Fill(2)",
        30,
        "valid",
        "cycles forever if the entering symbol is always the most negative (no Bland fallback)",
    ),
    (
        "bland-slow",
        "h:Legacy:1:0,0,287,1:Fill(2),Min(6),Fill(4),Length(6),Length(2),Min(6),Max(9),Min(8),Min(1),Fill(3),Fill(3),Length(9),Length(1),Length(6),Fill(0),Fill(4),Fill(2),Length(4),Fill(1),Fill(0),Fill(0),Max(6),Length(0),Max(8),Fill(3),Min(0),Min(5),Fill(3),Length(1),Length(3),Length(8),Fill(3),Length(8),Min(7),Min(7),Min(1),Length(8),Fill(3),Fill(1),Min(8),Fill(4),Max(9),Length(2),Min(0),Fill(4),Min(8),Max(9),Min(4),Min(6),Fill(0),Fill(2),Max(6),Fill(2),Fill(3),Fill(2)",
        30,
        "valid",
        "about 4 s and ObjectiveUnbounded with Bland's rule only; 0.24 s with the most negative",
    ),
    (
        "fill0",
        "h:Legacy:1:0,0,334,1:Fill(0),Max(8),Max(8),Min(0),Max(8),Fill(1),Min(0),Min(5),Fill(1),Min(4),Min(7),Min(8),Fill(3),Min(0),Fill(1),Fill(1),Fill(1),Fill(1),Fill(0),Length(1),Length(5),Max(3),Fill(0),Length(7),Fill(0),Min(5),Length(3),Min(9),Max(3),Min(7),Fill(2),Fill(1),Fill(3),Length(3),Min(9),Fill(3),Length(4),Max(5),Fill(3),Fill(1),Fill(0),Length(6),Fill(4),Fill(1),Fill(0),Min(0),Max(9),Fill(2),Length(1),Min(0),Length(7),Fill(0),Length(1),Min(0)",
        30,
        "valid",
        "gold-silver-copper/ratatui#4: Fill(0) is encoded as 1e-6; ObjectiveUnbounded with PR #1, valid with Harris' ratio test",
    ),
    (
        "fill0-overlap",
        "h:Legacy:1:0,0,334,1:Fill(0),Max(8),Max(8),Fill(2),Length(1),Max(8),Fill(2),Length(5),Min(3),Min(5),Fill(1),Min(4),Min(7),Min(8),Fill(3),Min(0),Fill(1),Fill(1),Fill(1),Length(9),Min(1),Fill(2),Length(5),Max(3),Fill(0),Length(7),Fill(2),Fill(3),Min(5),Length(3),Min(9),Max(3),Length(4),Min(7),Fill(2),Fill(1),Fill(3),Length(3),Min(9),Fill(3),Length(4),Max(5),Fill(3),Min(8),Min(0),Length(6),Fill(4),Fill(1),Fill(0),Fill(0),Max(0),Max(9),Fill(2),Length(1),Max(0),Length(7),Fill(0),Length(7),Fill(2)",
        30,
        "valid",
        "fill0 without one constraint: invalid (overlapping) after about 4 s with PR #1, valid in 41 ms with Harris",
    ),
    (
        "cols-30",
        "h:SpaceAround:1:0,0,190,1:Min(3)*30",
        30,
        "valid",
        "dense: every pair of Min columns is a constraint",
    ),
    (
        "cols-36",
        "h:SpaceAround:1:0,0,226,1:Min(3)*36",
        60,
        "valid",
        "",
    ),
    (
        "cols-40",
        "h:SpaceAround:1:0,0,250,1:Min(3)*40",
        60,
        "valid",
        "invalid (overlapping) after about 90 s with PR #1; valid in about 4 s with fix/harris-ratio-test",
    ),
    (
        "cols-44",
        "h:SpaceAround:1:0,0,274,1:Min(3)*44",
        600,
        "valid",
        "about 55 s",
    ),
    (
        "cols-60",
        "h:SpaceAround:1:0,0,370,1:Min(3)*60",
        900,
        "valid",
        "about 2 minutes",
    ),
];

fn named(name: &str) -> Option<Case> {
    HARD.iter().find(|h| h.0 == name).map(|h| Case::parse(h.1))
}

// --- sets of layouts ----------------------------------------------------------------------------

fn case(
    vertical: bool,
    flex: Flex,
    spacing: i16,
    area: Rect,
    constraints: Vec<Constraint>,
) -> Case {
    Case {
        vertical,
        table: false,
        flex,
        spacing,
        area,
        constraints,
    }
}

/// Layouts that applications commonly use, at many sizes
fn typical() -> Vec<Case> {
    let shapes: Vec<(Vec<Constraint>, bool, Flex, i16)> = vec![
        (vec![Length(1), Min(0), Length(1)], true, Flex::Legacy, 0),
        (vec![Length(3), Min(0), Length(3)], true, Flex::Legacy, 0),
        (vec![Length(20), Min(0)], false, Flex::Legacy, 0),
        (vec![Percentage(30), Percentage(70)], false, Flex::Legacy, 0),
        (
            vec![Percentage(20), Percentage(60), Percentage(20)],
            false,
            Flex::Legacy,
            0,
        ),
        (vec![Ratio(1, 3), Ratio(2, 3)], false, Flex::Legacy, 0),
        (vec![Fill(1), Fill(2), Fill(1)], false, Flex::Legacy, 1),
        (vec![Length(1), Fill(1), Length(1)], true, Flex::Center, 0),
        (vec![Length(40)], false, Flex::Center, 0),
        (
            vec![Length(6), Length(20), Min(10), Length(10), Length(8)],
            false,
            Flex::Start,
            1,
        ),
        (
            vec![Length(10), Fill(1), Fill(1), Percentage(20)],
            false,
            Flex::Start,
            1,
        ),
        (
            vec![Max(30), Min(20), Length(12)],
            false,
            Flex::SpaceBetween,
            1,
        ),
        (vec![Length(15); 5], false, Flex::SpaceAround, 1),
        (vec![Min(10); 8], false, Flex::Legacy, 1),
    ];
    let mut cases = vec![];
    for size in (0..=300).step_by(3) {
        for (c, vertical, flex, spacing) in &shapes {
            let area = if *vertical {
                Rect::new(0, 0, 80, size)
            } else {
                Rect::new(0, 0, size, 24)
            };
            cases.push(case(*vertical, *flex, *spacing, area, c.clone()));
        }
    }
    cases
}

/// Random layouts with realistic values
fn realistic(rng: &mut fastrand::Rng) -> Case {
    let c = (0..rng.usize(1..12))
        .map(|_| match rng.u8(0..6) {
            0 => Min(rng.u16(0..40)),
            1 => Max(rng.u16(0..40)),
            2 => Length(rng.u16(0..40)),
            3 => Percentage(rng.u16(0..=100)),
            4 => Ratio(rng.u32(1..5), rng.u32(1..10)),
            _ => Fill(rng.u16(0..5)),
        })
        .collect();
    let flex = FLEX[rng.usize(0..7)];
    case(
        false,
        flex,
        rng.i16(0..3),
        Rect::new(0, 0, rng.u16(0..250), 1),
        c,
    )
}

/// Random layouts including extreme values
fn extreme(rng: &mut fastrand::Rng) -> Case {
    let value = |rng: &mut fastrand::Rng| {
        if rng.bool() {
            rng.u16(0..100)
        } else {
            rng.u16(..)
        }
    };
    let c = (0..rng.usize(1..12))
        .map(|_| match rng.u8(0..6) {
            0 => Min(value(rng)),
            1 => Max(value(rng)),
            2 => Length(value(rng)),
            3 => Percentage(value(rng)),
            4 => Ratio(
                if rng.bool() {
                    rng.u32(0..100)
                } else {
                    rng.u32(..)
                },
                rng.u32(..),
            ),
            _ => Fill(value(rng)),
        })
        .collect();
    let spacing = if rng.bool() {
        rng.i16(0..5)
    } else {
        rng.i16(-32767..)
    };
    let flex = FLEX[rng.usize(0..7)];
    case(
        false,
        flex,
        spacing,
        Rect::new(rng.u16(0..100), 0, rng.u16(0..200), 1),
        c,
    )
}

/// Layouts of 60 small constraints; with `Fill(0)` unless `nonzero_fill`
fn big(rng: &mut fastrand::Rng, nonzero_fill: bool) -> Case {
    let c = (0..60)
        .map(|_| match rng.u8(0..4) {
            0 => Min(rng.u16(0..10)),
            1 => Max(rng.u16(0..10)),
            2 => Length(rng.u16(0..10)),
            _ => Fill(rng.u16(u16::from(nonzero_fill)..5)),
        })
        .collect();
    case(
        false,
        Flex::Legacy,
        1,
        Rect::new(0, 0, rng.u16(0..400), 1),
        c,
    )
}

fn random_cases(kind: &str, seed: u64, n: usize) -> Vec<Case> {
    let mut rng = fastrand::Rng::with_seed(seed);
    (0..n)
        .map(|_| match kind {
            "realistic" => realistic(&mut rng),
            "extreme" => extreme(&mut rng),
            "big" => big(&mut rng, false),
            "bignz" => big(&mut rng, true),
            _ => panic!("unknown kind {kind}"),
        })
        .collect()
}

/// The sets of layouts. The random ones use fixed seeds, so they're the same every time.
fn set(name: &str) -> Vec<(String, Case)> {
    let numbered = |cases: Vec<Case>| {
        cases
            .into_iter()
            .enumerate()
            .map(|(i, c)| (i.to_string(), c))
            .collect()
    };
    match name {
        "typical" => numbered(typical()),
        "random" => numbered(random_cases("realistic", 1, 2000)),
        "extreme" => numbered(random_cases("extreme", 2, 2000)),
        "big" => numbered(random_cases("bignz", 3, 100)),
        "large" => {
            let mut cases: Vec<Case> = [25, 50, 100]
                .map(|n| Case::parse(&format!("v:Start:0:0,0,10,{n}:Min(1)*{n}")))
                .into();
            cases.extend(
                (0..=300)
                    .step_by(10)
                    .map(|w| Case::parse(&format!("h:SpaceAround:1:0,0,{w},1:Min(3)*20"))),
            );
            numbered(cases)
        }
        "cols" => numbered(
            (10..=40)
                .step_by(2)
                .flat_map(|n| {
                    [6 * n + 10, 5 * n, 4 * n]
                        .map(|w| Case::parse(&format!("h:SpaceAround:1:0,0,{w},1:Min(3)*{n}")))
                })
                .collect(),
        ),
        // the hard cases that are expected to finish within seconds with every version so far
        "named" => HARD
            .iter()
            .filter(|h| {
                h.3 != "timeout"
                    && !["table20", "cols-40", "cols-44", "cols-60", "min1-200"].contains(&h.0)
            })
            .map(|h| (h.0.to_string(), Case::parse(h.1)))
            .collect(),
        // the named hard cases, and the 20-column table at every width
        "hard" => {
            let mut cases = set("named");
            for min in [1, 3, 5, 10] {
                for w in 0..=300 {
                    let spec = format!("h:SpaceAround:1:0,0,{w},1:Min({min})*20");
                    cases.push((format!("table20-min{min}-width{w}"), Case::parse(&spec)));
                }
            }
            cases
        }
        _ => panic!("unknown set {name}"),
    }
}

const SETS: &str = "typical (1,414), random (2,000), extreme (2,000), big (100), large (34), cols (48), named, hard";

// --- CPU time ------------------------------------------------------------------------------------

fn thread_cpu_time() -> f64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
    ts.tv_sec as f64 + ts.tv_nsec as f64 * 1e-9
}

fn process_cpu_time() -> f64 {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    let t = |tv: libc::timeval| tv.tv_sec as f64 + tv.tv_usec as f64 * 1e-6;
    t(usage.ru_utime) + t(usage.ru_stime)
}

fn panic_message(e: Box<dyn std::any::Any + Send>) -> String {
    e.downcast_ref::<String>()
        .cloned()
        .or(e.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

// --- commands ------------------------------------------------------------------------------------

fn run_case(name: &str, n: usize) {
    let case = named(name).unwrap_or_else(|| Case::parse(name));
    let mut results = BTreeMap::<String, usize>::new();
    let mut times = vec![];
    let mut verdict = String::new();
    for _ in 0..n {
        let start = thread_cpu_time();
        let result = catch_unwind(|| case.solve());
        times.push(thread_cpu_time() - start);
        let text = match &result {
            Ok(areas) => {
                verdict = match case.validate(areas) {
                    Ok(()) => "VALID".into(),
                    Err(e) => format!("INVALID {e}"),
                };
                format!("{areas:?}")
            }
            Err(_) => {
                verdict = "PANIC".into();
                "PANIC".into()
            }
        };
        *results.entry(text).or_default() += 1;
    }
    times.sort_by(f64::total_cmp);
    let panic_messages = if verdict == "PANIC" {
        let e = catch_unwind(|| case.solve())
            .err()
            .map(panic_message)
            .unwrap_or_default();
        format!(" ({e})")
    } else {
        String::new()
    };
    println!("CASE {}", case.spec());
    println!(
        "CPU per call: median {:.2} ms, min {:.2} ms",
        times[times.len() / 2] * 1e3,
        times[0] * 1e3
    );
    for (r, count) in &results {
        println!("{count}x {r}");
    }
    let agree = if results.len() == 1 {
        "calls agree"
    } else {
        "CALLS DISAGREE"
    };
    println!("RESULT {verdict}{panic_messages}; {agree}");
}

fn bench(name: &str) {
    let cases = set(name);
    let (process_start, start) = (process_cpu_time(), thread_cpu_time());
    for (_, case) in &cases {
        std::hint::black_box(case.solve());
    }
    let thread = thread_cpu_time() - start;
    let process = process_cpu_time() - process_start;
    let per = |t: f64| t * 1e6 / cases.len() as f64;
    println!(
        "BENCH set={name} layouts={} thread_cpu_us_per_layout={:.2} process_cpu_us_per_layout={:.2}",
        cases.len(),
        per(thread),
        per(process)
    );
}

fn search(kind: &str, seed: u64, n: usize) {
    let (mut panics, mut disagree, mut invalid) = (vec![], vec![], vec![]);
    for (i, case) in random_cases(kind, seed, n).into_iter().enumerate() {
        println!("CASE {kind}:{seed}:{i} {}", case.spec());
        let Ok(first) = catch_unwind(|| case.solve()) else {
            panics.push(i);
            continue;
        };
        if (0..2).any(|_| catch_unwind(|| case.solve()).ok().as_ref() != Some(&first)) {
            disagree.push(i);
        }
        if case.validate(&first).is_err() {
            invalid.push(i);
        }
    }
    println!("RESULT panics={panics:?} disagree={disagree:?} invalid={invalid:?}");
}

fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let args: Vec<String> = std::env::args().collect();
    let arg = |i: usize| args.get(i).map(String::as_str).unwrap_or_default();
    match arg(1) {
        "case" => run_case(arg(2), arg(3).parse().unwrap_or(1)),
        "hard" => {
            for (name, spec, limit, expectation, note) in HARD {
                println!("{name}\t{spec}\t{limit}\t{expectation}\t{note}");
            }
        }
        "bench" => bench(arg(2)),
        "outputs" => {
            for (name, case) in set(arg(2)) {
                let r = catch_unwind(|| case.solve()).map_or("PANIC".into(), |a| format!("{a:?}"));
                println!("{name} {r}");
            }
        }
        "record" => {
            for (name, case) in set(arg(2)) {
                eprintln!("KASREC ## {} {name} {}", arg(2), case.spec());
                let _ = catch_unwind(|| case.solve());
            }
        }
        "search" => search(arg(2), arg(3).parse().unwrap(), arg(4).parse().unwrap()),
        "sets" => println!("{SETS}"),
        _ => {
            eprintln!("usage: see the comment at the top of src/main.rs");
            std::process::exit(2)
        }
    }
}
