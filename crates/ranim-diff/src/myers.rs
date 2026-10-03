//! Myers' greedy O((N+M)·D) algorithm with an O(D²) trace for backtracking (§5.2).

use crate::{Sink, trimmed};

/// Diffs `a` against `b`; `ao`/`bo` are their offsets in the full inputs.
pub(crate) fn diff(a: &[u32], b: &[u32], ao: usize, bo: usize, sink: &mut Sink) {
    trimmed(a, b, ao, bo, sink, middle);
}

fn middle(a: &[u32], b: &[u32], ao: usize, bo: usize, sink: &mut Sink) {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let max = n + m;
    let offset = max + 1;
    // v[offset + k] = furthest x reached on diagonal k = x - y.
    let mut v = vec![0isize; 2 * offset as usize + 1];
    // trace[d] = v restricted to diagonals -d..=d, before round d.
    let mut trace: Vec<Vec<isize>> = Vec::new();

    let mut found = None;
    'outer: for d in 0..=max {
        trace.push(v[(offset - d) as usize..=(offset + d) as usize].to_vec());
        for k in (-d..=d).step_by(2) {
            let i = (offset + k) as usize;
            let mut x = if k == -d || (k != d && v[i - 1] < v[i + 1]) {
                v[i + 1] // down: insertion
            } else {
                v[i - 1] + 1 // right: deletion
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[i] = x;
            if x >= n && y >= m {
                found = Some(d);
                break 'outer;
            }
        }
    }
    let d_final = found.expect("Myers always terminates within N + M rounds");

    // Backtrack from (n, m), collecting ops in reverse.
    let mut rev = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (1..=d_final).rev() {
        let w = &trace[d as usize];
        let at = |k: isize| w[(k + d) as usize];
        let k = x - y;
        let prev_k = if k == -d || (k != d && at(k - 1) < at(k + 1)) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = at(prev_k);
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            x -= 1;
            y -= 1;
            rev.push(Step::Equal(x, y));
        }
        if x == prev_x {
            rev.push(Step::Insert(prev_y));
        } else {
            rev.push(Step::Delete(prev_x));
        }
        x = prev_x;
        y = prev_y;
    }
    while x > 0 && y > 0 {
        x -= 1;
        y -= 1;
        rev.push(Step::Equal(x, y));
    }
    debug_assert!(x == 0 && y == 0);

    for step in rev.into_iter().rev() {
        match step {
            Step::Equal(x, y) => sink.equal(ao + x as usize, bo + y as usize),
            Step::Delete(x) => sink.delete(ao + x as usize),
            Step::Insert(y) => sink.insert(bo + y as usize),
        }
    }
}

enum Step {
    Equal(isize, isize),
    Delete(isize),
    Insert(isize),
}
