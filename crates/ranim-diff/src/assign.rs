//! Minimum-cost pairing of two index sets: optimal (Hungarian) for small sets, greedy
//! nearest-first for large ones (§5.4).

/// Use the O(n²·m) Hungarian algorithm while the smaller side has at most this many items.
const HUNGARIAN_LIMIT: usize = 64;

/// Pairs items of `rows` with items of `cols`, as many as possible, minimizing total cost.
/// Returns `(row, col)` pairs of the given values, sorted by row.
pub(crate) fn assign(
    rows: &[usize],
    cols: &[usize],
    cost: &dyn Fn(usize, usize) -> f64,
) -> Vec<(usize, usize)> {
    if rows.is_empty() || cols.is_empty() {
        return Vec::new();
    }
    let c = |r: usize, c: usize| sanitize(cost(rows[r], cols[c]));
    let mut pairs: Vec<(usize, usize)> = if rows.len().min(cols.len()) <= HUNGARIAN_LIMIT {
        if rows.len() <= cols.len() {
            hungarian(rows.len(), cols.len(), &c)
        } else {
            hungarian(cols.len(), rows.len(), &|i, j| c(j, i))
                .into_iter()
                .map(|(i, j)| (j, i))
                .collect()
        }
    } else {
        greedy(rows.len(), cols.len(), &c)
    };
    pairs.sort_unstable();
    pairs.into_iter().map(|(r, c)| (rows[r], cols[c])).collect()
}

fn sanitize(x: f64) -> f64 {
    if x.is_nan() {
        1e18
    } else {
        x.clamp(-1e18, 1e18)
    }
}

/// Hungarian algorithm (shortest augmenting paths with potentials) for `n <= m`.
/// Returns `(row, col)` for every row.
fn hungarian(n: usize, m: usize, cost: &dyn Fn(usize, usize) -> f64) -> Vec<(usize, usize)> {
    debug_assert!(n <= m);
    // 1-indexed, column 0 is a sentinel.
    let mut u = vec![0.0f64; n + 1];
    let mut v = vec![0.0f64; m + 1];
    let mut p = vec![0usize; m + 1]; // p[j] = row matched to column j
    let mut way = vec![0usize; m + 1];
    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0;
        let mut minv = vec![f64::INFINITY; m + 1];
        let mut used = vec![false; m + 1];
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let mut delta = f64::INFINITY;
            let mut j1 = 0;
            for j in 1..=m {
                if used[j] {
                    continue;
                }
                let cur = cost(i0 - 1, j - 1) - u[i0] - v[j];
                if cur < minv[j] {
                    minv[j] = cur;
                    way[j] = j0;
                }
                if minv[j] < delta {
                    delta = minv[j];
                    j1 = j;
                }
            }
            for j in 0..=m {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    (1..=m)
        .filter(|&j| p[j] != 0)
        .map(|j| (p[j] - 1, j - 1))
        .collect()
}

/// Repeatedly takes the cheapest remaining pair. Ties break by row, then column.
fn greedy(n: usize, m: usize, cost: &dyn Fn(usize, usize) -> f64) -> Vec<(usize, usize)> {
    let mut all: Vec<(f64, usize, usize)> = (0..n)
        .flat_map(|i| (0..m).map(move |j| (i, j)))
        .map(|(i, j)| (cost(i, j), i, j))
        .collect();
    all.sort_unstable_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
    let (mut row_used, mut col_used) = (vec![false; n], vec![false; m]);
    let mut out = Vec::with_capacity(n.min(m));
    for (_, i, j) in all {
        if !row_used[i] && !col_used[j] {
            row_used[i] = true;
            col_used[j] = true;
            out.push((i, j));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn total(pairs: &[(usize, usize)], cost: &dyn Fn(usize, usize) -> f64) -> f64 {
        pairs.iter().map(|&(i, j)| cost(i, j)).sum()
    }

    #[test]
    fn hungarian_beats_greedy_trap() {
        // Greedy takes (0,0)=1 then must take (1,1)=100; optimal is (0,1)+(1,0) = 4.
        let m = [[1.0, 2.0], [2.0, 100.0]];
        let cost = |i: usize, j: usize| m[i][j];
        let pairs = assign(&[0, 1], &[0, 1], &cost);
        assert_eq!(pairs, vec![(0, 1), (1, 0)]);
        assert_eq!(total(&greedy(2, 2, &cost), &cost), 101.0);
    }

    #[test]
    fn rectangular_both_ways() {
        let cost = |i: usize, j: usize| (i as f64 - j as f64).abs();
        assert_eq!(assign(&[5], &[1, 4, 9], &cost), vec![(5, 4)]);
        assert_eq!(assign(&[1, 4, 9], &[5], &cost), vec![(4, 5)]);
    }

    #[test]
    fn hungarian_is_optimal_on_small_random_matrices() {
        let mut seed = 0x2545F4914F6CDD1Du64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 50) as f64
        };
        for _ in 0..200 {
            let m: Vec<Vec<f64>> = (0..4).map(|_| (0..4).map(|_| next()).collect()).collect();
            let cost = |i: usize, j: usize| m[i][j];
            let got = total(&hungarian(4, 4, &cost), &cost);
            let mut best = f64::INFINITY;
            let mut perm = [0, 1, 2, 3];
            permutations(&mut perm, 0, &mut |p| {
                best = best.min((0..4).map(|i| m[i][p[i]]).sum());
            });
            assert_eq!(got, best);
        }
    }

    fn permutations(p: &mut [usize; 4], k: usize, f: &mut dyn FnMut(&[usize; 4])) {
        if k == p.len() {
            f(p);
            return;
        }
        for i in k..p.len() {
            p.swap(k, i);
            permutations(p, k + 1, f);
            p.swap(k, i);
        }
    }
}
