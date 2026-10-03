//! SPEC §10 examples 1 and 2 as op-script and endpoint tests for `transform_diff`.

use ranim_core::{AnimationExt, Op, Scene};
use ranim_text::{math_tex, transform_diff};

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
