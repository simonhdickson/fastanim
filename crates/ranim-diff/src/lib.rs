//! A small, dependency-free, generic sequence diff library (see `docs/SPEC.md` §5).
//!
//! It knows nothing about animation: it turns two ordered sequences into an edit script of
//! [`Op`]s. Items are compared by a caller-supplied **key** rather than `PartialEq`, so the
//! caller decides what "the same" means.
//!
//! The pipeline is:
//!
//! 1. Keys are interned to dense integers and the common prefix/suffix is trimmed.
//! 2. A shortest edit script is computed with Myers' greedy algorithm, its linear-space
//!    variant (chosen automatically for large inputs), or patience diff. For inputs of
//!    moderate size, ties between equally short scripts are broken by [`TieBreak::Stable`].
//! 3. Hunks are normalized so deletions come before insertions (determinism).
//! 4. Optional post-passes: move detection (§5.4), semantic cleanup, and replacement
//!    pairing, both adjacent and cross-hunk (§5.5).
//!
//! ```
//! use ranim_diff::{diff, DiffOptions, Op};
//!
//! let a: Vec<char> = "a+b=c".chars().collect();
//! let b: Vec<char> = "b+a=c".chars().collect();
//! let ops = diff(&a, &b, |c| *c, &DiffOptions::default());
//! assert!(ops.iter().any(|op| matches!(op, Op::Move { .. })));
//! ```

#![forbid(unsafe_code)]

mod assign;
mod linear;
mod myers;
mod patience;
mod post;
mod script;
mod stable;

use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Range;

pub use script::{ScriptError, apply, edit_cost, validate};

/// Above this many items (`N + M`, after trimming), [`Algorithm::Myers`] switches to the
/// linear-space variant.
pub const LINEAR_SPACE_THRESHOLD: usize = 10_000;

/// [`TieBreak::Stable`] applies while `N · M` (after trimming, if needed) is at most this.
pub const STABLE_LIMIT: usize = 1 << 20;

/// One step of an edit script.
///
/// Scripts list ops in output order: the `b` indices of `Equal`, `Insert`, `Move` and
/// `Replace` ops appear in increasing order, so walking the script reconstructs `b`.
/// `Delete` ops sit at their position relative to the surviving `a` items.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Op {
    /// `a[a]` is kept and becomes `b[b]`.
    Equal {
        /// Index into `a`.
        a: usize,
        /// Index into `b`.
        b: usize,
    },
    /// `a[a]` is removed.
    Delete {
        /// Index into `a`.
        a: usize,
    },
    /// `b[b]` is new.
    Insert {
        /// Index into `b`.
        b: usize,
    },
    /// `a[a]` and `b[b]` have equal keys but were not matched in order: the item travels.
    Move {
        /// Index into `a`.
        a: usize,
        /// Index into `b`.
        b: usize,
    },
    /// The items `a[a]` are replaced by (morphed into) the items `b[b]`.
    Replace {
        /// Range of `a`.
        a: Range<usize>,
        /// Range of `b`.
        b: Range<usize>,
    },
}

/// The core shortest-edit-script algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Algorithm {
    /// Myers' greedy algorithm, switching to the linear-space variant above
    /// [`LINEAR_SPACE_THRESHOLD`] items.
    #[default]
    Myers,
    /// Always use Myers' linear-space (middle snake) variant.
    MyersLinearSpace,
    /// Patience diff: anchor on items that are unique in both inputs, Myers in between.
    /// Not guaranteed minimal, but often reads better for code.
    Patience,
}

/// How to choose between edit scripts of equal (minimal) cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TieBreak {
    /// Prefer the script whose unchanged blocks shift the least, then the one with fewest
    /// blocks, so that what stayed the same visibly stays put. For example
    /// `a + b = c → b + a = c` keeps `+ = c` and swaps the letters. Costs an O(N·M) pass,
    /// so it applies only up to [`STABLE_LIMIT`]; larger inputs keep Myers' choice.
    #[default]
    Stable,
    /// Whatever the algorithm's search order yields first. Fastest.
    Myers,
}

/// Folding of short equal runs into the surrounding edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cleanup {
    /// Keep the edit script as computed.
    #[default]
    None,
    /// Equal runs shorter than `min_equal_run` with an insertion or deletion on both sides
    /// are turned into edits, so a lone unchanged item doesn't sit still while everything
    /// around it changes.
    Semantic {
        /// Shortest equal run that is kept.
        min_equal_run: usize,
    },
}

/// Options for [`diff`] and [`Differ`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffOptions {
    /// The core algorithm.
    pub algorithm: Algorithm,
    /// Choice between equally short scripts. Ignored by [`Algorithm::Patience`].
    pub tie_break: TieBreak,
    /// Pair deletions and insertions with equal keys into [`Op::Move`].
    pub detect_moves: bool,
    /// Optional folding of short equal runs.
    pub cleanup: Cleanup,
    /// Pair adjacent deletions and insertions into [`Op::Replace`], and, when a class
    /// function is given to [`Differ::class`], leftover non-adjacent ones of the same class.
    pub pair_replacements: bool,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            algorithm: Algorithm::Myers,
            tie_break: TieBreak::Stable,
            detect_moves: true,
            cleanup: Cleanup::None,
            pair_replacements: true,
        }
    }
}

impl DiffOptions {
    /// Only the core algorithm: the result contains just `Equal`, `Delete` and `Insert`.
    pub fn raw() -> Self {
        Self {
            detect_moves: false,
            pair_replacements: false,
            ..Self::default()
        }
    }
}

/// Diffs `a` against `b`, comparing items by `key`.
///
/// Moves and cross-hunk replacements are paired by index distance; use [`Differ`] to supply
/// a visual cost or replacement classes.
pub fn diff<T, K: Eq + Hash>(
    a: &[T],
    b: &[T],
    key: impl Fn(&T) -> K,
    opts: &DiffOptions,
) -> Vec<Op> {
    Differ::new(a, b, key).options(opts.clone()).run()
}

type CostFn<'c> = Box<dyn Fn(usize, usize) -> f64 + 'c>;

/// Interned replacement classes of `a` and `b`.
type Classes = (Vec<Option<u32>>, Vec<Option<u32>>);

/// A configurable diff, for when [`diff`] isn't enough.
///
/// ```
/// use ranim_diff::{Differ, Op};
///
/// let a = ["a", "²", "+", "b", "²", "=", "c", "²"];
/// let b = ["a", "²", "=", "c", "²", "-", "b", "²"];
/// let ops = Differ::new(&a, &b, |t| *t)
///     .class(|t| matches!(*t, "+" | "-").then_some("operator"))
///     .run();
/// assert!(ops.contains(&Op::Replace { a: 2..3, b: 5..6 }));
/// ```
pub struct Differ<'c, T> {
    a: &'c [T],
    b: &'c [T],
    keys_a: Vec<u32>,
    keys_b: Vec<u32>,
    classes: Option<Classes>,
    cost: Option<CostFn<'c>>,
    opts: DiffOptions,
}

impl<'c, T> Differ<'c, T> {
    /// Prepares a diff of `a` against `b`, comparing items by `key`.
    pub fn new<K: Eq + Hash>(a: &'c [T], b: &'c [T], key: impl Fn(&T) -> K) -> Self {
        let (keys_a, keys_b) = intern(a, b, key);
        Self {
            a,
            b,
            keys_a,
            keys_b,
            classes: None,
            cost: None,
            opts: DiffOptions::default(),
        }
    }

    /// Sets the options.
    pub fn options(mut self, opts: DiffOptions) -> Self {
        self.opts = opts;
        self
    }

    /// Sets the cost of pairing `a[i]` with `b[j]` in move detection and cross-hunk
    /// replacement pairing, typically the distance between their centroids. Lower is better.
    /// Defaults to the index distance `|i - j|`.
    pub fn cost(mut self, cost: impl Fn(usize, usize) -> f64 + 'c) -> Self {
        self.cost = Some(Box::new(cost));
        self
    }

    /// Enables cross-hunk replacement pairing: leftover deletions and insertions whose class
    /// is `Some` and equal may be paired into a single-item [`Op::Replace`].
    pub fn class<C: Eq + Hash>(mut self, class: impl Fn(&T) -> Option<C>) -> Self {
        let mut ids = HashMap::new();
        let mut id = |item: &T| {
            class(item).map(|c| {
                let next = ids.len() as u32;
                *ids.entry(c).or_insert(next)
            })
        };
        let ca = self.a.iter().map(&mut id).collect();
        let cb = self.b.iter().map(&mut id).collect();
        self.classes = Some((ca, cb));
        self
    }

    /// Computes the edit script.
    pub fn run(&self) -> Vec<Op> {
        let (a, b) = (&self.keys_a[..], &self.keys_b[..]);
        let mut ops = core_diff(a, b, self.opts.algorithm, self.opts.tie_break);
        post::normalize(&mut ops);

        let index_cost = |i: usize, j: usize| i.abs_diff(j) as f64;
        let cost: &dyn Fn(usize, usize) -> f64 = match &self.cost {
            Some(c) => c,
            None => &index_cost,
        };

        if self.opts.detect_moves {
            ops = post::detect_moves(ops, a, b, cost);
        }
        if let Cleanup::Semantic { min_equal_run } = self.opts.cleanup {
            post::semantic_cleanup(&mut ops, min_equal_run);
        }
        if self.opts.pair_replacements {
            ops = post::pair_adjacent(ops);
            if let Some((ca, cb)) = &self.classes {
                ops = post::pair_cross_hunk(ops, ca, cb, cost);
            }
        }
        ops
    }
}

/// Expands a script over groups (e.g. lines) into one over their items (e.g. tokens), for
/// coarse-to-fine diffing (SPEC §5.6). `a` and `b` give each group's contiguous range of items.
///
/// Equal and moved groups pair their items in order (leftovers are deleted or inserted);
/// deleted and inserted groups expand item by item; each replaced run of groups is handed to
/// `inner` as item ranges, and its script (relative to those ranges) is spliced in.
///
/// ```
/// use ranim_diff::{Differ, Op, expand};
///
/// let (a, b) = (["x", "y", "z"], ["x", "w", "z"]);
/// let lines_a = [0..2, 2..3]; // "x y", "z"
/// let lines_b = [0..2, 2..3]; // "x w", "z"
/// let ka: Vec<_> = lines_a.iter().map(|l| &a[l.clone()]).collect();
/// let kb: Vec<_> = lines_b.iter().map(|l| &b[l.clone()]).collect();
/// let outer = Differ::new(&ka, &kb, |l| *l).run();
/// let ops = expand(&outer, &lines_a, &lines_b, |ra, rb| {
///     Differ::new(&a[ra], &b[rb], |t| *t).run()
/// });
/// assert_eq!(ops[0], Op::Equal { a: 0, b: 0 });
/// assert!(ops.contains(&Op::Equal { a: 2, b: 2 }));
/// ```
pub fn expand(
    ops: &[Op],
    a: &[Range<usize>],
    b: &[Range<usize>],
    mut inner: impl FnMut(Range<usize>, Range<usize>) -> Vec<Op>,
) -> Vec<Op> {
    // Items of a run of groups; groups are contiguous, so this is one range.
    let items = |g: &[Range<usize>], r: Range<usize>| match (g.get(r.start), r.end.checked_sub(1)) {
        (Some(first), Some(last)) if r.start < r.end => first.start..g[last].end,
        _ => 0..0,
    };
    let mut out = Vec::new();
    for op in ops {
        match op {
            Op::Equal { a: i, b: j } | Op::Move { a: i, b: j } => {
                let (ra, rb) = (a[*i].clone(), b[*j].clone());
                let n = ra.len().min(rb.len());
                for k in 0..n {
                    let (a, b) = (ra.start + k, rb.start + k);
                    out.push(match op {
                        Op::Equal { .. } => Op::Equal { a, b },
                        _ => Op::Move { a, b },
                    });
                }
                out.extend((ra.start + n..ra.end).map(|a| Op::Delete { a }));
                out.extend((rb.start + n..rb.end).map(|b| Op::Insert { b }));
            }
            Op::Delete { a: i } => out.extend(a[*i].clone().map(|a| Op::Delete { a })),
            Op::Insert { b: j } => out.extend(b[*j].clone().map(|b| Op::Insert { b })),
            Op::Replace { a: ga, b: gb } => {
                let (ra, rb) = (items(a, ga.clone()), items(b, gb.clone()));
                let (ao, bo) = (ra.start, rb.start);
                out.extend(inner(ra, rb).into_iter().map(|op| match op {
                    Op::Equal { a, b } => Op::Equal {
                        a: a + ao,
                        b: b + bo,
                    },
                    Op::Move { a, b } => Op::Move {
                        a: a + ao,
                        b: b + bo,
                    },
                    Op::Delete { a } => Op::Delete { a: a + ao },
                    Op::Insert { b } => Op::Insert { b: b + bo },
                    Op::Replace { a, b } => Op::Replace {
                        a: a.start + ao..a.end + ao,
                        b: b.start + bo..b.end + bo,
                    },
                }));
            }
        }
    }
    out
}

/// Maps keys to dense ids in order of first appearance. Exact (no hash collisions) and
/// deterministic across runs and platforms.
fn intern<T, K: Eq + Hash>(a: &[T], b: &[T], key: impl Fn(&T) -> K) -> (Vec<u32>, Vec<u32>) {
    let mut ids = HashMap::new();
    let mut id = |item: &T| {
        let next = ids.len() as u32;
        *ids.entry(key(item)).or_insert(next)
    };
    let ka = a.iter().map(&mut id).collect();
    let kb = b.iter().map(&mut id).collect();
    (ka, kb)
}

/// Runs the core algorithm on interned keys, producing `Equal`/`Delete`/`Insert` only.
fn core_diff(a: &[u32], b: &[u32], algorithm: Algorithm, tie_break: TieBreak) -> Vec<Op> {
    let mut out = Vec::with_capacity(a.len().max(b.len()));
    let sink = &mut Sink::new(&mut out);
    let myers = match algorithm {
        Algorithm::Patience => {
            patience::diff(a, b, 0, 0, sink);
            return out;
        }
        Algorithm::Myers if a.len() + b.len() <= LINEAR_SPACE_THRESHOLD => myers::diff,
        Algorithm::Myers | Algorithm::MyersLinearSpace => linear::diff,
    };
    let fits = |a: &[u32], b: &[u32]| a.len().saturating_mul(b.len()) <= STABLE_LIMIT;
    match tie_break {
        TieBreak::Myers => myers(a, b, 0, 0, sink),
        // Untrimmed when possible: trimming the suffix forces it to match, which can split
        // a block that would otherwise stay together.
        TieBreak::Stable if fits(a, b) => trimmed_none(a, b, sink),
        TieBreak::Stable => trimmed(a, b, 0, 0, sink, |a, b, ao, bo, sink| {
            if fits(a, b) {
                stable::diff(a, b, ao, bo, sink)
            } else {
                myers(a, b, ao, bo, sink)
            }
        }),
    }
    out
}

/// Runs [`stable::diff`] on whole inputs, handling empty ones.
fn trimmed_none(a: &[u32], b: &[u32], sink: &mut Sink) {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => (0..b.len()).for_each(|j| sink.insert(j)),
        (false, true) => (0..a.len()).for_each(|i| sink.delete(i)),
        (false, false) => stable::diff(a, b, 0, 0, sink),
    }
}

/// Collects core ops, translating sub-problem indices to global ones.
struct Sink<'o> {
    out: &'o mut Vec<Op>,
}

impl<'o> Sink<'o> {
    fn new(out: &'o mut Vec<Op>) -> Self {
        Self { out }
    }

    fn equal(&mut self, a: usize, b: usize) {
        self.out.push(Op::Equal { a, b });
    }

    fn delete(&mut self, a: usize) {
        self.out.push(Op::Delete { a });
    }

    fn insert(&mut self, b: usize) {
        self.out.push(Op::Insert { b });
    }
}

/// Emits the common prefix and returns its length.
fn emit_prefix(a: &[u32], b: &[u32], ao: usize, bo: usize, sink: &mut Sink) -> usize {
    let n = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    for i in 0..n {
        sink.equal(ao + i, bo + i);
    }
    n
}

/// Length of the common suffix (not emitted: callers emit it after the middle).
fn suffix_len(a: &[u32], b: &[u32]) -> usize {
    a.iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count()
}

/// Trims the common prefix and suffix around `middle`, which diffs what's left.
fn trimmed(
    a: &[u32],
    b: &[u32],
    ao: usize,
    bo: usize,
    sink: &mut Sink,
    middle: impl FnOnce(&[u32], &[u32], usize, usize, &mut Sink),
) {
    let p = emit_prefix(a, b, ao, bo, sink);
    let (a, b) = (&a[p..], &b[p..]);
    let s = suffix_len(a, b);
    let (am, bm) = (&a[..a.len() - s], &b[..b.len() - s]);
    let (ao, bo) = (ao + p, bo + p);
    match (am.is_empty(), bm.is_empty()) {
        (true, true) => {}
        (true, false) => (0..bm.len()).for_each(|j| sink.insert(bo + j)),
        (false, true) => (0..am.len()).for_each(|i| sink.delete(ao + i)),
        (false, false) => middle(am, bm, ao, bo, sink),
    }
    for i in 0..s {
        sink.equal(ao + am.len() + i, bo + bm.len() + i);
    }
}
