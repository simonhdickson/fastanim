# A port of manim's BooleanOperations: two overlapping ellipses, then their intersection,
# union, exclusion and difference, each shrunk into a labelled corner.
from fastanim import *

# manim's `font_size=23` against its default of 48.
SMALL = 23 / 48


@scene
def boolean_operations(s: Scene) -> None:
    e1 = ellipse(4, 5).fill(BLUE, 0.5).stroke(BLUE, 0.1).move_to(LEFT)
    e2 = e1.fill(RED, 0.5).stroke(RED, 0.1).move_to(RIGHT)
    title = text("Boolean Operation").next_to(e1, UP * 3)
    underline = line((-1, 0), (1, 0)).scale_to_fit_width(title.width).next_to(title, DOWN, 0.1)
    g = s.add(group(title, underline, e1, e2).move_to(LEFT * 3))
    s.play(fade_in(g))
    # The ellipses where the group put them.
    e1, e2 = s.get(g[2]), s.get(g[3])

    # Each result appears over the ellipses, then `.animate` shrinks it into its corner.
    i = s.add(intersection(e1, e2).fill(GREEN, 0.5).stroke(GREEN, 0.04))
    s.play(i.animate.scale(0.25).move_to(RIGHT * 5 + UP * 2.5))
    i = s.get(i)
    s.play(fade_in(s.add(text("Intersection").scale(SMALL).next_to(i, UP))))

    union_text = text("Union").scale(SMALL)
    u = s.add(union(e1, e2).fill(ORANGE, 0.5).stroke(ORANGE, 0.04))
    s.play(u.animate.scale(0.3).next_to(i, DOWN, union_text.height * 3))
    u = s.get(u)
    s.play(fade_in(s.add(union_text.next_to(u, UP))))

    exclusion_text = text("Exclusion").scale(SMALL)
    x = s.add(exclusion(e1, e2).fill(YELLOW, 0.5).stroke(YELLOW, 0.04))
    s.play(x.animate.scale(0.3).next_to(u, DOWN, exclusion_text.height * 3.5))
    x = s.get(x)
    s.play(fade_in(s.add(exclusion_text.next_to(x, UP))))

    difference_text = text("Difference").scale(SMALL)
    d = s.add(difference(e1, e2).fill(PINK, 0.5).stroke(PINK, 0.04))
    s.play(d.animate.scale(0.3).next_to(u, LEFT, difference_text.height * 3.5))
    d = s.get(d)
    s.play(fade_in(s.add(difference_text.next_to(d, UP))))
