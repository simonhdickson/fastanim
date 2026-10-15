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

#[test]
fn sine_curve_bakes() {
    // A port of manim's SineCurveUnitCircle; there is no Rust twin, so just check it runs.
    let t = script("sine_curve");
    assert_eq!(t.duration(), 9.5);
    t.eval(t.duration());
}

#[test]
fn sin_and_cos_bakes() {
    // A port of manim's SinAndCosFunctionPlot: axes, numbers, labels and plots on them.
    let t = script("sin_and_cos");
    assert_eq!(t.duration(), 1.0);
    assert!(t.eval(0.0).len() > 20);
}

#[test]
fn growing_wave_bakes() {
    // f(t) = sin(2πt)·t over its first 20 seconds, after a 2 s draw-in.
    let t = script("growing_wave");
    assert_eq!(t.duration(), 23.0);
    t.eval(10.0);
    t.eval(t.duration());
}

#[test]
fn fit_axes_and_default_plot_range() {
    // x² over [-3, 3] fits y to [0, 10], so y = 10 is the top of 6-tall axes.
    let src = r#"
        let ax = fit_axes(|x| x * x, [-3, 3]);
        if ax.c2p(0.0, 10.0).y != 3.0 { throw "fit"; }
        scene.add(ax.plot(|x| x * x));
    "#;
    fastanim_script::bake(src).unwrap();
    let e = err("fit_axes(|x| { throw \"boom\" }, [0, 1])");
    assert!(e.message.contains("boom"), "{e}");
}

/// A directory holding `tone.wav`, two seconds of silence.
fn wav_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("fastanim-script-sound");
    std::fs::create_dir_all(&dir).unwrap();
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 8000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(dir.join("tone.wav"), spec).unwrap();
    (0..16000).for_each(|_| w.write_sample(0i16).unwrap());
    w.finalize().unwrap();
    dir
}

#[test]
fn sounds_play_and_wait() {
    let src = r#"
        let a = audio("tone.wav");
        if a.duration != 2.0 { throw "duration"; }
        let c = scene.add(circle(1.0));
        scene.play(sound(a));
        scene.play([create(c), sound(a, 0.5, 1.0)]);
        scene.add_sound(a, 1.0, 2.0);
        scene.wait(0.25);
        scene.wait_for_sounds();
        scene.play(sequence([sound(a, 0.0, 0.5), fade_out(c)]));
    "#;
    let tl = fastanim_script::bake_in(src, &wav_dir()).unwrap();
    // 2 s of sound, 1 s of `create`, waiting on the background sound to 4 s, then 1.5 s more.
    assert_eq!(tl.duration(), 5.5);
    let got: Vec<_> = (tl.sounds().iter())
        .map(|e| (e.at, e.sound.offset, e.sound.duration))
        .collect();
    assert_eq!(
        got,
        [
            (0.0, 0.0, 2.0),
            (2.0, 0.5, 0.5),
            (3.0, 1.0, 1.0),
            (4.0, 0.0, 0.5),
        ]
    );
}

#[test]
fn bad_sounds_fail() {
    let dir = wav_dir();
    let e = |src: &str| match fastanim_script::bake_in(src, &dir) {
        Ok(_) => panic!("expected an error from {src:?}"),
        Err(e) => e.message,
    };
    assert!(e(r#"audio("nope.wav")"#).contains("nope.wav"));
    let m = e(r#"scene.play(sound(audio("tone.wav"), 1.5, 0.5))"#);
    assert!(m.contains("must lie within 0..2"), "{m}");
    let m = e(r#"scene.add_sound(audio("tone.wav"), 0.0, 3.0)"#);
    assert!(m.contains("must lie within"), "{m}");
}
