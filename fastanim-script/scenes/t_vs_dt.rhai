// f(t) = sin(2πt) · t, driven by t and by dt.
//
// In fastanim an updater is a function of t, the seconds since `always`, so the yellow dot sits
// at f(t): a wave whose amplitude grows with time. manim updaters get dt, the time since the last
// frame, and change the mobject a little each frame. Dropping the same formula into one,
//
//     dot.add_updater(lambda m, dt: m.shift(UP * f(dt)))
//
// adds f(1/fps) every frame: no wave at all, just a straight climb whose slope depends on the
// frame rate. The red and blue traces are that updater at 60 and 15 fps, written in closed form
// (n frames in, it has moved n · f(1/fps)) so they stay pure in t and scrub like everything else.
//
// fastanim preview fastanim-script/scenes/t_vs_dt.rhai

let run = 4.0;
let f = |t| (TAU * t).sin() * t;

let ax = axes([0.0, 4.0, 1.0], [-4.0, 4.0, 1.0], 10.0, 6.0);
scene.add(ax.shape().stroke(GREY, 0.03));
scene.add(ax.labels("t", "y"));
scene.add(math_tex("f(t) = sin(2 pi t) dot t").scale(0.8).to_edge(UP));

// A trace is a function of the clock: y at time t.
let by_t = |t| f.call(t);
let by_dt = |fps| |t| (t * fps).floor() * f.call(1.0 / fps);
let traces = [
    [by_t, YELLOW, "f(t)"],
    [by_dt.call(60.0), RED, "sum f(Delta t) quad \"60 fps\""],
    [by_dt.call(15.0), BLUE, "sum f(Delta t) quad \"15 fps\""],
];

let x0 = ax.c2p(0.0, 0.0).x;
for i in 0..traces.len() {
    let y = traces[i][0];
    let color = traces[i][1];
    // A legend in the empty top-left corner, left-aligned.
    let key = ax.c2p(0.1, 3.2 - 0.7 * i.to_float());
    scene.add(math_tex(traces[i][2]).scale(0.6).fill(color).move_to(key).align_to(dot(key), LEFT));

    let at = |t| ax.c2p(min(t, run), y.call(min(t, run)));
    let curve = scene.add(line(at.call(0.0), at.call(0.0)));
    let head = scene.add(dot(at.call(0.0)).fill(color).stroke(color, 0.0));
    scene.always(head, |s, t| dot(at.call(t)).fill(color).stroke(color, 0.0));
    scene.always(curve, |s, t| {
        // A zero-width graph has no slope; start it a hair long.
        let x1 = max(at.call(t).x, x0 + 0.001);
        function_graph(|x| at.call(ax.p2c(point(x, 0.0)).x).y, x0, x1, 480)
            .stroke(color, 0.04)
    });
}
scene.wait(run);
scene.wait();
