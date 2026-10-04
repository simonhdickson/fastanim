//! The example scripts bake to the same timelines as their Rust versions (SPEC §14.6), and
//! bad scripts fail with a located error instead of panicking or hanging.

use fastanim_core::color::BLACK;
use fastanim_core::{BakedTimeline, Scene, to_svg};

#[allow(dead_code)]
#[path = "../../fastanim-bevy/examples/diff.rs"]
mod diff;
#[allow(dead_code)]
#[path = "../../fastanim-bevy/examples/parity.rs"]
mod parity;

fn rust(construct: fn(&mut Scene)) -> BakedTimeline {
    let mut s = Scene::new();
    construct(&mut s);
    s.bake()
}

fn script(name: &str) -> BakedTimeline {
    let path = format!("{}/scenes/{name}.rhai", env!("CARGO_MANIFEST_DIR"));
    let src = std::fs::read_to_string(&path).unwrap();
    fastanim_script::bake(&src).unwrap_or_else(|e| panic!("{path}:{e}"))
}

fn assert_same(a: &BakedTimeline, b: &BakedTimeline) {
    assert_eq!(a.duration(), b.duration());
    assert_eq!(a.markers(), b.markers());
    let n = (a.duration() * 10.0) as usize;
    for t in (0..=n).map(|i| i as f32 / 10.0) {
        assert!(
            to_svg(&a.eval(t), BLACK) == to_svg(&b.eval(t), BLACK),
            "differs at {t}s"
        );
    }
}

#[test]
fn parity_matches_rust() {
    assert_same(&rust(parity::construct), &script("parity"));
}

#[test]
fn diff_matches_rust() {
    assert_same(&rust(diff::construct), &script("diff"));
}

fn err(src: &str) -> fastanim_script::Error {
    match fastanim_script::bake(src) {
        Ok(_) => panic!("expected an error from {src:?}"),
        Err(e) => e,
    }
}

#[test]
fn errors_are_located() {
    let e = err("let c = scene.add(circle(1.0));\nscene.play(create(c);");
    assert_eq!(e.line, 2, "{e}");

    let e = err("\n  scene.add(math_tex(\"a^\"));");
    assert_eq!((e.line, e.col), (2, 13), "{e}");

    let e = err("let t = scene.add(text(\"x\"));\nscene.always(t, |s, t| s[t]);");
    assert!(e.message.contains("expected a shape"), "{e}");

    let e = err("let c = scene.add(circle(1.0));\nscene.always(c, |s, t| 1);");
    assert!(e.message.contains("not a shape"), "{e}");

    let e = err("let c = scene.add(circle(1.0));\nscene.remove(c);\nscene.play(create(c));");
    assert!(e.message.contains("not in the scene"), "{e}");

    let e = err("let c = scene.add(circle(1.0));\nscene.play(apply_function(c, |p| p.z));");
    assert_eq!(e.line, 2, "{e}");

    // Fails one way or another (Rhai's own lock on the captured variable, or ours) rather
    // than deadlocking.
    let e = err("let c = scene.add(circle(1.0));\nscene.play(update(c, |s, a| scene.get(c)));");
    assert_eq!(e.line, 2, "{e}");
}

#[test]
fn runaway_scripts_stop() {
    let e = err("loop {}");
    assert!(e.message.contains("Too many operations"), "{e}");
    let e = err("fn f(x) { f(x) } f(1)");
    assert!(e.message.to_lowercase().contains("stack"), "{e}");
}
