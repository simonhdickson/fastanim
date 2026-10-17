//! Lottie files evaluate to the shapes and styles they describe, frame by frame.

use fastanim_core::color::Color;
use fastanim_core::kurbo::{Affine, Rect, Vec2};
use fastanim_core::{Animation, Lottie, LottieMobject, Position, Scene, VState, play_lottie};

/// A 100x100, 10 fps, 20-frame composition of `layers`.
fn comp(layers: &str) -> Lottie {
    comp_with(layers, "[]")
}

fn comp_with(layers: &str, assets: &str) -> Lottie {
    let json = format!(
        r#"{{"v":"5.7.4","fr":10,"ip":0,"op":20,"w":100,"h":100,"assets":{assets},"layers":{layers}}}"#
    );
    Lottie::parse(json.as_bytes()).unwrap()
}

/// A shape layer of `shapes`, with transform `ks`.
fn layer(shapes: &str, ks: &str) -> String {
    format!(r#"{{"ty":4,"ind":1,"ip":0,"op":20,"st":0,"ks":{ks},"shapes":{shapes}}}"#)
}

const RECT: &str =
    r#"{"ty":"rc","p":{"a":0,"k":[50,50]},"s":{"a":0,"k":[20,10]},"r":{"a":0,"k":0}}"#;
const RED: &str = r#"{"ty":"fl","c":{"a":0,"k":[1,0,0,1]},"o":{"a":0,"k":100}}"#;

fn bbox(m: &VState) -> Rect {
    m.path.bbox().unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn rect_close(a: Rect, b: Rect) -> bool {
    close(a.x0, b.x0) && close(a.y0, b.y0) && close(a.x1, b.x1) && close(a.y1, b.y1)
}

#[test]
fn reads_the_composition() {
    let l = comp("[]");
    assert_eq!((l.width(), l.height(), l.fps()), (100.0, 100.0, 10.0));
    assert_eq!(l.duration(), 2.0);
    assert!(Lottie::parse(b"{}").unwrap_err().contains("not a Lottie"));
    assert!(Lottie::parse(b"[").is_err());
}

#[test]
fn a_filled_rectangle_in_pixels() {
    let l = comp(&format!("[{}]", layer(&format!("[{RECT},{RED}]"), "{}")));
    let shapes = l.shapes(0.0, Affine::IDENTITY);
    assert_eq!(shapes.len(), 1);
    let m = &shapes[0];
    assert!(rect_close(bbox(m), Rect::new(40.0, 45.0, 60.0, 55.0)));
    assert_eq!(m.fill, Color::rgb(1.0, 0.0, 0.0));
    assert_eq!(m.stroke.width, 0.0);
    assert_eq!(m.opacity, 1.0);
}

#[test]
fn placed_like_an_image() {
    let l = comp(&format!("[{}]", layer(&format!("[{RECT},{RED}]"), "{}")));
    let k = 8.0 / 1080.0;
    // The rectangle sits 10 px left of center to 10 px right, and 5 px above to 5 below.
    let m = &l.shapes(0.0, l.default_placement())[0];
    assert!(rect_close(
        bbox(m),
        Rect::new(-10.0 * k, -5.0 * k, 10.0 * k, 5.0 * k)
    ));

    let lm = LottieMobject::new(l);
    let b = lm.bbox().unwrap();
    assert!(rect_close(
        b,
        Rect::new(-50.0 * k, -50.0 * k, 50.0 * k, 50.0 * k)
    ));
    let moved = lm.scale_to_fit_height(2.0).shift(Vec2::new(1.0, 0.0));
    assert!(rect_close(
        moved.bbox().unwrap(),
        Rect::new(0.0, -1.0, 2.0, 1.0)
    ));
}

#[test]
fn keyframes_ease_and_hold() {
    // Linear from x=0 to x=100 over frames 0..10, then held at 100 until 15, then a jump.
    let ks = r#"{"p":{"a":1,"k":[
        {"t":0,"s":[0,0],"o":{"x":[0],"y":[0]},"i":{"x":[1],"y":[1]}},
        {"t":10,"s":[100,0],"h":1},
        {"t":15,"s":[200,0]}]}}"#;
    let l = comp(&format!("[{}]", layer(&format!("[{RECT},{RED}]"), ks)));
    let x = |f: f64| bbox(&l.shapes(f, Affine::IDENTITY)[0]).center().x;
    assert!(close(x(0.0), 50.0));
    assert!(close(x(5.0), 100.0));
    assert!(close(x(10.0), 150.0));
    assert!(close(x(14.0), 150.0));
    assert!(close(x(15.0), 250.0));

    // An ease-in handle starts slower than linear.
    let ks = r#"{"p":{"a":1,"k":[
        {"t":0,"s":[0,0],"o":{"x":[0.5],"y":[0]},"i":{"x":[1],"y":[1]}},
        {"t":10,"s":[100,0]}]}}"#;
    let l = comp(&format!("[{}]", layer(&format!("[{RECT},{RED}]"), ks)));
    let x = bbox(&l.shapes(5.0, Affine::IDENTITY)[0]).center().x - 50.0;
    assert!(x > 1.0 && x < 50.0, "{x}");
}

#[test]
fn old_style_keyframes_with_ends() {
    let ks = r#"{"o":{"a":1,"k":[
        {"t":0,"s":[0],"e":[100],"o":{"x":[0],"y":[0]},"i":{"x":[1],"y":[1]}},
        {"t":10}]}}"#;
    let l = comp(&format!("[{}]", layer(&format!("[{RECT},{RED}]"), ks)));
    assert!(close(
        f64::from(l.shapes(5.0, Affine::IDENTITY)[0].opacity),
        0.5
    ));
    assert_eq!(l.shapes(15.0, Affine::IDENTITY)[0].opacity, 1.0);
}

#[test]
fn styles_draw_the_paths_above_them() {
    // Stroke above fill in the list draws on top; the fill doesn't see the later ellipse.
    let shapes = format!(
        r#"[{RECT},
        {{"ty":"st","c":{{"a":0,"k":[0,0,1,1]}},"o":{{"a":0,"k":50}},"w":{{"a":0,"k":2}}}},
        {RED},
        {{"ty":"el","p":{{"a":0,"k":[0,0]}},"s":{{"a":0,"k":[10,10]}}}}]"#
    );
    let l = comp(&format!("[{}]", layer(&shapes, "{}")));
    let s = l.shapes(0.0, Affine::IDENTITY);
    assert_eq!(s.len(), 2);
    assert_eq!(s[0].fill, Color::rgb(1.0, 0.0, 0.0));
    assert_eq!(s[1].stroke.color, Color::rgb(0.0, 0.0, 1.0));
    assert_eq!(s[1].stroke.width, 2.0);
    assert_eq!(s[1].opacity, 0.5);
    assert_eq!(s[1].fill, Color::TRANSPARENT);
    assert!(rect_close(bbox(&s[0]), Rect::new(40.0, 45.0, 60.0, 55.0)));
}

#[test]
fn groups_transform_their_items_and_scale_strokes() {
    let group = format!(
        r#"[{{"ty":"gr","it":[{RECT},
          {{"ty":"st","c":{{"a":0,"k":[1,1,1,1]}},"o":{{"a":0,"k":100}},"w":{{"a":0,"k":2}}}},
          {{"ty":"tr","p":{{"a":0,"k":[10,0]}},"a":{{"a":0,"k":[50,50]}},
            "s":{{"a":0,"k":[200,200]}},"r":{{"a":0,"k":0}},"o":{{"a":0,"k":50}}}}]}}]"#
    );
    let l = comp(&format!("[{}]", layer(&group, "{}")));
    let m = &l.shapes(0.0, Affine::IDENTITY)[0];
    // Doubled about its anchor (50, 50), which then sits at (10, 0).
    assert!(rect_close(bbox(m), Rect::new(-10.0, -10.0, 30.0, 10.0)));
    assert_eq!(m.stroke.width, 4.0);
    assert_eq!(m.opacity, 0.5);
}

#[test]
fn layers_stack_and_parent() {
    // Layer 1 (on top) is parented to null layer 2, which moves it 10 px right.
    let child = r#"{"ty":4,"ind":1,"parent":2,"ip":0,"op":20,"ks":{},"shapes":[RECT,BLUE]}"#
        .replace("RECT", RECT)
        .replace(
            "BLUE",
            r#"{"ty":"fl","c":{"a":0,"k":[0,0,1,1]},"o":{"a":0,"k":100}}"#,
        );
    let null = r#"{"ty":3,"ind":2,"ip":0,"op":20,"ks":{"p":{"a":0,"k":[10,0]}}}"#;
    let below = layer(&format!("[{RECT},{RED}]"), "{}").replace(r#""ind":1"#, r#""ind":3"#);
    let l = comp(&format!("[{child},{null},{below}]"));
    let s = l.shapes(0.0, Affine::IDENTITY);
    assert_eq!(s.len(), 2);
    assert_eq!(
        s[0].fill,
        Color::rgb(1.0, 0.0, 0.0),
        "the bottom layer comes first"
    );
    assert!(rect_close(bbox(&s[1]), Rect::new(50.0, 45.0, 70.0, 55.0)));
}

#[test]
fn layers_show_between_their_in_and_out_points() {
    let l = comp(&format!(
        "[{}]",
        layer(&format!("[{RECT},{RED}]"), "{}").replace(r#""ip":0,"op":20"#, r#""ip":5,"op":10"#)
    ));
    for (f, shown) in [(0.0, false), (5.0, true), (9.0, true), (10.0, false)] {
        let m = &l.shapes(f, Affine::IDENTITY)[0];
        assert_eq!(
            m.opacity > 0.0 && m.path.bbox().is_some(),
            shown,
            "frame {f}"
        );
    }
}

#[test]
fn trims_cut_paths() {
    let line = r#"{"ty":"sh","ks":{"a":0,"k":{"c":false,"v":[[0,0],[100,0]],"i":[[0,0],[0,0]],"o":[[0,0],[0,0]]}}}"#;
    let stroke = r#"{"ty":"st","c":{"a":0,"k":[1,1,1,1]},"o":{"a":0,"k":100},"w":{"a":0,"k":1}}"#;
    let trimmed = |s: f64, e: f64, o: f64| {
        let tm = format!(
            r#"{{"ty":"tm","s":{{"a":0,"k":{s}}},"e":{{"a":0,"k":{e}}},"o":{{"a":0,"k":{o}}},"m":1}}"#
        );
        let l = comp(&format!(
            "[{}]",
            layer(&format!("[{line},{tm},{stroke}]"), "{}")
        ));
        l.shapes(0.0, Affine::IDENTITY)[0].path.bbox()
    };
    let b = trimmed(25.0, 75.0, 0.0).unwrap();
    assert!(
        (b.x0 - 25.0).abs() < 0.5 && (b.x1 - 75.0).abs() < 0.5,
        "{b:?}"
    );
    // Offset by a quarter turn: 50%..100%, the start and end swapped.
    let b = trimmed(75.0, 25.0, 90.0).unwrap();
    assert!(
        (b.x0 - 50.0).abs() < 0.5 && (b.x1 - 100.0).abs() < 0.5,
        "{b:?}"
    );
    assert!(trimmed(30.0, 30.0, 0.0).is_none());
    let b = trimmed(0.0, 100.0, 45.0).unwrap();
    assert!(close(b.x0, 0.0) && close(b.x1, 100.0));
}

#[test]
fn precomps_play_their_own_layers_offset_in_time() {
    // The precomp's rectangle moves right from frame 0 of its own time; the layer starts it
    // at frame 5 of the main composition.
    let inner = layer(
        &format!("[{RECT},{RED}]"),
        r#"{"p":{"a":1,"k":[{"t":0,"s":[0,0],"o":{"x":[0],"y":[0]},"i":{"x":[1],"y":[1]}},{"t":10,"s":[10,0]}]}}"#,
    );
    let assets = format!(r#"[{{"id":"inner","layers":[{inner}]}}]"#);
    let pre = r#"{"ty":0,"refId":"inner","ind":1,"ip":0,"op":20,"st":5,"ks":{}}"#;
    let l = comp_with(&format!("[{pre}]"), &assets);
    let x = |f: f64| bbox(&l.shapes(f, Affine::IDENTITY)[0]).center().x;
    assert!(close(x(5.0), 50.0));
    assert!(close(x(10.0), 55.0));

    let looped = r#"[{"id":"a","layers":[{"ty":0,"refId":"a","ks":{}}]}]"#;
    let json = format!(
        r#"{{"fr":10,"ip":0,"op":20,"w":100,"h":100,"assets":{looped},"layers":[{{"ty":0,"refId":"a","ks":{{}}}}]}}"#
    );
    assert!(Lottie::parse(json.as_bytes()).unwrap_err().contains("nest"));
}

#[test]
fn gradients_and_solids() {
    let gf = r#"{"ty":"gf","o":{"a":0,"k":100},"g":{"p":2,"k":{"a":0,"k":[0,1,0,0,1,0,0,1]}}}"#;
    let solid = r##"{"ty":1,"ind":2,"ip":0,"op":20,"ks":{},"sw":100,"sh":100,"sc":"#00ff00"}"##;
    let l = comp(&format!(
        "[{},{solid}]",
        layer(&format!("[{RECT},{gf}]"), "{}")
    ));
    let s = l.shapes(0.0, Affine::IDENTITY);
    assert_eq!(s[0].fill, Color::rgb(0.0, 1.0, 0.0));
    assert!(rect_close(bbox(&s[0]), Rect::new(0.0, 0.0, 100.0, 100.0)));
    assert_eq!(s[1].fill, Color::rgb(0.5, 0.0, 0.5));
}

#[test]
fn plays_from_wherever_it_was_moved() {
    let ks = r#"{"p":{"a":1,"k":[
        {"t":0,"s":[0,0],"o":{"x":[0],"y":[0]},"i":{"x":[1],"y":[1]}},
        {"t":20,"s":[40,0]}]}}"#;
    let l = comp(&format!("[{}]", layer(&format!("[{RECT},{RED}]"), ks)));
    let lm = LottieMobject::new(l)
        .scale_to_fit_height(4.0)
        .shift(Vec2::new(1.0, 0.0));
    let mut s = Scene::new();
    let ids = lm.add_to(&mut s);
    assert_eq!(ids.len(), 2);
    // 100 px is 4 units: the rectangle starts centered on x = 1.
    let x = |s: &fastanim_core::SceneState| bbox(&s[&ids[1]]).center().x;
    assert!(close(x(s.state()), 1.0));

    let mut play = play_lottie(&ids, lm.lottie.clone());
    assert_eq!(play.duration(), 2.0);
    play.plan(s.state());
    let mut st = s.state().clone();
    play.sample(0.5, &mut st);
    assert!(close(x(&st), 1.8), "{}", x(&st));
    play.sample(1.0, &mut st);
    assert!((x(&st) - 2.6).abs() < 1e-3, "the last frame, not past it");

    let mut back = play_lottie(&ids, lm.lottie.clone())
        .frames(20.0, 0.0)
        .loops(2);
    assert_eq!(back.duration(), 4.0);
    back.plan(s.state());
    back.sample(0.25, &mut st);
    assert!(close(x(&st), 1.8));
    back.sample(1.0, &mut st);
    assert!(close(x(&st), 1.0));

    // Moving the whole group first, as an earlier animation in a `Parallel` would, carries
    // the playing animation with it.
    let mut st = s.state().clone();
    let outline = st[&ids[0]].clone();
    st.insert(ids[0], outline.shift(Vec2::new(0.0, 2.0)));
    play.sample(0.5, &mut st);
    assert!(close(x(&st), 1.8));
    assert!(close(bbox(&st[&ids[1]]).center().y, 2.0));
}

#[test]
fn the_example_file_plays() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fastanim-script/scenes/bounce.json"
    );
    let l = Lottie::load(path.as_ref()).unwrap();
    assert_eq!(l.duration(), 2.0);
    let place = l.default_placement();
    let s = l.shapes(30.0, place);
    // Ground stroke, shadow fill, ball fill, ball outline.
    assert_eq!(s.len(), 4);
    assert!(s[0].stroke.width > 0.0 && s[1].fill.a > 0.0);
    let ball = |f: f64| bbox(&l.shapes(f, place)[2]);
    assert!(
        ball(30.0).center().y < ball(0.0).center().y,
        "falls (y is up in the scene)"
    );
    assert!(
        ball(30.0).width() > ball(30.0).height(),
        "squashes on landing"
    );
    let ground = |f: f64| l.shapes(f, place)[0].path.bbox().map_or(0.0, |b| b.width());
    assert!(ground(0.0) < 1e-9 && ground(15.0) > 2.0);
}
