//! Myers' linear-space variant: find the middle snake with simultaneous forward and reverse
//! searches, then recurse on both halves (§5.3). O((N+M)·D) time, O(N+M) memory.

use crate::{Sink, myers, trimmed};

/// Diffs `a` against `b`; `ao`/`bo` are their offsets in the full inputs.
pub(crate) fn diff(a: &[u32], b: &[u32], ao: usize, bo: usize, sink: &mut Sink) {
    trimmed(a, b, ao, bo, sink, middle);
}

fn middle(a: &[u32], b: &[u32], ao: usize, bo: usize, sink: &mut Sink) {
    let snake = middle_snake(a, b);
    if snake.d <= 1 {
        // At most one edit: the greedy algorithm's trace is tiny.
        myers::diff(a, b, ao, bo, sink);
        return;
    }
    // With D >= 2 both halves contain at least one edit, so each is strictly smaller.
    let (xs, ys, xe, ye) = (snake.x_start, snake.y_start, snake.x_end, snake.y_end);
    diff(&a[..xs], &b[..ys], ao, bo, sink);
    for i in 0..xe - xs {
        sink.equal(ao + xs + i, bo + ys + i);
    }
    diff(&a[xe..], &b[ye..], ao + xe, bo + ye, sink);
}

struct Snake {
    d: usize,
    x_start: usize,
    y_start: usize,
    x_end: usize,
    y_end: usize,
}

/// Finds a snake lying on the middle of some shortest edit path.
fn middle_snake(a: &[u32], b: &[u32]) -> Snake {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let delta = n - m;
    let odd = delta % 2 != 0;
    let max = (n + m + 1) / 2;
    let offset = max + 1;
    let size = 2 * offset as usize + 1;
    // vf[offset + k]: furthest forward x on diagonal k.
    // vb[offset + k]: furthest x in the *reversed* inputs on reversed diagonal k.
    let mut vf = vec![0isize; size];
    let mut vb = vec![0isize; size];

    for d in 0..=max {
        for k in (-d..=d).step_by(2) {
            let i = (offset + k) as usize;
            let mut x = if k == -d || (k != d && vf[i - 1] < vf[i + 1]) {
                vf[i + 1]
            } else {
                vf[i - 1] + 1
            };
            let mut y = x - k;
            let (x0, y0) = (x, y);
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            vf[i] = x;
            // Reversed diagonal of the same line is delta - k; the reverse search has
            // completed d - 1 rounds.
            let kr = delta - k;
            if odd && kr.abs() < d && vf[i] + vb[(offset + kr) as usize] >= n {
                return Snake {
                    d: (2 * d - 1) as usize,
                    x_start: x0 as usize,
                    y_start: y0 as usize,
                    x_end: x as usize,
                    y_end: y as usize,
                };
            }
        }
        for k in (-d..=d).step_by(2) {
            let i = (offset + k) as usize;
            let mut x = if k == -d || (k != d && vb[i - 1] < vb[i + 1]) {
                vb[i + 1]
            } else {
                vb[i - 1] + 1
            };
            let mut y = x - k;
            let (x0, y0) = (x, y);
            while x < n && y < m && a[(n - 1 - x) as usize] == b[(m - 1 - y) as usize] {
                x += 1;
                y += 1;
            }
            vb[i] = x;
            let kf = delta - k;
            if !odd && kf.abs() <= d && vb[i] + vf[(offset + kf) as usize] >= n {
                return Snake {
                    d: (2 * d) as usize,
                    x_start: (n - x) as usize,
                    y_start: (m - y) as usize,
                    x_end: (n - x0) as usize,
                    y_end: (m - y0) as usize,
                };
            }
        }
    }
    unreachable!("the forward and reverse searches always meet within ⌈(N+M)/2⌉ rounds")
}
