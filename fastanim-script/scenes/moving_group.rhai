// A port of manim's MovingGroupToDestination: shift a group so its red dot lands on the
// yellow one.
let g = scene.add(group([
    dot(LEFT),
    dot(ORIGIN),
    dot(RIGHT).fill(RED),
    dot(RIGHT * 2.0),
]).scale(1.4));
let dest = scene.add(dot(point(4.0, 3.0)).fill(YELLOW));
scene.play(shift(g, scene.get(dest).center() - scene.get(g[2]).center()));
