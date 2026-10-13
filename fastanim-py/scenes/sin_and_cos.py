# manim's SinAndCosFunctionPlot example: sin and cos on labelled axes, with a marked x = 2π.
# The Python version of fastanim-script/scenes/sin_and_cos.rhai.
from math import cos, sin

from fastanim import *


@scene
def sin_and_cos(s: Scene) -> None:
    ax = axes((-10, 10.3, 1), (-1.5, 1.5, 1), 10, 6)
    s.add(ax.shape().stroke(GREEN, 0.03).fill(GREEN))
    # manim's `numbers_to_include`: every other x tick. `ax.numbers()` numbers them all.
    for x in range(-10, 11, 2):
        if x != 0:
            s.add(math_tex(str(x)).scale(0.75).next_to(ax.c2p(x, 0), DOWN))
    s.add(ax.labels("x", "y"))

    s.add(ax.plot(sin, -10, 10.3).stroke(BLUE, 0.04))
    s.add(ax.plot(cos, -10, 10.3).stroke(RED, 0.04))
    s.add(math_tex("sin(x)").fill(BLUE).next_to(ax.c2p(-10, sin(-10)), UP / 2))
    s.add(math_tex("cos(x)").fill(RED).next_to(ax.c2p(10.3, cos(10.3)), RIGHT))

    top = ax.c2p(TAU, cos(TAU))
    s.add(ax.vertical_line(top).stroke(YELLOW, 0.04))
    s.add(math_tex("x = 2 pi").next_to(top, UP + RIGHT))
    s.wait()
