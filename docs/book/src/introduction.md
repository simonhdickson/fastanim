# Introduction

fastanim is a mathematical-animation engine in the spirit of
[manim](https://github.com/ManimCommunity/manim). Scenes can be written in Python, in Rust or as
[Rhai](https://rhai.rs) scripts; this book covers Python. Rhai scripts have the same API and
run locally with the `fastanim` CLI; see [Rhai scripts](rhai.md).

A scene is a function marked `@scene` that takes the scene as its argument. It runs once, top to
bottom, adding shapes and text to the scene and playing animations on it. What it records is
baked into a timeline, so playback, scrubbing and export never run the function again.

```python
# hello.py: a circle draws itself, then a formula is written under it.
from fastanim import *


@scene
def hello(s: Scene) -> None:
    title = s.add(text("Hello, fastanim").to_edge(UP))
    s.play(write(title))

    ring = s.add(circle(1.5, stroke=BLUE, stroke_width=0.06))
    s.play(create(ring), run_time=1.5)

    formula = s.add(math_tex("y = sin(x)").scale(1.5).fill(GREEN).to_edge(DOWN))
    s.play(write(formula))
    s.wait(1)
```

Save that as `hello.py` and run `python -m fastanim preview hello.py`. The window runs the file
again every time you save it.

## Install

```sh
pip install fastanim
```

From a checkout, build it into a virtualenv with [maturin](https://www.maturin.rs):

```sh
maturin develop -m fastanim-py/pyproject.toml
```

Video export also needs `ffmpeg` on your `PATH`.

## Try it in the browser

The [playground](../) runs Python scenes in the browser with no install, on
[Pyodide](https://pyodide.org). It ships with the example scenes from
[`fastanim-py/scenes`](https://github.com/simonhdickson/fastanim/tree/main/fastanim-py/scenes).
Link straight to one with `?scene=<name>`, e.g. [`?scene=diff`](../?scene=diff), or to the
first `.py` file in a public GitHub gist with `?gist=<id>`. **Share link** copies a link that
holds the script itself, so edits can be shared with nothing uploaded anywhere. Imports of
Pyodide's own packages, such as `numpy`, are fetched on first use.
