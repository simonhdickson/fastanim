# A port of manim's MovingAround: a blue square moves, turns orange, shrinks and turns, each
# step an `.animate` from wherever the last one left it.
from fastanim import *


@scene
def moving_around(s: Scene) -> None:
    sq = s.add(square(2).fill(BLUE).stroke(BLUE, 0.04))
    s.play(sq.animate.shift(LEFT))
    s.play(sq.animate.fill(ORANGE))
    s.play(sq.animate.scale(0.3))
    s.play(sq.animate.rotate(0.4))
