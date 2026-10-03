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

#[test]
fn updaters_follow_time_and_order() {
    let mut s = Scene::new();
    let lead = s.add(VState::dot(Point::ORIGIN));
    let follow = s.add(VState::dot(Point::ORIGIN));
    s.wait(1.0);
    s.always(lead, move |st, t| {
        st[&lead].clone().move_to(Point::new(f64::from(t), 0.0))
    });
    s.always(follow, move |st, _| {
        st[&follow].clone().move_to(st[&lead].path.center() + UP)
    });
    s.marker("go");
    s.wait(2.0);
    let tl = s.bake();
    assert_eq!(tl.markers(), &[("go".to_owned(), 1.0)]);
    let at = |t: f32, id| tl.eval(t)[&id].path.center();
    assert!(
        (at(0.5, lead) - Point::ORIGIN).hypot() < 1e-9,
        "not active before registration"
    );
    assert!((at(2.5, lead) - Point::new(1.5, 0.0)).hypot() < 1e-9);
    assert!((at(2.5, follow) - Point::new(1.5, 1.0)).hypot() < 1e-9);
}

#[test]
fn function_graph_hits_samples() {
    let g = VState::function_graph(f64::sin, -3.0..3.0, 12);
    let segs = &g.path.subpaths[0].segments;
    assert_eq!(segs.len(), 12);
    for s in segs {
        assert!((s.p0.y - s.p0.x.sin()).abs() < 1e-12);
        let mid = kurbo::ParamCurve::eval(s, 0.5);
        assert!((mid.y - mid.x.sin()).abs() < 1e-3, "smooth between samples");
    }
}

#[test]
fn write_staggers_and_ends_exactly() {
    let mut s = Scene::new();
    let glyphs: Vec<_> = (0..5)
        .map(|i| s.add(VState::square(0.5).fill(WHITE).shift(RIGHT * f64::from(i))))
        .collect();
    let end = s.state().clone();
    s.play(write(&glyphs));
    let tl = s.bake();
    assert_eq!(tl.eval(tl.duration()), end);
    // Early on, the first glyph is tracing its outline and the last hasn't started.
    let early = tl.eval(0.1);
    let (first, last) = (&early[&glyphs[0]], &early[&glyphs[4]]);
    assert!(first.draw_range.end > 0.0 && first.stroke.width > 0.0 && first.fill.a == 0.0);
    assert_eq!(last.draw_range.end, 0.0);
}

#[test]
fn rate_fns_hit_endpoints() {
    use Ease::*;
    let eases = [Quad, Cubic, Expo, Back];
    let fns = (eases.iter())
        .flat_map(|&e| [RateFn::EaseIn(e), RateFn::EaseOut(e), RateFn::EaseInOut(e)])
        .chain([
            RateFn::Linear,
            RateFn::Smooth,
            RateFn::Spring {
                stiffness: 20.0,
                damping: 6.0,
            },
        ]);
    for f in fns {
        assert_eq!(f.apply(0.0), 0.0, "{f:?}");
        assert_eq!(f.apply(1.0), 1.0, "{f:?}");
    }
    assert!(RateFn::EaseIn(Back).apply(0.2) < 0.0, "pulls back");
    let spring = RateFn::Spring {
        stiffness: 20.0,
        damping: 6.0,
    };
    assert!(
        (0..100).any(|i| spring.apply(i as f32 / 100.0) > 1.0),
        "overshoots"
    );
}

#[test]
fn creation_and_removal_endpoints() {
    let sq = VState::square(2.0).shift(RIGHT);
    let center = |m: &VState| m.path.center();
    // (animation, ends where it started)
    let anims = [
        (grow_from_center as fn(_) -> _, true),
        (spin_in, true),
        (shrink_to_center, false),
        (uncreate, false),
    ];
    for (f, restores) in anims {
        let mut s = Scene::new();
        let id = s.add(sq.clone());
        s.play(f(id));
        let tl = s.bake();
        let (start, end) = (tl.eval(0.0)[&id].clone(), tl.eval(1.0)[&id].clone());
        assert!((center(&start) - center(&sq)).hypot() < 1e-9);
        assert!((center(&end) - center(&sq)).hypot() < 1e-9);
        assert_eq!(end == sq, restores);
    }
    let mut s = Scene::new();
    let id = s.add(sq.clone());
    s.play(shrink_to_center(id));
    s.play(uncreate(id));
    let end = &s.state()[&id];
    assert!(end.path.bbox().unwrap().area() < 1e-12);
    assert_eq!(end.draw_range, 0.0..0.0);
}

#[test]
fn move_to_and_apply_function() {
    let mut s = Scene::new();
    let id = s.add(VState::square(1.0));
    s.play(move_to(id, Point::new(3.0, -1.0)));
    assert!((s.state()[&id].path.center() - Point::new(3.0, -1.0)).hypot() < 1e-9);
    s.play(apply_function(id, |p| Point::new(p.x * 2.0, p.y)));
    let b = s.state()[&id].path.bbox().unwrap();
    assert!((b.width() - 2.0).abs() < 1e-9 && (b.height() - 1.0).abs() < 1e-9);
}

#[test]
fn emphasis_returns_to_start() {
    let mut s = Scene::new();
    let ids: Vec<_> = (0..3)
        .map(|i| s.add(VState::square(0.5).fill(WHITE).shift(RIGHT * f64::from(i))))
        .collect();
    let start = s.state().clone();
    s.play(indicate(&ids));
    s.play(wiggle(&ids));
    let tl = s.bake();
    assert_eq!(tl.duration(), 3.0);
    assert_eq!(tl.eval(1.0), start);
    assert_eq!(tl.eval(3.0), start);
    // Midway through `indicate`, the group scales about its joint center: the middle square
    // stays put and the outer ones spread.
    let mid = tl.eval(0.5);
    assert!((mid[&ids[1]].path.center() - start[&ids[1]].path.center()).hypot() < 1e-9);
    assert!(mid[&ids[2]].path.center().x > start[&ids[2]].path.center().x + 0.1);
    assert_ne!(mid[&ids[0]].fill, start[&ids[0]].fill);
    assert_ne!(tl.eval(1.5), start, "wiggling");
}

#[test]
fn overlays_leave_the_scene() {
    let mut s = Scene::new();
    let eq = s.add(VState::square(1.0));
    let n = s.state().len();
    let c = circumscribe(&mut s, &[eq]);
    s.play(c);
    let f = flash(&mut s, Point::ORIGIN);
    s.play(f);
    assert_eq!(s.state().len(), n);
    let tl = s.bake();
    let mid = tl.eval(0.5);
    assert_eq!(mid.len(), n + 1);
    let rect = mid.values().last().unwrap();
    assert_eq!(rect.draw_range, 0.0..1.0, "fully traced halfway");
    let b = rect.path.bbox().unwrap();
    assert!((b.width() - 1.4).abs() < 1e-9, "buffered around the target");
    assert_eq!(tl.eval(1.5).len(), n + 12);
    assert_eq!(tl.eval(2.0).len(), n);
}

#[test]
fn sequence_and_lagged_start() {
    let mut s = Scene::new();
    let a = s.add(VState::dot(Point::ORIGIN));
    let b = s.add(VState::dot(Point::ORIGIN));
    s.play(Sequence(vec![
        Box::new(shift(a, RIGHT).rate(RateFn::Linear)),
        Box::new(shift(a, UP).run_time(2.0).rate(RateFn::Linear)),
    ]));
    s.play(lagged_start(
        0.5,
        vec![
            Box::new(fade_out(a).rate(RateFn::Linear)),
            Box::new(fade_out(b).rate(RateFn::Linear)),
        ],
    ));
    let tl = s.bake();
    assert_eq!(tl.duration(), 3.0 + 1.5);
    let at = |t: f32| tl.eval(t)[&a].path.center();
    assert!((at(0.5) - Point::new(0.5, 0.0)).hypot() < 1e-6);
    assert!(
        (at(2.0) - Point::new(1.0, 0.5)).hypot() < 1e-6,
        "second plans after first"
    );
    assert!((at(3.0) - Point::new(1.0, 1.0)).hypot() < 1e-9);
    let mid = tl.eval(3.75);
    assert_eq!((mid[&a].opacity, mid[&b].opacity), (0.25, 0.75));
    assert_eq!(tl.eval(4.5)[&b].opacity, 0.0);
}

#[test]
fn unwrite_reverses_write() {
    let mut s = Scene::new();
    let glyphs: Vec<_> = (0..5)
        .map(|i| s.add(VState::square(0.5).fill(WHITE).shift(RIGHT * f64::from(i))))
        .collect();
    let start = s.state().clone();
    s.play(unwrite(&glyphs));
    let tl = s.bake();
    assert_eq!(tl.eval(0.0), start);
    let early = tl.eval(0.1);
    assert!(
        early[&glyphs[4]].fill.a < early[&glyphs[0]].fill.a,
        "last glyph goes first"
    );
    let end = tl.eval(tl.duration());
    assert!(glyphs.iter().all(|g| end[g].draw_range.end == 0.0));
}
