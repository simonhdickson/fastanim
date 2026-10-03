//! Randomized property tests for the correctness requirements in `docs/SPEC.md` §5.7.
//!
//! Uses a small seeded PRNG so failures are reproducible from the printed seed.

use ranim_diff::{
    Algorithm, Cleanup, DiffOptions, Differ, Op, TieBreak, apply, diff, edit_cost, expand, validate,
};

/// xorshift64*: tiny, deterministic, good enough for test inputs.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// A sequence of up to `max_len` items over an alphabet of `alphabet` symbols.
    fn seq(&mut self, max_len: u64, alphabet: u64) -> Vec<u8> {
        let len = self.below(max_len + 1);
        (0..len).map(|_| self.below(alphabet) as u8).collect()
    }

    /// `base` with a few random edits, the typical "small change" case.
    fn mutate(&mut self, base: &[u8], edits: u64, alphabet: u64) -> Vec<u8> {
        let mut out = base.to_vec();
        for _ in 0..self.below(edits + 1) {
            let at = self.below(out.len() as u64 + 1) as usize;
            match self.below(3) {
                0 if at < out.len() => {
                    out.remove(at);
                }
                1 => out.insert(at, self.below(alphabet) as u8),
                _ if at < out.len() => out[at] = self.below(alphabet) as u8,
                _ => {}
            }
        }
        out
    }
}

fn lcs_len(a: &[u8], b: &[u8]) -> usize {
    let mut row = vec![0usize; b.len() + 1];
    for x in a {
        let mut diag = 0;
        for (j, y) in b.iter().enumerate() {
            let up = row[j + 1];
            row[j + 1] = if x == y { diag + 1 } else { up.max(row[j]) };
            diag = up;
        }
    }
    row[b.len()]
}

fn all_options() -> Vec<DiffOptions> {
    let mut v = Vec::new();
    for algorithm in [
        Algorithm::Myers,
        Algorithm::MyersLinearSpace,
        Algorithm::Patience,
    ] {
        for tie_break in [TieBreak::Stable, TieBreak::Myers] {
            for detect_moves in [false, true] {
                for pair_replacements in [false, true] {
                    for cleanup in [Cleanup::None, Cleanup::Semantic { min_equal_run: 3 }] {
                        v.push(DiffOptions {
                            algorithm,
                            tie_break,
                            detect_moves,
                            cleanup,
                            pair_replacements,
                        });
                    }
                }
            }
        }
    }
    v
}

fn raw(algorithm: Algorithm) -> DiffOptions {
    DiffOptions {
        algorithm,
        tie_break: TieBreak::Myers,
        ..DiffOptions::raw()
    }
}

fn stable() -> DiffOptions {
    DiffOptions::raw()
}

fn inputs(seed: u64, cases: usize, mut f: impl FnMut(&[u8], &[u8])) {
    let mut rng = Rng(seed);
    for case in 0..cases {
        let (a, b) = if case % 2 == 0 {
            let alphabet = 2 + rng.below(6);
            (rng.seq(40, alphabet), rng.seq(40, alphabet))
        } else {
            let a = rng.seq(60, 8);
            let b = rng.mutate(&a, 6, 8);
            (a, b)
        };
        f(&a, &b);
    }
}

#[test]
fn every_script_applies_to_b() {
    let options = all_options();
    inputs(1, 400, |a, b| {
        for opts in &options {
            let ops = Differ::new(a, b, |x| *x)
                .options(opts.clone())
                .class(|x| (*x < 3).then_some(()))
                .run();
            let got = apply(a, b, &ops, |x| *x)
                .unwrap_or_else(|e| panic!("{e} for {a:?} -> {b:?} with {opts:?}: {ops:?}"));
            assert_eq!(got, b, "{a:?} -> {b:?} with {opts:?}");
        }
    });
}

#[test]
fn expanded_line_scripts_apply_to_b() {
    // Item 0 ends a line.
    let lines = |s: &[u8]| {
        let mut out = vec![];
        let mut start = 0;
        for (i, x) in s.iter().enumerate() {
            if *x == 0 || i + 1 == s.len() {
                out.push(start..i + 1);
                start = i + 1;
            }
        }
        out
    };
    inputs(2, 400, |a, b| {
        let (la, lb) = (lines(a), lines(b));
        let ka: Vec<_> = la.iter().map(|l| &a[l.clone()]).collect();
        let kb: Vec<_> = lb.iter().map(|l| &b[l.clone()]).collect();
        let outer = Differ::new(&ka, &kb, |l| *l).run();
        let ops = expand(&outer, &la, &lb, |ra, rb| {
            Differ::new(&a[ra], &b[rb], |x| *x).run()
        });
        let got =
            apply(a, b, &ops, |x| *x).unwrap_or_else(|e| panic!("{e} for {a:?} -> {b:?}: {ops:?}"));
        assert_eq!(got, b);
    });
}

#[test]
fn myers_is_minimal() {
    inputs(2, 400, |a, b| {
        let want = a.len() + b.len() - 2 * lcs_len(a, b);
        for alg in [Algorithm::Myers, Algorithm::MyersLinearSpace] {
            let ops = diff(a, b, |x| *x, &raw(alg));
            assert_eq!(edit_cost(&ops), want, "{alg:?}: {a:?} -> {b:?}");
        }
        let ops = diff(a, b, |x| *x, &stable());
        assert_eq!(edit_cost(&ops), want, "stable: {a:?} -> {b:?}");
        let patience = diff(a, b, |x| *x, &raw(Algorithm::Patience));
        assert!(edit_cost(&patience) >= want);
    });
}

#[test]
fn myers_is_minimal_up_to_200_items() {
    let mut rng = Rng(3);
    for _ in 0..40 {
        let alphabet = 2 + rng.below(20);
        let a = rng.seq(200, alphabet);
        let b = rng.seq(200, alphabet);
        let want = a.len() + b.len() - 2 * lcs_len(&a, &b);
        for alg in [Algorithm::Myers, Algorithm::MyersLinearSpace] {
            assert_eq!(edit_cost(&diff(&a, &b, |x| *x, &raw(alg))), want);
        }
        assert_eq!(edit_cost(&diff(&a, &b, |x| *x, &stable())), want);
    }
}

#[test]
fn deletions_come_before_insertions_in_each_hunk() {
    inputs(4, 300, |a, b| {
        let variants = [
            raw(Algorithm::Myers),
            raw(Algorithm::MyersLinearSpace),
            raw(Algorithm::Patience),
            stable(),
        ];
        for opts in variants {
            let ops = diff(a, b, |x| *x, &opts);
            for w in ops.windows(2) {
                assert!(
                    !matches!(w, [Op::Insert { .. }, Op::Delete { .. }]),
                    "{opts:?}: {ops:?}"
                );
            }
        }
    });
}

#[test]
fn deterministic() {
    inputs(5, 100, |a, b| {
        for opts in all_options() {
            assert_eq!(diff(a, b, |x| *x, &opts), diff(a, b, |x| *x, &opts));
        }
    });
}

#[test]
fn large_inputs_use_linear_space() {
    let mut rng = Rng(6);
    let a: Vec<u8> = (0..20_000).map(|_| rng.below(64) as u8).collect();
    let b = rng.mutate(&a, 200, 64);
    let ops = diff(&a, &b, |x| *x, &DiffOptions::default());
    assert_eq!(apply(&a, &b, &ops, |x| *x).unwrap(), b);
    // 20k × 20k is past the stable limit; the trimmed middle is not.
    let linear = diff(&a, &b, |x| *x, &raw(Algorithm::MyersLinearSpace));
    let auto = diff(&a, &b, |x| *x, &raw(Algorithm::Myers));
    let stable = diff(&a, &b, |x| *x, &stable());
    assert_eq!(edit_cost(&linear), edit_cost(&auto));
    assert_eq!(edit_cost(&linear), edit_cost(&stable));
}

#[test]
fn validate_rejects_bad_scripts() {
    assert!(validate(&[Op::Insert { b: 1 }, Op::Insert { b: 0 }], 0, 2).is_err());
    assert!(validate(&[Op::Delete { a: 0 }, Op::Delete { a: 0 }], 1, 0).is_err());
    assert!(validate(&[Op::Delete { a: 0 }], 2, 0).is_err());
    assert!(validate(&[], 0, 1).is_err());
    assert!(validate(&[Op::Equal { a: 1, b: 0 }, Op::Equal { a: 0, b: 1 }], 2, 2).is_err());
    let a = [1, 2];
    let b = [3, 4];
    let mismatched = [Op::Equal { a: 0, b: 0 }, Op::Move { a: 1, b: 1 }];
    assert!(apply(&a, &b, &mismatched, |x| *x).is_err());
}
