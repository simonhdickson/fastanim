# A port of manim's GradientImageFromArray: the Python version of
# fastanim-script/scenes/gradient_image.rhai. A NumPy array works as well as nested lists.
from fastanim import *


@scene
def gradient_image(s: Scene) -> None:
    n = 256
    img = image([[i * 256 // n for i in range(n)] for _ in range(n)]).scale(2)
    side = 2 * FRAME_HEIGHT * n / 1080 + 0.2
    s.add(img)
    s.add(rectangle(side, side, stroke=GREEN, stroke_width=0.04).move_to(img.center()))
    s.wait(1)
