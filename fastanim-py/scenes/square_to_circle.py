# A port of manim's SquareToCircle: the Python version of
# fastanim-script/scenes/square_to_circle.rhai.
from fastanim import *


@scene
def square_to_circle(s: Scene) -> None:
    sq = s.add(square(2).rotate(PI / 4))
    s.play(create(sq))
    s.play(transform(sq, circle(1).fill(PINK, 0.5)))
    s.play(fade_out(sq))
