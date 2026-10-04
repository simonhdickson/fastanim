//! Patience diff: match items that occur exactly once in both inputs, keep the longest
//! order-preserving chain of them as anchors, and recurse between anchors. Falls back to
//! Myers when there are no unique common items.

use std::collections::HashMap;

use crate::{LINEAR_SPACE_THRESHOLD, Sink, linear, myers, trimmed};

/// Diffs `a` against `b`; `ao`/`bo` are their offsets in the full inputs.
pub(crate) fn diff(a: &[u32], b: &[u32], ao: usize, bo: usize, sink: &mut Sink) {
    trimmed(a, b, ao, bo, sink, middle);
}

fn middle(a: &[u32], b: &[u32], ao: usize, bo: usize, sink: &mut Sink) {
    let anchors = unique_lcs(a, b);
    if anchors.is_empty() {
        if a.len() + b.len() > LINEAR_SPACE_THRESHOLD {
            linear::diff(a, b, ao, bo, sink);
        } else {
            myers::diff(a, b, ao, bo, sink);
        }
        return;
    }
    let (mut pa, mut pb) = (0, 0);
    for (i, j) in anchors {
        diff(&a[pa..i], &b[pb..j], ao + pa, bo + pb, sink);
        sink.equal(ao + i, bo + j);
        (pa, pb) = (i + 1, j + 1);
    }
    diff(&a[pa..], &b[pb..], ao + pa, bo + pb, sink);
}

/// Longest increasing chain of `(i, j)` pairs where `a[i] == b[j]` is unique in both.
fn unique_lcs(a: &[u32], b: &[u32]) -> Vec<(usize, usize)> {
    // key -> (count in a, index in a, count in b, index in b)
    let mut seen: HashMap<u32, (u32, usize, u32, usize)> = HashMap::new();
    for (i, &k) in a.iter().enumerate() {
        let e = seen.entry(k).or_insert((0, i, 0, 0));
        e.0 += 1;
    }
    for (j, &k) in b.iter().enumerate() {
        if let Some(e) = seen.get_mut(&k) {
            e.2 += 1;
            e.3 = j;
        }
    }
    let mut pairs: Vec<(usize, usize)> = seen
        .values()
        .filter(|e| e.0 == 1 && e.2 == 1)
        .map(|e| (e.1, e.3))
        .collect();
    pairs.sort_unstable();

    // Longest increasing subsequence on j, via patience sorting with back-pointers.
    let mut tops: Vec<usize> = Vec::new(); // index into pairs of each pile's top
    let mut back: Vec<Option<usize>> = vec![None; pairs.len()];
    for (p, &(_, j)) in pairs.iter().enumerate() {
        let pile = tops.partition_point(|&t| pairs[t].1 < j);
        back[p] = pile.checked_sub(1).map(|q| tops[q]);
        if pile == tops.len() {
            tops.push(p);
        } else {
            tops[pile] = p;
        }
    }
    let mut chain = Vec::with_capacity(tops.len());
    let mut cur = tops.last().copied();
    while let Some(p) = cur {
        chain.push(pairs[p]);
        cur = back[p];
    }
    chain.reverse();
    chain
}
