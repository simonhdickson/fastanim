//! Basic shapes and transforms, rendered as SVG stills (no GPU needed).
//!
//! `cargo run -p fastanim --example shapes -- [out_dir]` writes one SVG every half second.

use std::f64::consts::PI;

use fastanim::prelude::*;

fn construct(s: &mut Scene) {
    let circle = s.add(VState::circle(1.5).stroke(BLUE, 0.06));
    s.play(create(circle).run_time(1.5));

    let square = s.add(VState::square(2.0).stroke(RED, 0.06).shift(LEFT * 4.0));
    s.play(fade_in(square));
    s.play(transform(
        square,
        VState::circle(1.0)
            .fill(RED.with_alpha(0.5))
            .shift(LEFT * 4.0),
    ));
    s.wait(0.5);

    s.play(Parallel(vec![
        Box::new(shift(circle, RIGHT * 3.0)),
        Box::new(rotate(square, PI / 2.0)),
        Box::new(scale(square, 0.5).run_time(2.0)),
    ]));
    s.play(fade_out(square));
    s.remove(square);

    let tri = s.add(VState::polygon(
        &[(0.0, 1.0), (-1.0, -0.7), (1.0, -0.7)].map(Point::from),
    ));
    s.play(Sequence(vec![
        Box::new(spin_in(tri)),
        Box::new(indicate(&[circle, tri])),
        Box::new(move_to(tri, Point::new(-4.0, 0.0)).rate(RateFn::EaseOut(Ease::Back))),
    ]));
    let f = flash(s, Point::new(-4.0, 0.0));
    s.play(f);
    s.play(lagged_start(
        0.5,
        vec![Box::new(uncreate(circle)), Box::new(shrink_to_center(tri))],
    ));
    s.wait(0.5);
}

fn main() -> std::io::Result<()> {
    let out = std::env::args().nth(1).unwrap_or_else(|| "frames".into());
    std::fs::create_dir_all(&out)?;
    let mut scene = Scene::new();
    construct(&mut scene);
    let tl = scene.bake();
    let frames = (tl.duration() * 2.0).round() as u32;
    for i in 0..=frames {
        let svg = to_svg(&tl.eval(i as f32 / 2.0), BLACK);
        std::fs::write(format!("{out}/{i:03}.svg"), svg)?;
    }
    println!("wrote {} frames to {out}/", frames + 1);
    Ok(())
}
