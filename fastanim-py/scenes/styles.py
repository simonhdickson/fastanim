# Style options: the Python version of fastanim-script/scenes/styles.rhai.
from fastanim import *


@scene
def styles(s: Scene) -> None:
    s.background(hex(0x1C1C28))
    s.add(text("Styles").fill_gradient([BLUE, PINK]).scale(1.5).to_edge(UP))
    ring = s.add(
        circle(1.2, stroke=WHITE, stroke_width=0.16)
        .stroke_gradient([YELLOW, RED], UP)
        .shift(LEFT * 4)
    )
    card = s.add(square(2.4).fill_gradient([GREEN, BLUE, PURPLE], (1, -1)))
    dashes = s.add(circle(1.2).dashed(24).shift(RIGHT * 4))
    bar = line(LEFT, RIGHT, stroke=WHITE, stroke_width=0.2)
    caps = group(*(bar.line_cap(c) for c in ["butt", "round", "square"]))
    s.add(caps.arrange(DOWN).shift(DOWN * 2.6 + LEFT * 4))
    corner = square(0.8, stroke=ORANGE, stroke_width=0.16)
    joins = group(*(corner.line_join(j) for j in ["miter", "round", "bevel"]))
    s.add(joins.arrange(RIGHT).shift(DOWN * 2.6 + RIGHT * 4))
    road = s.add(dashed_line(LEFT * 1.5, RIGHT * 1.5, 0.15).shift(DOWN * 2.6))
    s.play(create(ring), create(dashes), create(road), fade_in(card))
    s.play(transform(card, square(2.4, stroke=GREEN, stroke_width=0.04).fill(GREEN, 0.4)))
    s.wait(1)
