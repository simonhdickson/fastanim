# f(t) = sin(2πt) · t over its first 20 seconds: a wave whose amplitude grows with time.
# The dot sits at f(t), with t the seconds since `always`, and the graph traces behind it.
# python -m fastanim preview fastanim-py/scenes/growing_wave.py
from math import ceil, sin

from fastanim import *

RUN = 20.0


def f(t: float) -> float:
    return sin(TAU * t) * t


@scene
def growing_wave(s: Scene) -> None:
    # |f(t)| <= t, so y spans ±RUN. x runs a little past RUN so the tip doesn't cover the
    # last number.
    ax = axes((0, 21.5, 2), (-RUN, RUN, 5), 11, 6)
    shape = s.add(ax.shape().stroke(GREY, 0.03).fill(GREY))
    numbers = s.add(ax.numbers())
    labels = s.add(ax.labels("t", "y"))
    title = s.add(math_tex("f(t) = sin(2 pi t) dot t").scale(0.8).to_edge(UP))
    s.play(create(shape), write(numbers), write(labels), write(title))

    def at(t: float) -> Point:
        return ax.c2p(min(t, RUN), f(min(t, RUN)))

    curve = s.add(line(at(0), at(0)))
    head = s.add(dot(at(0)).fill(YELLOW).stroke(YELLOW, 0))
    s.always(head, lambda st, t: dot(at(t)).fill(YELLOW).stroke(YELLOW, 0))

    @s.always(curve)
    def trace(st: State, t: float) -> Shape:
        # A zero-width graph has no slope; start it a hair long.
        t1 = max(min(t, RUN), 0.002)
        # One cycle per second: 32 segments a cycle keeps the peaks smooth, and sampling only
        # what's drawn, in graph coordinates, keeps every frame cheap.
        return ax.plot(f, 0, t1, ceil(32 * t1)).stroke(YELLOW, 0.04)

    s.wait(RUN)
    s.wait()
