//! SPEC §10 examples 1 and 2 as op-script and endpoint tests for `transform_diff`.

use fastanim_core::{AnimationExt, Op, Scene};
use fastanim_text::{math_tex, transform_diff};

fn count(ops: &[Op], f: fn(&Op) -> bool) -> usize {
    ops.iter().filter(|op| f(op)).count()
}

/// Plays `from → to` over 1 s and checks the scene ends exactly on `to`'s glyphs.
fn play(from: &str, to: &str) -> Vec<Op> {
    let mut s = Scene::new();
    let mut g = math_tex(from).add_to(&mut s);
    let target = math_tex(to);
    let d = transform_diff(&mut s, &mut g, &target);
    let ops = d.ops().to_vec();
    s.play(d.run_time(1.0));
    let ends: Vec<_> = g.ids.iter().map(|id| s.state()[id].clone()).collect();
    assert_eq!(ends, target.glyphs, "ends on the target");
    assert_eq!(s.state().len(), target.glyphs.len(), "deleted glyphs left");
    assert_eq!(g.parts, target.parts());
    ops
}

#[test]
fn pythagoras_rearrangement() {
    // Appendix B: =a =² ~(+→−) ↷b ↷² == =c =²
    let ops = play("a^2 + b^2 = c^2", "a^2 = c^2 - b^2");
    assert_eq!(count(&ops, |o| matches!(o, Op::Move { .. })), 2, "{ops:?}");
    assert!(ops.contains(&Op::Replace { a: 2..3, b: 5..6 }), "{ops:?}");
    assert_eq!(count(&ops, |o| matches!(o, Op::Equal { .. })), 5, "{ops:?}");

    // a = sqrt(...) deletes a's square and grows the radical in.
    let ops = play("a^2 = c^2 - b^2", "a = sqrt(c^2 - b^2)");
    assert_eq!(
        count(&ops, |o| matches!(o, Op::Delete { .. })),
        1,
        "{ops:?}"
    );
    assert_eq!(
        count(&ops, |o| matches!(o, Op::Insert { .. })),
        2,
        "{ops:?}"
    );
}

#[test]
fn inserts_start_invisible_and_deletes_leave() {
    let mut s = Scene::new();
    let mut g = math_tex("x^2").add_to(&mut s);
    let old = g.ids.clone();
    let d = transform_diff(&mut s, &mut g, &math_tex("x^2 + 1"));
    let new: Vec<_> = g
        .ids
        .iter()
        .filter(|id| !old.contains(id))
        .copied()
        .collect();
    assert_eq!(new.len(), 2);
    s.play(d);
    let tl = s.bake();
    for id in &new {
        assert_eq!(tl.eval(0.0)[id].opacity, 0.0);
        assert_eq!(tl.eval(1.0)[id].opacity, 1.0);
    }

    let mut s = Scene::new();
    let mut g = math_tex("x^2 + 1").add_to(&mut s);
    let gone = g.ids[2..].to_vec();
    let d = transform_diff(&mut s, &mut g, &math_tex("x^2"));
    s.play(d);
    let tl = s.bake();
    assert!(gone.iter().all(|id| tl.eval(0.5).contains_key(id)));
    assert!(gone.iter().all(|id| !tl.eval(1.0).contains_key(id)));
}

#[test]
fn commutativity_swaps_on_opposite_sides() {
    let mut s = Scene::new();
    let mut g = math_tex("a + b = b + a").add_to(&mut s);
    let start = s.state().clone();
    let d = transform_diff(&mut s, &mut g, &math_tex("b + a = a + b"));
    assert_eq!(
        count(d.ops(), |o| matches!(o, Op::Move { .. })),
        4,
        "{:?}",
        d.ops()
    );
    s.play(d);
    let mid = s.bake().eval(0.5);
    let d = |i: usize| mid[&g.ids[i]].path.center() - start[&g.ids[i]].path.center();
    // Whatever moves right is above the baseline mid-flight, whatever moves left below.
    for i in [0, 2, 4, 6] {
        assert!(d(i).x.abs() > 0.1, "glyph {i} moves");
        assert!(d(i).y * d(i).x > 0.0, "glyph {i}: {:?}", d(i));
    }
}

/// SPEC §10 example 3: extracting a function.
const BEFORE: &str = r#"fn main() {
    let scores = vec![3, 9, 4, 7];
    let mut total = 0;
    for s in &scores {
        total += s;
    }
    let mean = total as f64 / scores.len() as f64;
    println!("mean = {mean}");
}"#;

/// After extracting `mean`.
const AFTER: &str = r#"fn mean(scores: &[i32]) -> f64 {
    let mut total = 0;
    for s in scores {
        total += s;
    }
    total as f64 / scores.len() as f64
}

fn main() {
    let scores = vec![3, 9, 4, 7];
    let mean = mean(&scores);
    println!("mean = {mean}");
}"#;

#[test]
fn code_refactor() {
    use fastanim_text::code;
    let mut s = Scene::new();
    let mut g = code(BEFORE, "rust").add_to(&mut s);
    let target = code(AFTER, "rust");
    let d = transform_diff(&mut s, &mut g, &target);
    let ops = d.ops().to_vec();
    s.play(d);
    let ends: Vec<_> = g.ids.iter().map(|id| s.state()[id].clone()).collect();
    assert_eq!(ends, target.glyphs);
    assert_eq!(g.lines, target.lines);

    // The loop body keeps its tokens: `total += s ;` survives as equal or moved tokens.
    let kept = count(&ops, |o| matches!(o, Op::Equal { .. } | Op::Move { .. }));
    assert!(kept >= 40, "{kept} kept: {ops:?}");
    // The new signature writes in rather than being morphed from unrelated tokens.
    let inserted = count(&ops, |o| matches!(o, Op::Insert { .. }));
    assert!(inserted >= 10, "{inserted} inserted: {ops:?}");
}

#[test]
fn bubble_sort_swaps_are_two_moves() {
    use fastanim_text::{list, transform_list};
    let mut v = vec![5, 1, 4, 2, 8];
    let mut s = Scene::new();
    let mut g = list(&v).add_to(&mut s);
    for i in 0..v.len() {
        for j in 0..v.len() - 1 - i {
            if v[j] > v[j + 1] {
                v.swap(j, j + 1);
                let d = transform_list(&mut s, &mut g, &list(&v));
                let mut moves: Vec<_> = (d.ops().iter())
                    .filter(|o| matches!(o, Op::Move { .. }))
                    .collect();
                moves.sort_by_key(|o| format!("{o:?}"));
                let want = [Op::Move { a: j, b: j + 1 }, Op::Move { a: j + 1, b: j }];
                assert_eq!(moves, [&want[0], &want[1]], "{:?}", d.ops());
                s.play(d);
            }
        }
    }
    assert_eq!(v, [1, 2, 4, 5, 8]);
    assert_eq!(g.parts, list(&v).parts());
}
