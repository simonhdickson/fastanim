// A port of manim's MovingAround: a blue square moves, turns orange, shrinks and turns, each
// step an `animate` from wherever the last one left it.
// fastanim preview fastanim-script/scenes/moving_around.rhai
let sq = scene.add(square(2.0).fill(BLUE).stroke(BLUE, 0.04));
scene.play(animate(sq, |s| s.shift(LEFT)));
scene.play(animate(sq, |s| s.fill(ORANGE)));
scene.play(animate(sq, |s| s.scale(0.3)));
scene.play(animate(sq, |s| s.rotate(0.4)));
