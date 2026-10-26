# A Lottie file playing as fastanim shapes: `bounce.json` is a ball bouncing on a line that
# draws itself in. It's placed, scaled and faded like any mobject and plays at its own frame
# rate. Animations that move it while it plays go before `play_lottie`, which follows them.
# python -m fastanim preview fastanim-py/scenes/lottie.py
from pathlib import Path

from fastanim import *


@scene
def lottie_scene(s: Scene) -> None:
    title = s.add(text("Lottie").to_edge(UP))
    ball = s.add(lottie(Path(__file__).parent / "bounce.json").scale_to_fit_height(5).shift(DOWN * 0.5))
    s.play(fade_in(ball))
    s.play(play_lottie(ball, loops=2))
    s.play(shift(ball, LEFT * 3), play_lottie(ball))
    s.play(fade_out(group(title, ball)))
