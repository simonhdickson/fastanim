# Python scenes

The same API is available from Python, as the `fastanim` package. A Python scene is a function
marked `@scene` that takes the scene as its argument:

```python
# hello.py
from fastanim import *

@scene
def hello(s: Scene) -> None:
    title = s.add(text("Hello, fastanim").to_edge(UP))
    s.play(write(title))
    ring = s.add(circle(1.5, stroke=BLUE, stroke_width=0.06))
    s.play(create(ring), run_time=1.5)
```

A plain `def construct(scene)` works too.

## Installing

`fastanim` records and bakes scenes and exports SVG. Video, PNG and the preview window come with
the `preview` extra, a second native module built with the renderer, which `fastanim` uses when
it is installed:

```sh
pip install fastanim            # SVG stills and frames
pip install fastanim[preview]   # + video, PNG and the preview window
```

From a checkout, build both into a virtualenv with [maturin](https://www.maturin.rs):

```sh
maturin develop -m fastanim-py/pyproject.toml
maturin develop -m fastanim-py/preview/pyproject.toml
```

## From the command line

`python -m fastanim` takes the commands and options of [the `fastanim` CLI](running.md):

```sh
python -m fastanim preview hello.py
python -m fastanim render hello.py -q 720p30 -o hello.mp4
python -m fastanim render hello.py --section intro -o intro.webm
python -m fastanim still hello.py --at 2.5s -o frame.png
```

When a file has more than one `@scene`, name the one to run: `hello.py:hello`.

Preview runs the file again whenever it is saved and keeps the current time; if the new version
fails, the error is printed and the last good scene stays up. Modules the scene imports are not
reloaded. The file runs with `__name__` set to something other than `"__main__"`, so code under
`if __name__ == "__main__":` is skipped.

## From code

```python
hello.preview()                                   # blocks until the window closes
hello.render("hello.mp4", quality="1080p60")      # also section="intro"
hello.still("frame.svg", at=3.5)                  # or frame=210
tl = hello.bake()                                 # the timeline: tl.duration, tl.svg(t), …
tl.render("frames.svg", quality="480p15")         # a directory of SVG frames
```

Only `svg` output works without the `preview` extra; other formats raise a `RuntimeError` saying
to install it. The preview window can be opened once per Python process.

## Images, Lottie animations and fonts

`image` takes a path, or rows of pixels as nested lists or a NumPy array (anything with
`tolist()`), plus `height=`, `width=` and `pixelated=` keywords. `lottie(path)` takes
`height=` and `width=` too, and `play_lottie(m, start=, end=, loops=)` plays it. Text functions take a `font=`
keyword, and `load_font(path)` loads a font file (relative to the current directory):

```python
import numpy as np

load_font("fonts/Inter-Regular.ttf")

@scene
def gallery(s: Scene) -> None:
    ramp = np.tile(np.arange(256, dtype=np.uint8), (64, 1))
    s.add(image(ramp, height=2, pixelated=True).to_edge(DOWN))
    s.add(text("Hello", font="Inter").to_edge(UP))
    spinner = s.add(lottie("spinner.json", height=3))
    s.play(play_lottie(spinner, loops=3))
```
