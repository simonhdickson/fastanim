# fastanim

Animated math and code scenes in Python, in the spirit of
[manim](https://github.com/ManimCommunity/manim). Transforms between equations, text and code
are planned with Myers' diff, so unchanged parts stay put or slide, moved parts travel, and only
real changes fade or morph. The engine is Rust, rendering with [Vello](https://github.com/linebender/vello)
and typesetting with [Typst](https://typst.app), so no LaTeX install is needed.

```sh
pip install fastanim
```

```python
# hello.py
from fastanim import *


@scene
def hello(s: Scene) -> None:
    title = s.add(text("Hello, fastanim").to_edge(UP))
    s.play(write(title))
    ring = s.add(circle(1.5, stroke=BLUE, stroke_width=0.06))
    s.play(create(ring), run_time=1.5)
    eq = s.add(math_tex("a^2 + b^2 = c^2").scale(1.5).to_edge(DOWN))
    s.play(write(eq))
    s.play(transform_diff(eq, math_tex("a^2 = c^2 - b^2").scale(1.5).to_edge(DOWN)))
```

```sh
python -m fastanim preview hello.py                  # a window with a scrubber, reloads on save
python -m fastanim render hello.py -o hello.mp4      # mp4, webm, gif, png or svg frames
python -m fastanim still hello.py --at 2s -o frame.png
```

In Jupyter, a cell ending in a scene (`hello`) or its timeline (`hello.bake()`) plays it inline.

Video export needs `ffmpeg` on your `PATH`. The package ships type stubs for completion and
type checking.

- [Docs](https://simonhdickson.github.io/fastanim/docs/)
- [Playground](https://simonhdickson.github.io/fastanim/): runs scenes in the browser, no install
- [Source](https://github.com/simonhdickson/fastanim)
