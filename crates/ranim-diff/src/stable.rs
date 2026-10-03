//! Stable tie-breaking: among all *minimal* edit scripts, choose the one that keeps
//! unchanged items where they are.
//!
//! Myers' algorithm finds a shortest edit script, but inputs often have several, and which
//! one it returns is an accident of its search order. For animation the choice matters:
//! `a + b = c → b + a = c` should keep `+ = c` still and swap the letters, not keep `b`
//! and send both `a` and `+` travelling.
//!
//! This is an O(N·M) dynamic program over the edit graph that compares alignments by:
//!
//! 1. most `Equal` items (so the edit cost is the same as Myers'),
//! 2. least total **block shift**: each maximal run of consecutive `Equal`s is a rigid block,
//!    costing how far it moves, `|i - j|`, once per block,
//! 3. fewest blocks,
//! 4. least total per-item displacement `Σ |i - j|`.
//!
//! Remaining ties go to deletions before insertions. Used only while `N · M` is at most
//! [`STABLE_LIMIT`](crate::STABLE_LIMIT).

use std::cmp::Ordering;

use crate::Sink;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Score {
    matches: u32,
    shift: u64,
    blocks: u32,
    drift: u64,
}

impl Score {
    const ZERO: Score = Score {
        matches: 0,
        shift: 0,
        blocks: 0,
        drift: 0,
    };

    /// Unreachable state; loses to everything reachable.
    const NONE: Score = Score {
        matches: 0,
        shift: u64::MAX,
        blocks: u32::MAX,
        drift: u64::MAX,
    };

    fn better_than(self, other: Score) -> bool {
        self.cmp_quality(other) == Ordering::Greater
    }

    fn cmp_quality(self, other: Score) -> Ordering {
        self.matches
            .cmp(&other.matches)
            .then(other.shift.cmp(&self.shift))
            .then(other.blocks.cmp(&self.blocks))
            .then(other.drift.cmp(&self.drift))
    }
}

// Back-pointers, packed into one byte per cell.
// Gap state (bits 0..2): how we reached (i, j) without matching a[i-1] with b[j-1].
const GAP_FROM_DEL_GAP: u8 = 0;
const GAP_FROM_DEL_MATCH: u8 = 1;
const GAP_FROM_INS_GAP: u8 = 2;
const GAP_FROM_INS_MATCH: u8 = 3;
// Match state (bit 2): whether the diagonal step continues a block.
const MATCH_CONTINUES: u8 = 1 << 2;

/// Diffs `a` against `b` (both non-empty); `ao`/`bo` are their offsets in the full inputs.
pub(crate) fn diff(a: &[u32], b: &[u32], ao: usize, bo: usize, sink: &mut Sink) {
    let (n, m) = (a.len(), b.len());
    let w = m + 1;
    let mut back = vec![0u8; (n + 1) * w];
    // Two rows of (gap, match) scores.
    let mut prev_gap = vec![Score::NONE; w];
    let mut prev_match = vec![Score::NONE; w];
    let mut cur_gap = vec![Score::NONE; w];
    let mut cur_match = vec![Score::NONE; w];

    // Shift is measured in global indices so sub-problems agree with the full input.
    let shift = |i: usize, j: usize| (ao + i).abs_diff(bo + j) as u64;

    for i in 0..=n {
        for j in 0..=m {
            let cell = i * w + j;
            // Gap state.
            let (mut best, mut how) = if i == 0 && j == 0 {
                (Score::ZERO, GAP_FROM_DEL_GAP)
            } else {
                (Score::NONE, GAP_FROM_DEL_GAP)
            };
            let mut consider = |s: Score, h: u8| {
                if s != Score::NONE && (best == Score::NONE || s.better_than(best)) {
                    best = s;
                    how = h;
                }
            };
            // Deletion candidates first, so they win ties.
            if i > 0 {
                consider(prev_gap[j], GAP_FROM_DEL_GAP);
                consider(prev_match[j], GAP_FROM_DEL_MATCH);
            }
            if j > 0 {
                consider(cur_gap[j - 1], GAP_FROM_INS_GAP);
                consider(cur_match[j - 1], GAP_FROM_INS_MATCH);
            }
            cur_gap[j] = best;
            let mut bits = how;

            // Match state.
            cur_match[j] = Score::NONE;
            if i > 0 && j > 0 && a[i - 1] == b[j - 1] {
                let cont = prev_match[j - 1];
                let start = prev_gap[j - 1];
                let d = shift(i - 1, j - 1);
                let cont = (cont != Score::NONE).then(|| Score {
                    matches: cont.matches + 1,
                    drift: cont.drift + d,
                    ..cont
                });
                let start = (start != Score::NONE).then(|| Score {
                    matches: start.matches + 1,
                    shift: start.shift + d,
                    blocks: start.blocks + 1,
                    drift: start.drift + d,
                });
                cur_match[j] = match (cont, start) {
                    (Some(c), Some(s)) if !s.better_than(c) => {
                        bits |= MATCH_CONTINUES;
                        c
                    }
                    (Some(c), None) => {
                        bits |= MATCH_CONTINUES;
                        c
                    }
                    (_, Some(s)) => s,
                    (None, None) => Score::NONE,
                };
            }
            back[cell] = bits;
        }
        std::mem::swap(&mut prev_gap, &mut cur_gap);
        std::mem::swap(&mut prev_match, &mut cur_match);
    }

    // After the final swap, `prev_*` holds row n.
    let mut in_match = prev_match[m] != Score::NONE && prev_match[m].better_than(prev_gap[m]);
    let mut rev = Vec::with_capacity(n + m);
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        let bits = back[i * w + j];
        if in_match {
            rev.push((Some(i - 1), Some(j - 1)));
            in_match = bits & MATCH_CONTINUES != 0;
            i -= 1;
            j -= 1;
        } else {
            match bits & 0b11 {
                GAP_FROM_DEL_GAP | GAP_FROM_DEL_MATCH => {
                    rev.push((Some(i - 1), None));
                    in_match = bits & 0b11 == GAP_FROM_DEL_MATCH;
                    i -= 1;
                }
                _ => {
                    rev.push((None, Some(j - 1)));
                    in_match = bits & 0b11 == GAP_FROM_INS_MATCH;
                    j -= 1;
                }
            }
        }
    }
    for step in rev.into_iter().rev() {
        match step {
            (Some(x), Some(y)) => sink.equal(ao + x, bo + y),
            (Some(x), None) => sink.delete(ao + x),
            (None, Some(y)) => sink.insert(bo + y),
            (None, None) => unreachable!(),
        }
    }
}
