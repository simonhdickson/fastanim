// A Lottie file playing as fastanim shapes: `bounce.json` is a ball bouncing on a line that
// draws itself in. It's placed, scaled and faded like any mobject and plays at its own frame
// rate. Animations that move it while it plays go before `play_lottie`, which follows them.
let title = scene.add(text("Lottie").to_edge(UP));
let ball = scene.add(lottie("bounce.json").scale_to_fit_height(5.0).shift(DOWN * 0.5));
scene.play(fade_in(ball));
scene.play(play_lottie(ball, #{loops: 2}));
scene.play([shift(ball, LEFT * 3.0), play_lottie(ball)]);
scene.play(fade_out(group([title, ball])));
