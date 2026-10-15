# A port of manim's MovingGroupToDestination: the Python version of
# fastanim-script/scenes/moving_group.rhai.
from fastanim import *


@scene
def moving_group(s: Scene) -> None:
    g = s.add(group(dot(LEFT), dot(ORIGIN), dot(RIGHT).fill(RED), dot(RIGHT * 2)).scale(1.4))
    dest = s.add(dot((4, 3)).fill(YELLOW))
    s.play(g.animate.shift(s.get(dest).center() - s.get(g[2]).center()))
