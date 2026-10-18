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

let i0 = intersection(e1, e2).fill(GREEN, 0.5).stroke(GREEN, 0.04);
let i = i0.scale(0.25).move_to(RIGHT * 5.0 + UP * 2.5);
scene.play(transform(scene.add(i0), i));
let intersection_text = scene.add(text("Intersection").scale(small).next_to(i, UP));
scene.play(fade_in(intersection_text));

let u0 = union(e1, e2).fill(ORANGE, 0.5).stroke(ORANGE, 0.04);
let union_text = text("Union").scale(small);
let u = u0.scale(0.3).next_to(i, DOWN, union_text.height() * 3.0);
scene.play(transform(scene.add(u0), u));
scene.play(fade_in(scene.add(union_text.next_to(u, UP))));

let x0 = exclusion(e1, e2).fill(YELLOW, 0.5).stroke(YELLOW, 0.04);
let exclusion_text = text("Exclusion").scale(small);
let x = x0.scale(0.3).next_to(u, DOWN, exclusion_text.height() * 3.5);
scene.play(transform(scene.add(x0), x));
scene.play(fade_in(scene.add(exclusion_text.next_to(x, UP))));

let d0 = difference(e1, e2).fill(PINK, 0.5).stroke(PINK, 0.04);
let difference_text = text("Difference").scale(small);
let d = d0.scale(0.3).next_to(u, LEFT, difference_text.height() * 3.5);
scene.play(transform(scene.add(d0), d));
scene.play(fade_in(scene.add(difference_text.next_to(d, UP))));
