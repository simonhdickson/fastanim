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
    assert_eq!(a.background(), b.background());
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
fn check_samples_closures() {
    // `always` tries the updater at t = 0, where it works, so only sampling finds this.
    let src = "let c = scene.add(circle(1.0));
        scene.always(c, |s, t| if t < 0.5 { s[c] } else { 1 });
        scene.wait(1.0);";
    let dir = std::path::Path::new("");
    assert!(fastanim_script::bake_in(src, dir).is_ok());
    assert!(fastanim_script::check_in(src, dir, 0.0).is_ok());
    let e = match fastanim_script::check_in(src, dir, 30.0) {
        Ok(_) => panic!("expected an error from {src:?}"),
        Err(e) => e,
    };
    assert!(e.message.starts_with("at 0.50s: "), "{e}");
    assert!(e.message.contains("not a shape"), "{e}");

    let tl = fastanim_script::check_in(
        &std::fs::read_to_string(format!(
            "{}/scenes/growing_wave.rhai",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
        dir,
        30.0,
    )
    .unwrap();
    assert_eq!(tl.duration(), 23.0);
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

#[test]
fn moving_group_lands_on_destination() {
    // A port of manim's MovingGroupToDestination: the group's third dot ends on the yellow one.
    let t = script("moving_group");
    let end = t.eval(t.duration());
    let centers: Vec<_> = (end.values())
        .map(|m| m.path.center())
        .filter(|c| (c.x - 4.0).abs() < 1e-9 && (c.y - 3.0).abs() < 1e-9)
        .collect();
    assert_eq!(centers.len(), 2);
}

#[test]
fn groups_nest_arrange_and_animate() {
    let src = r#"
        let row = group([square(1.0), group([circle(0.5), text("a")]), dot(ORIGIN)])
            .arrange(RIGHT, 0.5)
            .to_edge(LEFT);
        let g = scene.add(row);
        if g.len() != 3 || g[1].len() != 2 { throw "len"; }
        let all = group([g, scene.add(circle(1.0))]);
        if scene.get(g[0]).center().x > -5.0 { throw "arranged"; }
        scene.play(fade_in(all));
        scene.play([rotate(g, PI), scale(g[1], 2.0)]);
        scene.play(indicate(all));
        if scene.get(g[0]).center().x < scene.get(g[2]).center().x { throw "rotated as one"; }
        scene.play(transform_diff(g[1][1], text("b")));
        scene.play(fade_out(g[-1]));
        scene.remove(all);
        scene.get(g[0]);
    "#;
    let e = err(src);
    assert!(e.message.contains("not in the scene"), "{e}");

    let e = err("let g = scene.add(group([circle(1.0)]));\nscene.play(transform(g, circle(2.0)));");
    assert!(e.message.contains("got a group"), "{e}");
    let e = err("let g = scene.add(group([circle(1.0)]));\ng[1];");
    assert!(e.message.contains("out of range"), "{e}");
    let e = err("group([scene.add(circle(1.0)), circle(1.0)]);");
    assert!(e.message.contains("all mobjects"), "{e}");
}

#[test]
fn events_are_labelled() {
    let src = r#"
        let t = scene.add(text("hi"));
        let c = scene.add(circle(1.0));
        scene.play(write(t).named("title"));
        scene.play([fade_in(c), rotate(t, PI)]);
        scene.play(create(t).run_time(2.0));
    "#;
    let tl = fastanim_script::bake(src).unwrap();
    let labels: Vec<_> = tl.events().iter().map(|e| e.label.as_str()).collect();
    assert_eq!(labels, ["title", "FadeIn + Rotate", "Create"]);
    assert_eq!(tl.events()[2].end, 4.0);
}

#[test]
fn gradient_image_bakes() {
    let svg = to_svg(&script("gradient_image").eval(0.5), BLACK);
    assert_eq!(svg.matches("<image").count(), 1);
}

#[test]
fn images_from_files_and_pixels() {
    let dir = std::env::temp_dir().join("fastanim-script-image");
    std::fs::create_dir_all(&dir).unwrap();
    let px = fastanim_core::Image::from_rgba(1, 1, vec![255, 0, 0, 255]).unwrap();
    std::fs::write(dir.join("red.png"), px.to_png()).unwrap();
    let src = r#"
        let a = scene.add(image("red.png").pixelated().scale_to_fit_height(2.0));
        let b = scene.add(image([[0, [255, 0, 0]], [[0, 0, 255, 128], 255.0]]).next_to(scene.get(a), RIGHT));
        scene.play(rotate(group([a, b]), 1.0));
        scene.play(fade_out(b));
    "#;
    let tl = fastanim_script::bake_in(src, &dir).unwrap();
    assert_eq!(to_svg(&tl.eval(1.0), BLACK).matches("<image").count(), 2);
    assert_eq!(to_svg(&tl.eval(2.0), BLACK).matches("<image").count(), 1);

    let e = |src: &str| match fastanim_script::bake_in(src, &dir) {
        Ok(_) => panic!("expected an error from {src:?}"),
        Err(e) => e.message,
    };
    assert!(e(r#"image("missing.png");"#).contains("missing.png"));
    assert!(e("image([[1, 2], [3]]);").contains("row 1 has 1 pixels"));
    assert!(e("image([[[1, 2]]]);").contains("1, 3 or 4 values"));
    assert!(e("image([]);").contains("0x0"));
}

#[test]
fn lottie_scene_plays() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scenes");
    let src = std::fs::read_to_string(dir.join("lottie.rhai")).unwrap();
    let tl = fastanim_script::bake_in(&src, &dir).unwrap();
    // Fade in, two 2 s loops, 2 s moving, fade out.
    assert_eq!(tl.duration(), 8.0);
    let at = |t: f32| to_svg(&tl.eval(t), BLACK);
    assert_ne!(at(1.5), at(2.0), "it animates");

    let e = |src: &str| match fastanim_script::bake_in(src, &dir) {
        Ok(_) => panic!("expected an error from {src:?}"),
        Err(e) => e.message,
    };
    let c = "let c = scene.add(circle(1.0));";
    assert!(e(&format!("{c}scene.play(play_lottie(c));")).contains("expected a Lottie"));
    let l = r#"let l = scene.add(lottie("bounce.json"));"#;
    assert!(e(&format!("{l}scene.play(play_lottie(l, #{{loops: 0}}));")).contains("loops"));
    let speed = format!("{l}scene.play(play_lottie(l, #{{speed: 2.0}}));");
    assert!(e(&speed).contains("unknown option"));
    assert!(e(r#"lottie("missing.json");"#).contains("missing.json"));
    assert!(e(r#"lottie("lottie.rhai");"#).contains("lottie.rhai"));
}

#[test]
fn text_in_other_fonts() {
    let tl = fastanim_script::bake(
        r#"scene.add(text("mono", #{font: "DejaVu Sans Mono"}).to_edge(UP));
           scene.add(code("let x = 1;", "rust", #{font: "DejaVu Sans Mono"}));
           scene.add(math_tex("x^2", #{font: "New Computer Modern Math"}));"#,
    );
    assert!(tl.is_ok(), "{:?}", tl.err());
    let e = |src: &str| err(src).message;
    assert!(e(r#"text("x", #{font: "No Such Font"});"#).contains("load_font"));
    assert!(e(r#"text("x", #{size: 2.0});"#).contains("unknown text option"));
    assert!(e(r#"load_font("no/such/font.ttf");"#).contains("no/such/font.ttf"));
}

#[test]
fn fades_shift_and_scale() {
    let src = r#"
        let c = scene.add(circle(1.0));
        scene.play(fade_in(c, RIGHT * 2.0));
        let g = scene.add(group([circle(1.0).shift(LEFT), circle(1.0).shift(RIGHT)]));
        scene.play(fade_out(g, UP, 2.0));
    "#;
    let tl = fastanim_script::bake(src).unwrap();
    let (start, end) = (tl.eval(0.0), tl.eval(tl.duration()));
    let at = |s: &fastanim_core::SceneState, i, x, y| {
        let p = s[&fastanim_core::MobjectId(i)].path.center();
        assert!(
            (p - fastanim_core::Point::new(x, y)).hypot() < 1e-9,
            "{i}: {p:?}"
        );
    };
    // fade_in starts transparent, `v` back from where it ends.
    assert_eq!(start[&fastanim_core::MobjectId(0)].opacity, 0.0);
    at(&start, 0, -2.0, 0.0);
    // fade_out scales the group about its joint center, so its parts spread apart.
    assert_eq!(end[&fastanim_core::MobjectId(1)].opacity, 0.0);
    at(&end, 0, 0.0, 0.0);
    at(&end, 1, -2.0, 1.0);
    at(&end, 2, 2.0, 1.0);
}

#[test]
fn square_to_circle_ends_pink_and_faded() {
    use fastanim_core::MobjectId;
    let tl = script("square_to_circle");
    let n = tl.duration();
    let sq = |t: f32| tl.eval(t)[&MobjectId(0)].clone();
    let pink = fastanim_core::color::PINK.with_alpha(0.5);
    assert_eq!(sq(n * 2.0 / 3.0).fill, pink);
    assert_eq!(sq(n).opacity, 0.0);
}

#[test]
fn styles_scene_renders_every_style() {
    let tl = script("styles");
    assert_eq!(tl.background(), fastanim_core::Color::hex(0x1c1c28));
    let svg = to_svg(&tl.eval(tl.duration()), tl.background());
    assert!(svg.contains(r##"fill="#1c1c28""##));
    // The title's glyphs, the ring and the card.
    assert!(svg.matches("<linearGradient").count() > 2);
    for attr in [
        r#"stroke-linecap="butt""#,
        r#"stroke-linecap="square""#,
        r#"stroke-linejoin="miter""#,
        r#"stroke-linejoin="bevel""#,
    ] {
        assert!(svg.contains(attr), "{attr}");
    }
}

#[test]
fn bad_styles_fail() {
    let e = err(r#"scene.add(line(LEFT, RIGHT).line_cap("pointy"));"#);
    assert!(e.message.contains("unknown line cap"), "{e}");
    let e = err(r#"scene.add(square(1.0).line_join("x"));"#);
    assert!(e.message.contains("unknown line join"), "{e}");
    let e = err("scene.add(square(1.0).fill_gradient([RED, 1.0]));");
    assert!(e.message.contains("expected a color"), "{e}");
}

#[test]
fn groups_and_texts_color_by_gradient() {
    use fastanim_core::color::{BLUE, RED};
    let tl = fastanim_script::bake(
        r#"
        scene.add(group([square(1.0), square(1.0).fill(WHITE, 0.5), square(1.0)])
            .set_color_by_gradient([RED, BLUE]));
        "#,
    )
    .unwrap();
    let s = tl.eval(0.0);
    let m = |i| &s[&fastanim_core::MobjectId(i)];
    assert_eq!((m(0).stroke.color, m(2).stroke.color), (RED, BLUE));
    // Opacities stay: the first square is unfilled, the second half filled.
    assert_eq!(m(0).fill.a, 0.0);
    assert_eq!(m(1).fill.a, 0.5);
    assert_ne!(m(1).stroke.color, RED);
}
