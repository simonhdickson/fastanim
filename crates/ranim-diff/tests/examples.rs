//! The worked examples from `docs/SPEC.md`.

use ranim_diff::{Cleanup, DiffOptions, Differ, Op, diff};

fn chars(s: &str) -> Vec<char> {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn is_operator(c: &char) -> Option<()> {
    "+-=*/".contains(*c).then_some(())
}

/// Renders a script in the notation used by the spec.
fn render<T: std::fmt::Display>(a: &[T], b: &[T], ops: &[Op]) -> String {
    let join = |items: &[T]| items.iter().map(ToString::to_string).collect::<String>();
    ops.iter()
        .map(|op| match op {
            Op::Equal { b: j, .. } => format!("={}", b[*j]),
            Op::Delete { a: i } => format!("-[{}]", a[*i]),
            Op::Insert { b: j } => format!("+[{}]", b[*j]),
            Op::Move { b: j, .. } => format!("↷{}", b[*j]),
            Op::Replace { a: r, b: s } => {
                format!("~({}→{})", join(&a[r.clone()]), join(&b[s.clone()]))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn section_2_insertion_keeps_the_original_plus() {
    let (a, b) = (chars("a + b = c"), chars("a + b + d = c"));
    let ops = diff(&a, &b, |c| *c, &DiffOptions::raw());
    assert_eq!(render(&a, &b, &ops), "=a =+ =b +[+] +[d] == =c");
}

#[test]
fn section_5_4_swap_becomes_moves() {
    let (a, b) = (chars("a + b = c"), chars("b + a = c"));
    let ops = diff(&a, &b, |c| *c, &DiffOptions::default());
    // `+ = c` stay put; `a` and `b` swap.
    assert_eq!(render(&a, &b, &ops), "↷b =+ ↷a == =c");
    assert!(ops.contains(&Op::Move { a: 0, b: 2 }));
    assert!(ops.contains(&Op::Move { a: 2, b: 0 }));
}

#[test]
fn appendix_b_pythagoras() {
    // Tokens keyed by (text, script-level); superscripts are level 1.
    let tok = |s: &str| -> Vec<(char, u8)> {
        let mut out = Vec::new();
        let mut sup = false;
        for c in s.chars().filter(|c| !c.is_whitespace()) {
            if c == '^' {
                sup = true;
                continue;
            }
            out.push((c, sup as u8));
            sup = false;
        }
        out
    };
    let a = tok("a^2 + b^2 = c^2");
    let b = tok("a^2 = c^2 - b^2");
    let show = |t: &[(char, u8)]| -> Vec<String> {
        t.iter()
            .map(|&(c, s)| {
                if s == 1 && c == '2' {
                    "²".into()
                } else {
                    c.to_string()
                }
            })
            .collect()
    };
    let (sa, sb) = (show(&a), show(&b));

    let myers = diff(&a, &b, |t| *t, &DiffOptions::raw());
    assert_eq!(
        render(&sa, &sb, &myers),
        "=a =² -[+] -[b] -[²] == =c =² +[-] +[b] +[²]"
    );

    let ops = Differ::new(&a, &b, |t| *t)
        .class(|t| is_operator(&t.0))
        .run();
    assert_eq!(render(&sa, &sb, &ops), "=a =² == =c =² ~(+→-) ↷b ↷²");
    assert!(ops.contains(&Op::Move { a: 3, b: 6 }));
    assert!(ops.contains(&Op::Move { a: 4, b: 7 }));
    assert!(ops.contains(&Op::Replace { a: 2..3, b: 5..6 }));
}

#[test]
fn example_2_commutativity() {
    let (a, b) = (chars("a + b = b + a"), chars("b + a = a + b"));
    let ops = Differ::new(&a, &b, |c| *c).class(is_operator).run();
    let moves = ops
        .iter()
        .filter(|op| matches!(op, Op::Move { .. }))
        .count();
    assert_eq!(moves, 4, "{}", render(&a, &b, &ops));
    // Only the letters travel: `+`, `=` and `+` stay put.
    for op in &ops {
        if let Op::Move { a: i, .. } = op {
            assert!(a[*i].is_alphabetic());
        }
    }
}

#[test]
fn adjacent_hunk_becomes_replace() {
    let (a, b) = (chars("x^2 + 1"), chars("x^3 + 1"));
    let ops = diff(&a, &b, |c| *c, &DiffOptions::default());
    assert_eq!(render(&a, &b, &ops), "=x =^ ~(2→3) =+ =1");
}

#[test]
fn semantic_cleanup_folds_lonely_equals() {
    let (a, b) = (chars("abcxdef"), chars("uvwxyz"));
    let opts = DiffOptions {
        cleanup: Cleanup::Semantic { min_equal_run: 2 },
        ..DiffOptions::default()
    };
    let ops = diff(&a, &b, |c| *c, &opts);
    assert_eq!(render(&a, &b, &ops), "~(abcxdef→uvwxyz)");

    let without = diff(&a, &b, |c| *c, &DiffOptions::default());
    assert_eq!(render(&a, &b, &without), "~(abc→uvw) =x ~(def→yz)");
}

#[test]
fn moves_pair_by_visual_cost() {
    // Two deleted `x`s and two inserted `x`s: with a cost that prefers crossing pairs,
    // the assignment follows the cost, not the index order.
    let a = ['x', 'x', 'q', 'q', 'q'];
    let b = ['q', 'q', 'q', 'x', 'x'];
    let ops = Differ::new(&a, &b, |c| *c)
        .cost(|i, j| if i + 3 == j { 10.0 } else { 1.0 })
        .run();
    assert!(ops.contains(&Op::Move { a: 0, b: 4 }));
    assert!(ops.contains(&Op::Move { a: 1, b: 3 }));
}
