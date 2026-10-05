// manim's SinAndCosFunctionPlot example: sin and cos on labelled axes, with a marked x = 2π.
//
// fastanim run fastanim-script/scenes/sin_and_cos.rhai

let ax = axes([-10.0, 10.3, 1.0], [-1.5, 1.5, 1.0], 10.0, 6.0);
scene.add(ax.shape().stroke(GREEN, 0.03));
// manim's `numbers_to_include`: every other x tick. `ax.numbers()` numbers them all.
for x in range(-10, 11, 2) {
    if x != 0 {
        scene.add(math_tex(x.to_string()).scale(0.75).next_to(ax.c2p(x.to_float(), 0.0), DOWN));
    }
}
scene.add(ax.labels("x", "y"));

scene.add(ax.plot(|x| x.sin(), -10.0, 10.3).stroke(BLUE, 0.04));
scene.add(ax.plot(|x| x.cos(), -10.0, 10.3).stroke(RED, 0.04));
scene.add(math_tex("sin(x)").fill(BLUE).next_to(ax.c2p(-10.0, (-10.0).sin()), UP / 2.0));
scene.add(math_tex("cos(x)").fill(RED).next_to(ax.c2p(10.3, 10.3.cos()), RIGHT));

let top = ax.c2p(TAU, TAU.cos());
scene.add(ax.vertical_line(top).stroke(YELLOW, 0.04));
scene.add(math_tex("x = 2 pi").next_to(top, UP + RIGHT));
scene.wait();
