// Style options: a background colour, gradients, fill opacity, dashes, and line caps and joins.
scene.background(hex(0x1c1c28));
let title = scene.add(text("Styles").fill_gradient([BLUE, PINK]).scale(1.5).to_edge(UP));
let ring = scene.add(
    circle(1.2).stroke(WHITE, 0.16).stroke_gradient([YELLOW, RED], UP).shift(LEFT * 4.0)
);
let card = scene.add(square(2.4).fill_gradient([GREEN, BLUE, PURPLE], point(1.0, -1.0)));
let dashes = scene.add(circle(1.2).dashed(24).shift(RIGHT * 4.0));
let caps = scene.add(group([
    line(LEFT, RIGHT).stroke(WHITE, 0.2).line_cap("butt"),
    line(LEFT, RIGHT).stroke(WHITE, 0.2).line_cap("round"),
    line(LEFT, RIGHT).stroke(WHITE, 0.2).line_cap("square"),
]).arrange(DOWN).shift(DOWN * 2.6 + LEFT * 4.0));
let joins = scene.add(group([
    square(0.8).stroke(ORANGE, 0.16).line_join("miter"),
    square(0.8).stroke(ORANGE, 0.16).line_join("round"),
    square(0.8).stroke(ORANGE, 0.16).line_join("bevel"),
]).arrange(RIGHT).shift(DOWN * 2.6 + RIGHT * 4.0));
let road = scene.add(dashed_line(LEFT * 1.5, RIGHT * 1.5, 0.15).shift(DOWN * 2.6));
scene.play([create(ring), create(dashes), create(road), fade_in(card)]);
scene.play(transform(card, square(2.4).fill(GREEN, 0.4).stroke(GREEN, 0.04)));
scene.wait(1.0);
