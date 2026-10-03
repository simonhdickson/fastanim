//! Post-processing passes over a core edit script (§5.4, §5.5).

use std::collections::BTreeMap;

use crate::Op;
use crate::assign::assign;

/// Within every maximal run of `Delete`/`Insert` ops, puts deletions first. Both keep their
/// relative order, so `a` and `b` indices stay increasing.
pub(crate) fn normalize(ops: &mut [Op]) {
    let mut start = 0;
    while start < ops.len() {
        if !is_edit(&ops[start]) {
            start += 1;
            continue;
        }
        let end = start + ops[start..].iter().take_while(|op| is_edit(op)).count();
        // Stable: false (Delete) sorts before true (Insert).
        ops[start..end].sort_by_key(|op| matches!(op, Op::Insert { .. }));
        start = end;
    }
}

fn is_edit(op: &Op) -> bool {
    matches!(op, Op::Delete { .. } | Op::Insert { .. })
}

/// Turns deleted/inserted pairs with equal keys into `Move`s, pairing by minimal total cost
/// within each key. The `Move` takes the insertion's place; the deletion is dropped.
pub(crate) fn detect_moves(
    ops: Vec<Op>,
    keys_a: &[u32],
    keys_b: &[u32],
    cost: &dyn Fn(usize, usize) -> f64,
) -> Vec<Op> {
    let mut deleted: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    let mut inserted: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for op in &ops {
        match *op {
            Op::Delete { a } => deleted.entry(keys_a[a]).or_default().push(a),
            Op::Insert { b } => inserted.entry(keys_b[b]).or_default().push(b),
            _ => {}
        }
    }
    let mut pairs = Vec::new();
    for (key, rows) in &deleted {
        if let Some(cols) = inserted.get(key) {
            pairs.extend(assign(rows, cols, cost));
        }
    }
    replace_pairs(ops, &pairs, |a, b| Op::Move { a, b })
}

/// Drops the `Delete` of every pair and replaces its `Insert` with `make(a, b)`.
fn replace_pairs(
    ops: Vec<Op>,
    pairs: &[(usize, usize)],
    make: impl Fn(usize, usize) -> Op,
) -> Vec<Op> {
    if pairs.is_empty() {
        return ops;
    }
    let partner_of_b: BTreeMap<usize, usize> = pairs.iter().map(|&(a, b)| (b, a)).collect();
    let paired_a: std::collections::BTreeSet<usize> = pairs.iter().map(|&(a, _)| a).collect();
    ops.into_iter()
        .filter_map(|op| match op {
            Op::Delete { a } if paired_a.contains(&a) => None,
            Op::Insert { b } => Some(match partner_of_b.get(&b) {
                Some(&a) => make(a, b),
                None => op,
            }),
            op => Some(op),
        })
        .collect()
}

/// Folds equal runs shorter than `min_equal_run` that have a `Delete` or `Insert` directly on
/// both sides into the surrounding edits, until nothing changes.
pub(crate) fn semantic_cleanup(ops: &mut Vec<Op>, min_equal_run: usize) {
    loop {
        let mut changed = false;
        let mut out = Vec::with_capacity(ops.len());
        let mut i = 0;
        while i < ops.len() {
            if !matches!(ops[i], Op::Equal { .. }) {
                out.push(ops[i].clone());
                i += 1;
                continue;
            }
            let end = i + ops[i..]
                .iter()
                .take_while(|op| matches!(op, Op::Equal { .. }))
                .count();
            let sandwiched = i > 0 && end < ops.len() && is_edit(&ops[i - 1]) && is_edit(&ops[end]);
            if sandwiched && end - i < min_equal_run {
                for op in &ops[i..end] {
                    if let Op::Equal { a, b } = *op {
                        out.push(Op::Delete { a });
                        out.push(Op::Insert { b });
                    }
                }
                changed = true;
            } else {
                out.extend_from_slice(&ops[i..end]);
            }
            i = end;
        }
        normalize(&mut out);
        *ops = out;
        if !changed {
            return;
        }
    }
}

/// Turns each run of consecutive `Delete`s with contiguous `a` indices that is immediately
/// followed by `Insert`s with contiguous `b` indices into one `Replace`.
pub(crate) fn pair_adjacent(ops: Vec<Op>) -> Vec<Op> {
    let mut out = Vec::with_capacity(ops.len());
    let mut i = 0;
    while i < ops.len() {
        let Op::Delete { a: a0 } = ops[i] else {
            out.push(ops[i].clone());
            i += 1;
            continue;
        };
        let dels = contiguous(&ops[i..], a0, |op| match *op {
            Op::Delete { a } => Some(a),
            _ => None,
        });
        let ins = match ops.get(i + dels) {
            Some(&Op::Insert { b: b0 }) => {
                let n = contiguous(&ops[i + dels..], b0, |op| match *op {
                    Op::Insert { b } => Some(b),
                    _ => None,
                });
                Some((b0, n))
            }
            _ => None,
        };
        match ins {
            Some((b0, n)) => {
                out.push(Op::Replace {
                    a: a0..a0 + dels,
                    b: b0..b0 + n,
                });
                i += dels + n;
            }
            None => {
                out.extend_from_slice(&ops[i..i + dels]);
                i += dels;
            }
        }
    }
    out
}

/// Length of the leading run of `ops` whose indices (per `index`) are `start, start+1, ...`.
fn contiguous(ops: &[Op], start: usize, index: impl Fn(&Op) -> Option<usize>) -> usize {
    ops.iter()
        .zip(start..)
        .take_while(|(op, want)| index(op) == Some(*want))
        .count()
}

/// Pairs leftover single `Delete`s and `Insert`s that share a class into one-item
/// `Replace`s, at most one partner per item, by minimal total cost.
pub(crate) fn pair_cross_hunk(
    ops: Vec<Op>,
    class_a: &[Option<u32>],
    class_b: &[Option<u32>],
    cost: &dyn Fn(usize, usize) -> f64,
) -> Vec<Op> {
    let mut deleted: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    let mut inserted: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for op in &ops {
        match *op {
            Op::Delete { a } => {
                if let Some(c) = class_a[a] {
                    deleted.entry(c).or_default().push(a);
                }
            }
            Op::Insert { b } => {
                if let Some(c) = class_b[b] {
                    inserted.entry(c).or_default().push(b);
                }
            }
            _ => {}
        }
    }
    let mut pairs = Vec::new();
    for (class, rows) in &deleted {
        if let Some(cols) = inserted.get(class) {
            pairs.extend(assign(rows, cols, cost));
        }
    }
    replace_pairs(ops, &pairs, |a, b| Op::Replace {
        a: a..a + 1,
        b: b..b + 1,
    })
}
