// A port of manim's SquareToCircle: a square turns into a pink, half-transparent circle.
let sq = scene.add(square(2.0).rotate(PI / 4.0));
scene.play(create(sq));
scene.play(transform(sq, circle(1.0).fill(PINK, 0.5)));
scene.play(fade_out(sq));
