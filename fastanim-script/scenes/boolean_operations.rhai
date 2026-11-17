// A port of manim's BooleanOperations: two overlapping ellipses, then their intersection,
// union, exclusion and difference, each shrunk into a labelled corner.
// fastanim preview fastanim-script/scenes/boolean_operations.rhai

// manim's `font_size=23` against its default of 48.
let small = 23.0 / 48.0;

let e1 = ellipse(4.0, 5.0).fill(BLUE, 0.5).stroke(BLUE, 0.1).move_to(LEFT);
let e2 = e1.fill(RED, 0.5).stroke(RED, 0.1).move_to(RIGHT);
let title = text("Boolean Operation").next_to(e1, UP * 3.0);
let underline = line(point(-1.0, 0.0), point(1.0, 0.0))
    .scale_to_fit_width(title.width())
    .next_to(title, DOWN, 0.1);
let g = scene.add(group([title, underline, e1, e2]).move_to(LEFT * 3.0));
scene.play(fade_in(g));
// The ellipses where the group put them.
let e1 = scene.get(g[2]);
let e2 = scene.get(g[3]);

// Each result appears over the ellipses, then `animate` shrinks it into its corner.
let i = scene.add(intersection(e1, e2).fill(GREEN, 0.5).stroke(GREEN, 0.04));
scene.play(animate(i, |s| s.scale(0.25).move_to(RIGHT * 5.0 + UP * 2.5)));
let i = scene.get(i);
let intersection_text = scene.add(text("Intersection").scale(small).next_to(i, UP));
scene.play(fade_in(intersection_text));

let union_text = text("Union").scale(small);
let u = scene.add(union(e1, e2).fill(ORANGE, 0.5).stroke(ORANGE, 0.04));
scene.play(animate(u, |s| s.scale(0.3).next_to(i, DOWN, union_text.height() * 3.0)));
let u = scene.get(u);
scene.play(fade_in(scene.add(union_text.next_to(u, UP))));

let exclusion_text = text("Exclusion").scale(small);
let x = scene.add(exclusion(e1, e2).fill(YELLOW, 0.5).stroke(YELLOW, 0.04));
scene.play(animate(x, |s| s.scale(0.3).next_to(u, DOWN, exclusion_text.height() * 3.5)));
let x = scene.get(x);
scene.play(fade_in(scene.add(exclusion_text.next_to(x, UP))));

let difference_text = text("Difference").scale(small);
let d = scene.add(difference(e1, e2).fill(PINK, 0.5).stroke(PINK, 0.04));
scene.play(animate(d, |s| s.scale(0.3).next_to(u, LEFT, difference_text.height() * 3.5)));
let d = scene.get(d);
scene.play(fade_in(scene.add(difference_text.next_to(d, UP))));
