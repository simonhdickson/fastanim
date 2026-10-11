# manim's SineCurveUnitCircle example (contributed by heejin_park,
# https://infograph.tistory.com/230): a dot circles the unit circle and traces sin.
# manim accumulates state in updaters; here every frame is pure in t, so scrubbing works.
# The Python version of fastanim-script/scenes/sine_curve.rhai.
from math import cos, sin

from fastanim import *

CENTER = point(-4, 0)
CURVE_START = -3.0
RATE = 0.25  # turns per second
RUN = 8.5


def angle(t: float) -> float:
    return TAU * RATE * min(t, RUN)


# Where the dot is, and how far along x the curve has got, `t` seconds in.
def dot_at(t: float) -> Point:
    return CENTER + point(cos(angle(t)), sin(angle(t)))


def curve_x(t: float) -> float:
    return CURVE_START + 4 * RATE * min(t, RUN)


@scene
def sine_curve(s: Scene) -> None:
    s.add(line((-6, 0), (6, 0)))
    s.add(line((-4, -2), (-4, 2)))
    for i, label in enumerate(["pi", "2 pi", "3 pi", "4 pi"]):
        s.add(math_tex(label).move_to((-1 + 2 * i, -0.5)))
    s.add(circle(1, stroke=RED, stroke_width=0.06).move_to(CENTER))

    to_circle = s.add(line(CENTER, CENTER))
    to_curve = s.add(line(CENTER, CENTER))
    curve = s.add(line(CENTER, CENTER))
    d = s.add(dot(dot_at(0)))

    s.always(d, lambda st, t: dot(dot_at(t)).fill(YELLOW).stroke(YELLOW, 0))
    s.always(to_circle, lambda st, t: line(CENTER, dot_at(t)).stroke(BLUE, 0.04))

    @s.always(to_curve)
    def follow(st: State, t: float) -> Shape:
        p = dot_at(t)
        return line(p, (curve_x(t), p.y)).stroke(hex(0xFFF1B6), 0.02)

    @s.always(curve)
    def trace(st: State, t: float) -> Shape:
        # A zero-width graph has no slope; start it a hair long.
        x1 = max(curve_x(t), CURVE_START + 0.001)
        return function_graph(lambda x: sin(PI / 2 * (x - CURVE_START)), CURVE_START, x1, 64).stroke(
            hex(0xF4D345), 0.04
        )

    s.wait(RUN)
    s.wait()
