//! SPEC §10 example 6, "classic manim parity", minus `Write(Text)` until text lands in M5:
//! `Create(Circle)`, `Transform(square, circle)`, and `sin(x)` on axes with a dot driven by an
//! updater.
//!
//! `cargo run -p ranim-bevy --example parity`

use std::f64::consts::PI;

use ranim_core::color::{BLUE, GREEN, RED, YELLOW};
use ranim_core::*;

fn construct(s: &mut Scene) {
    let circle = s.add(VState::circle(1.5).stroke(BLUE, 0.06));
    s.play(create(circle).run_time(1.5));
    let square = s.add(VState::square(2.0).stroke(RED, 0.06).shift(LEFT * 4.0));
    s.play(fade_in(square));
    s.play(transform(
        square,
        VState::circle(1.0)
            .fill(RED.with_alpha(0.5))
            .stroke(RED, 0.06)
            .shift(LEFT * 4.0),
    ));
    s.wait(0.5);
    s.play(Parallel(vec![
        Box::new(fade_out(circle)),
        Box::new(fade_out(square)),
    ]));
    s.remove(circle);
    s.remove(square);

    s.marker("graph");
    let axes = s.add(VState::axes(-6.5..6.5, -2.0..2.0));
    s.play(create(axes));
    let graph =
        s.add(VState::function_graph(f64::sin, -2.0 * PI..2.0 * PI, 24).stroke(GREEN, 0.05));
    s.play(create(graph).run_time(2.0));

    let dot = s.add(VState::dot(Point::new(-2.0 * PI, 0.0)).fill(YELLOW));
    s.play(fade_in(dot).run_time(0.5));
    // One full sweep left to right in 4 s, then back: pure in t, so scrubbing works.
    s.always(dot, move |st, t| {
        let phase = f64::from(t) / 4.0 % 2.0;
        let x = 2.0 * PI * (2.0 * phase.min(2.0 - phase) - 1.0);
        st[&dot].clone().move_to(Point::new(x, x.sin()))
    });
    s.wait(8.0);
}

fn main() {
    let mut s = Scene::new();
    construct(&mut s);
    ranim_bevy::preview(s.bake());
}
