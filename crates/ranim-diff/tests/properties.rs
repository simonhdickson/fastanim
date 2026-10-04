//! Property tests for the correctness requirements in `docs/SPEC.md` §5.7.

use proptest::prelude::*;
use proptest::sample::Index;
use ranim_diff::{
    Algorithm, Cleanup, DiffOptions, Differ, Op, TieBreak, apply, diff, edit_cost, expand, validate,
};

/// Up to `max_len` items over an alphabet of `alphabet` symbols.
fn seq(max_len: usize, alphabet: u8) -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(0..alphabet, 0..=max_len)
}

/// `base` with up to `edits` random deletes, inserts and substitutions.
fn mutated(
    base: impl Strategy<Value = Vec<u8>>,
    edits: usize,
    alphabet: u8,
) -> impl Strategy<Value = (Vec<u8>, Vec<u8>)> {
    let edit = (0..3u8, any::<Index>(), 0..alphabet);
    (base, prop::collection::vec(edit, 0..=edits)).prop_map(|(a, edits)| {
        let mut b = a.clone();
        for (kind, at, x) in edits {
            let at = at.index(b.len() + 1);
            match kind {
                0 if at < b.len() => {
                    b.remove(at);
                }
                1 => b.insert(at, x),
                _ if at < b.len() => b[at] = x,
                _ => {}
            }
        }
        (a, b)
    })
}

/// Two unrelated sequences, or a sequence and a small edit of it (the typical case).
fn pair() -> impl Strategy<Value = (Vec<u8>, Vec<u8>)> {
    prop_oneof![
        (2..8u8).prop_flat_map(|n| (seq(40, n), seq(40, n))),
        mutated(seq(60, 8), 6, 8),
    ]
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

proptest! {
    #[test]
    fn every_script_applies_to_b((a, b) in pair()) {
        for opts in all_options() {
            let ops = Differ::new(&a, &b, |x| *x)
                .options(opts.clone())
                .class(|x| (*x < 3).then_some(()))
                .run();
            let got = apply(&a, &b, &ops, |x| *x)
                .map_err(|e| TestCaseError::fail(format!("{e} with {opts:?}: {ops:?}")))?;
            prop_assert_eq!(&got, &b, "{:?}", opts);
        }
    }

    #[test]
    fn expanded_line_scripts_apply_to_b((a, b) in pair()) {
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
        let (la, lb) = (lines(&a), lines(&b));
        let ka: Vec<_> = la.iter().map(|l| &a[l.clone()]).collect();
        let kb: Vec<_> = lb.iter().map(|l| &b[l.clone()]).collect();
        let outer = Differ::new(&ka, &kb, |l| *l).run();
        let ops = expand(&outer, &la, &lb, |ra, rb| {
            Differ::new(&a[ra], &b[rb], |x| *x).run()
        });
        let got = apply(&a, &b, &ops, |x| *x)
            .map_err(|e| TestCaseError::fail(format!("{e}: {ops:?}")))?;
        prop_assert_eq!(&got, &b);
    }

    #[test]
    fn myers_is_minimal((a, b) in pair()) {
        let want = a.len() + b.len() - 2 * lcs_len(&a, &b);
        for alg in [Algorithm::Myers, Algorithm::MyersLinearSpace] {
            prop_assert_eq!(edit_cost(&diff(&a, &b, |x| *x, &raw(alg))), want, "{:?}", alg);
        }
        prop_assert_eq!(edit_cost(&diff(&a, &b, |x| *x, &stable())), want, "stable");
        let patience = diff(&a, &b, |x| *x, &raw(Algorithm::Patience));
        prop_assert!(edit_cost(&patience) >= want);
    }

    #[test]
    fn deletions_come_before_insertions_in_each_hunk((a, b) in pair()) {
        let variants = [
            raw(Algorithm::Myers),
            raw(Algorithm::MyersLinearSpace),
            raw(Algorithm::Patience),
            stable(),
        ];
        for opts in variants {
            let ops = diff(&a, &b, |x| *x, &opts);
            for w in ops.windows(2) {
                prop_assert!(
                    !matches!(w, [Op::Insert { .. }, Op::Delete { .. }]),
                    "{:?}: {:?}", opts, ops
                );
            }
        }
    }

    #[test]
    fn deterministic((a, b) in pair()) {
        for opts in all_options() {
            prop_assert_eq!(diff(&a, &b, |x| *x, &opts), diff(&a, &b, |x| *x, &opts));
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    #[test]
    fn myers_is_minimal_up_to_200_items(
        (a, b) in (2..22u8).prop_flat_map(|n| (seq(200, n), seq(200, n)))
    ) {
        let want = a.len() + b.len() - 2 * lcs_len(&a, &b);
        for alg in [Algorithm::Myers, Algorithm::MyersLinearSpace] {
            prop_assert_eq!(edit_cost(&diff(&a, &b, |x| *x, &raw(alg))), want);
        }
        prop_assert_eq!(edit_cost(&diff(&a, &b, |x| *x, &stable())), want);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1))]

    #[test]
    fn large_inputs_use_linear_space(
        (a, b) in mutated(prop::collection::vec(0..64u8, 20_000), 200, 64)
    ) {
        let ops = diff(&a, &b, |x| *x, &DiffOptions::default());
        prop_assert_eq!(&apply(&a, &b, &ops, |x| *x).unwrap(), &b);
        // 20k × 20k is past the stable limit; the trimmed middle is not.
        let linear = diff(&a, &b, |x| *x, &raw(Algorithm::MyersLinearSpace));
        let auto = diff(&a, &b, |x| *x, &raw(Algorithm::Myers));
        let stable = diff(&a, &b, |x| *x, &stable());
        prop_assert_eq!(edit_cost(&linear), edit_cost(&auto));
        prop_assert_eq!(edit_cost(&linear), edit_cost(&stable));
    }
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
