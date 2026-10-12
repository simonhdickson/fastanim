// f(t) = sin(2πt) · t over its first 20 seconds: a wave whose amplitude grows with time.
//
// The dot sits at f(t), with t the seconds since `always`, and the graph traces behind it.
//
// fastanim preview fastanim-script/scenes/growing_wave.rhai

let run = 20.0;
let f = |t| (TAU * t).sin() * t;

// The y range comes from f: -20 to 20 in steps of 5.
let ax = fit_axes(f, [0.0, 20.0, 2.0], 11.0, 6.0);
scene.add(ax.shape().stroke(GREY, 0.03));
scene.add(ax.labels("t", "y"));
scene.add(math_tex("f(t) = sin(2 pi t) dot t").scale(0.8).to_edge(UP));

let at = |t| ax.c2p(min(t, run), f.call(min(t, run)));
let curve = scene.add(line(at.call(0.0), at.call(0.0)));
let head = scene.add(dot(at.call(0.0)).fill(YELLOW).stroke(YELLOW, 0.0));
scene.always(head, |s, t| dot(at.call(t)).fill(YELLOW).stroke(YELLOW, 0.0));
scene.always(curve, |s, t| {
    // A zero-width graph has no slope; start it a hair long.
    let t1 = max(min(t, run), 0.002);
    // One cycle per second: 32 segments a cycle keeps the peaks smooth, and sampling only
    // what's drawn, in graph coordinates, keeps every frame cheap.
    ax.plot(f, 0.0, t1, (32.0 * t1).ceiling().to_int()).stroke(YELLOW, 0.04)
});
scene.wait(run);
scene.wait();
