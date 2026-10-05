// SPEC §10 example 6, "classic manim parity": the script version of
// fastanim-bevy/examples/parity.rs.
//
// fastanim run fastanim-script/scenes/parity.rhai

let title = scene.add(text("Classic manim parity").to_edge(UP));
scene.play(write(title));
let circle = scene.add(circle(1.5).stroke(BLUE, 0.06));
scene.play(create(circle).run_time(1.5));
let square = scene.add(square(2.0).stroke(RED, 0.06).shift(LEFT * 4.0));
scene.play(fade_in(square));
scene.play(transform(
    square,
    circle(1.0).fill(RED.with_alpha(0.5)).stroke(RED, 0.06).shift(LEFT * 4.0),
));
scene.wait(0.5);
scene.play([fade_out(circle), fade_out(square)]);
scene.remove(circle);
scene.remove(square);

let formula = scene.add(math_tex("y = sin(x)").scale(1.5).fill(GREEN).to_edge(DOWN));
scene.play(write(formula));

scene.marker("graph");
let ax = axes([-7, 7, 1], [-2, 2, 1], 14.0, 4.0);
let axes = scene.add(ax);
scene.play(create(axes));
let graph = scene.add(ax.plot(|x| x.sin(), -2.0 * PI, 2.0 * PI).stroke(GREEN, 0.05));
scene.play(create(graph).run_time(2.0));

let dot = scene.add(dot(point(-2.0 * PI, 0.0)).fill(YELLOW));
scene.play(fade_in(dot).run_time(0.5));
// One full sweep left to right in 4 s, then back: pure in t, so scrubbing works.
scene.always(dot, |state, t| {
    let phase = t / 4.0 % 2.0;
    let x = 2.0 * PI * (2.0 * min(phase, 2.0 - phase) - 1.0);
    state[dot].move_to(ax.c2p(x, x.sin()))
});
scene.wait(8.0);
