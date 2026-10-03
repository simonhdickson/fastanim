//! SPEC §11 for `ranim-core`: interpolation endpoints, alignment invariants, timeline `eval`.

use ranim_core::color::{BLUE, RED, WHITE};
use ranim_core::*;

fn shape_pairs() -> Vec<(VState, VState)> {
    let three_holes = VState::new(VPath {
        subpaths: [0.0, 1.0, 2.0]
            .iter()
            .flat_map(|&x| {
                VPath::arc(0.3, 0.0, std::f64::consts::TAU)
                    .transform(kurbo::Affine::translate((x, 0.0)))
                    .subpaths
            })
            .collect(),
    });
    vec![
        (VState::square(2.0), VState::circle(1.0).fill(BLUE)),
        (
            VState::circle(1.0),
            VState::line(Point::new(-1.0, 0.0), Point::new(1.0, 1.0)),
        ),
        (
            VState::polygon(&[(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)].map(Point::from)),
            three_holes.clone(),
        ),
        (three_holes, VState::square(1.0).stroke(RED, 0.1)),
    ]
}

#[test]
fn transform_endpoints_and_alignment() {
    for (a, b) in shape_pairs() {
        let (pa, pb) = align(&a.path, &b.path);
        assert_eq!(pa.subpaths.len(), pb.subpaths.len());
        for (x, y) in pa.subpaths.iter().zip(&pb.subpaths) {
            assert_eq!(x.segments.len(), y.segments.len());
        }

        let mut s = Scene::new();
        let id = s.add(a.clone());
        s.play(transform(id, b.clone()));
        let tl = s.bake();
        assert_eq!(tl.eval(0.0)[&id], a);
        assert_eq!(tl.eval(1.0)[&id], b);
        let mid = &tl.eval(0.5)[&id].path;
        assert_eq!(mid.subpaths.len(), pa.subpaths.len());
    }
}

#[test]
fn color_lerp_endpoints() {
    assert_eq!(Color::lerp(&RED, &BLUE, 0.0), RED);
    assert_eq!(Color::lerp(&RED, &BLUE, 1.0), BLUE);
    let mid = Color::lerp(&WHITE, &WHITE, 0.5);
    assert!((mid.r - 1.0).abs() < 1e-4 && (mid.g - 1.0).abs() < 1e-4);
}

#[test]
fn trim_halves_arc_length() {
    let sq = VPath::polyline(
        &[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].map(Point::from),
        true,
    );
    let half = sq.trim(0.0..0.5);
    let end = half.subpaths.last().unwrap().segments.last().unwrap().p3;
    assert!((end - Point::new(1.0, 1.0)).hypot() < 1e-6, "{end:?}");
    assert!(sq.trim(0.5..0.5).subpaths.is_empty());
}

fn demo() -> (BakedTimeline, MobjectId, MobjectId) {
    let mut s = Scene::new();
    let c = s.add(VState::circle(1.0));
    s.play(create(c));
    let d = s.add(VState::dot(ORIGIN));
    s.wait(1.0);
    s.marker("shift");
    s.play(Parallel(vec![
        Box::new(shift(c, RIGHT * 2.0)),
        Box::new(fade_out(d).run_time(0.5).rate(RateFn::Linear)),
    ]));
    s.remove(d);
    s.wait(0.5);
    (s.bake(), c, d)
}

#[test]
fn timeline_boundaries() {
    let (tl, c, d) = demo();
    assert_eq!(tl.duration(), 3.5);
    assert_eq!(tl.marker("shift"), Some(2.0));
    assert_eq!(tl.eval(0.0)[&c].draw_range, 0.0..0.0);
    assert_eq!(tl.eval(1.0)[&c].draw_range, 0.0..1.0);
    // Added right before the wait: visible during it.
    assert!(tl.eval(1.5).contains_key(&d));
    assert_eq!(tl.eval(2.25)[&d].opacity, 0.5);
    assert_eq!(tl.eval(2.5)[&d].opacity, 0.0);
    assert_eq!(tl.eval(3.0)[&c].path.center(), Point::new(2.0, 0.0));
    assert!(!tl.eval(3.0).contains_key(&d));
    assert_eq!(tl.eval(99.0), tl.eval(3.5));
}

#[test]
fn eval_independent_of_order() {
    let (tl, ..) = demo();
    let times: Vec<f32> = (0..=35).map(|i| i as f32 / 10.0).collect();
    let forward: Vec<_> = times.iter().map(|&t| tl.eval(t)).collect();
    let backward: Vec<_> = times.iter().rev().map(|&t| tl.eval(t)).collect();
    assert!(forward.iter().eq(backward.iter().rev()));
}

#[test]
fn svg_still() {
    let (tl, ..) = demo();
    let svg = to_svg(&tl.eval(0.5), Color::hex(0x000000));
    assert!(svg.starts_with("<svg"));
    assert_eq!(svg.matches("<path").count(), 1);
    assert!(svg.contains("stroke=\"#ffffff\""));
    // Fully transparent: skipped.
    assert_eq!(to_svg(&tl.eval(2.6), WHITE).matches("<path").count(), 1);
}
