// manim's SineCurveUnitCircle example (contributed by heejin_park,
// https://infograph.tistory.com/230): a dot circles the unit circle and traces sin.
// manim accumulates state in updaters; here every frame is pure in t, so scrubbing works.
//
// fastanim preview fastanim-script/scenes/sine_curve.rhai

let origin = point(-4.0, 0.0);
let curve_start = -3.0;
let rate = 0.25; // turns per second
let run = 8.5;

scene.add(line(point(-6.0, 0.0), point(6.0, 0.0)));
scene.add(line(point(-4.0, -2.0), point(-4.0, 2.0)));
let labels = ["pi", "2 pi", "3 pi", "4 pi"];
for i in 0..labels.len() {
    scene.add(math_tex(labels[i]).move_to(point(-1.0 + 2.0 * i.to_float(), -0.5)));
}
scene.add(circle(1.0).stroke(RED, 0.06).move_to(origin));

// Where the dot is, and how far along x the curve has got, `t` seconds in.
let angle = |t| TAU * rate * min(t, run);
let dot_at = |t| origin + point(angle.call(t).cos(), angle.call(t).sin());
let curve_x = |t| curve_start + 4.0 * rate * min(t, run);

let to_circle = scene.add(line(origin, origin));
let to_curve = scene.add(line(origin, origin));
let curve = scene.add(line(origin, origin));
let dot = scene.add(dot(dot_at.call(0.0)));

scene.always(dot, |s, t| dot(dot_at.call(t)).fill(YELLOW).stroke(YELLOW, 0.0));
scene.always(to_circle, |s, t| line(origin, dot_at.call(t)).stroke(BLUE, 0.04));
scene.always(to_curve, |s, t| {
    let p = dot_at.call(t);
    line(p, point(curve_x.call(t), p.y)).stroke(hex(0xfff1b6), 0.02)
});
scene.always(curve, |s, t| {
    // A zero-width graph has no slope; start it a hair long.
    let x1 = max(curve_x.call(t), curve_start + 0.001);
    function_graph(|x| (PI / 2.0 * (x - curve_start)).sin(), curve_start, x1, 64)
        .stroke(hex(0xf4d345), 0.04)
});
scene.wait(run);
scene.wait();
