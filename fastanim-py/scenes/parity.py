# SPEC §10 example 6, "classic manim parity": the Python version of
# fastanim-bevy/examples/parity.rs and fastanim-script/scenes/parity.rhai.
from math import sin

from fastanim import *


@scene
def parity(s: Scene) -> None:
    title = s.add(text("Classic manim parity").to_edge(UP))
    s.play(write(title))
    c = s.add(circle(1.5, stroke=BLUE, stroke_width=0.06))
    s.play(create(c), run_time=1.5)
    sq = s.add(square(2, stroke=RED, stroke_width=0.06).shift(LEFT * 4))
    s.play(fade_in(sq))
    s.play(transform(sq, circle(1, fill=RED.with_alpha(0.5), stroke=RED, stroke_width=0.06).shift(LEFT * 4)))
    s.wait(0.5)
    s.play(fade_out(c), fade_out(sq))
    s.remove(c)
    s.remove(sq)

    formula = s.add(math_tex("y = sin(x)").scale(1.5).fill(GREEN).to_edge(DOWN))
    s.play(write(formula))

    s.marker("graph")
    ax = axes((-7, 7, 1), (-2, 2, 1), 14, 4)
    s.play(create(s.add(ax)))
    graph = s.add(ax.plot(sin, -2 * PI, 2 * PI).stroke(GREEN, 0.05))
    s.play(create(graph), run_time=2)

    d = s.add(dot((-2 * PI, 0)).fill(YELLOW))
    s.play(fade_in(d), run_time=0.5)

    # One full sweep left to right in 4 s, then back: pure in t, so scrubbing works.
    @s.always(d)
    def sweep(state: State, t: float) -> Shape:
        phase = t / 4 % 2
        x = 2 * PI * (2 * min(phase, 2 - phase) - 1)
        return state[d].move_to(ax.c2p(x, sin(x)))

    s.wait(8)
