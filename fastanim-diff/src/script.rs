//! Checking and applying edit scripts (§5.7).

use std::fmt;
use std::hash::Hash;

use crate::Op;

/// Why an edit script is not a valid transformation of `a` into `b`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptError {
    /// An index of `a` is used more than once or out of range.
    BadA(usize),
    /// An index of `b` is used more than once, out of range, or out of order.
    BadB(usize),
    /// An index of `a` is not accounted for.
    MissingA(usize),
    /// An index of `b` is not produced.
    MissingB(usize),
    /// `Equal` ops are not in increasing `a` order.
    EqualOutOfOrder(usize),
    /// An `Equal` or `Move` pairs items whose keys differ.
    KeyMismatch {
        /// Index into `a`.
        a: usize,
        /// Index into `b`.
        b: usize,
    },
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadA(i) => write!(f, "a[{i}] is used twice or out of range"),
            Self::BadB(j) => write!(f, "b[{j}] is used twice, out of range or out of order"),
            Self::MissingA(i) => write!(f, "a[{i}] is not accounted for"),
            Self::MissingB(j) => write!(f, "b[{j}] is not produced"),
            Self::EqualOutOfOrder(i) => write!(f, "Equal for a[{i}] is out of order"),
            Self::KeyMismatch { a, b } => write!(f, "a[{a}] and b[{b}] have different keys"),
        }
    }
}

impl std::error::Error for ScriptError {}

/// Checks the structure of a script for inputs of length `n` and `m`: every index of each
/// input is used exactly once, `b` indices appear in increasing order, and `Equal` ops keep
/// `a` order.
pub fn validate(ops: &[Op], n: usize, m: usize) -> Result<(), ScriptError> {
    let mut seen_a = vec![false; n];
    let mut use_a = |i: usize| match seen_a.get_mut(i) {
        Some(s) if !*s => {
            *s = true;
            Ok(())
        }
        _ => Err(ScriptError::BadA(i)),
    };
    let mut next_b = 0;
    let mut use_b = |j: usize| {
        if j == next_b && j < m {
            next_b += 1;
            Ok(())
        } else {
            Err(ScriptError::BadB(j))
        }
    };
    let mut last_equal_a = None;
    for op in ops {
        match op {
            Op::Equal { a, b } => {
                if last_equal_a.is_some_and(|l| l >= *a) {
                    return Err(ScriptError::EqualOutOfOrder(*a));
                }
                last_equal_a = Some(*a);
                use_a(*a)?;
                use_b(*b)?;
            }
            Op::Move { a, b } => {
                use_a(*a)?;
                use_b(*b)?;
            }
            Op::Delete { a } => use_a(*a)?,
            Op::Insert { b } => use_b(*b)?,
            Op::Replace { a, b } => {
                a.clone().try_for_each(&mut use_a)?;
                b.clone().try_for_each(&mut use_b)?;
            }
        }
    }
    if let Some(i) = seen_a.iter().position(|s| !s) {
        return Err(ScriptError::MissingA(i));
    }
    if next_b < m {
        return Err(ScriptError::MissingB(next_b));
    }
    Ok(())
}

/// Applies a script to `a`, producing the new sequence. Kept and moved items are taken from
/// `a`; inserted and replacement items from `b`. Fails if the script is invalid or pairs
/// items whose keys differ, so on success the result's keys equal `b`'s.
pub fn apply<T: Clone, K: Eq + Hash>(
    a: &[T],
    b: &[T],
    ops: &[Op],
    key: impl Fn(&T) -> K,
) -> Result<Vec<T>, ScriptError> {
    validate(ops, a.len(), b.len())?;
    let mut out = Vec::with_capacity(b.len());
    for op in ops {
        match op {
            Op::Equal { a: i, b: j } | Op::Move { a: i, b: j } => {
                if key(&a[*i]) != key(&b[*j]) {
                    return Err(ScriptError::KeyMismatch { a: *i, b: *j });
                }
                out.push(a[*i].clone());
            }
            Op::Delete { .. } => {}
            Op::Insert { b: j } => out.push(b[*j].clone()),
            Op::Replace { b: r, .. } => out.extend_from_slice(&b[r.clone()]),
        }
    }
    Ok(out)
}

/// Number of items not kept in place: every `a` and `b` item outside an `Equal` counts once.
/// For a core script (`Equal`/`Delete`/`Insert` only) this is the edit distance `D`.
pub fn edit_cost(ops: &[Op]) -> usize {
    ops.iter()
        .map(|op| match op {
            Op::Equal { .. } => 0,
            Op::Delete { .. } | Op::Insert { .. } => 1,
            Op::Move { .. } => 2,
            Op::Replace { a, b } => a.len() + b.len(),
        })
        .sum()
}
