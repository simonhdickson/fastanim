// A port of manim's GradientImageFromArray: a 256×256 grey ramp built from pixel values, in a
// green box.
let n = 256;
let ramp = [];
for i in 0..n {
    ramp.push(i * 256 / n);
}
let rows = [];
for _ in 0..n {
    rows.push(ramp);
}
let img = image(rows).scale(2.0);
// ponytail: `surrounding_rectangle` (ROADMAP item 8) will replace this hand-made box.
let side = 2.0 * FRAME_HEIGHT * n.to_float() / 1080.0 + 0.2;
let frame = rectangle(side, side).stroke(GREEN, 0.04).move_to(img.center());
scene.add(img);
scene.add(frame);
scene.wait(1.0);
