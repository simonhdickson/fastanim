# Running scenes

`python -m fastanim` runs scene files in three ways.

| Command | What it does |
| --- | --- |
| `python -m fastanim preview <file>` | Opens a window with a timeline, re-running the file on save |
| `python -m fastanim render <file>` | Exports video or frames |
| `python -m fastanim still <file>` | Exports one frame |

When a file has more than one `@scene`, name the one to run: `hello.py:hello`. A plain
`def construct(scene)` works too.

The file runs with `__name__` set to something other than `"__main__"`, so code under
`if __name__ == "__main__":` is skipped.

## Preview

```sh
python -m fastanim preview hello.py
```

| Key | Action |
| --- | --- |
| Space | Play / pause |
| ← / → | Step a frame |
| `[` / `]` | Jump to the previous / next [marker](scene.md#markers) |
| Drag along the timeline | Scrub |
| Shift + drag along the timeline | Select a region; playback loops over it |
| Esc | Clear the loop region |
| Mouse wheel over the timeline | Scroll its rows, when there are more than five |

The timeline under the scene has a block per `play`, labelled with the animation's kind, and
rows below for updaters and sounds. It shows five rows at a time and scrolls through the rest.
The web player has the same timeline.

Preview runs the file again whenever it is saved and keeps the current time; if the new version
fails, the error is printed and the last good scene stays up. Modules it imports from its own
folder are watched too, and imported afresh on each run; installed packages are not. Typeset
text is cached, so a re-run only typesets text that changed.

## Render

```sh
python -m fastanim render hello.py                  # hello.mp4, named after the scene
python -m fastanim render hello.py -f gif -q 720p30 # hello.gif
python -m fastanim render hello.py -f png           # hello/0000.png, hello/0001.png, …
python -m fastanim render hello.py --section intro -o intro.webm
```

- `-f, --format`: what to write.
  - `mp4` (the default), `webm` or `gif`: a video file. `mp4` and `webm` include the scene's
    [sounds](audio.md).
  - `png` or `svg`: a folder of frames, one file each.
  - `animated-svg`: one `.svg` that plays the scene and loops. Browsers play it, inline or in
    an `<img>`, with no script. It has no sound or shader fills, as with SVG frames. Frames
    that don't change are written once, but every moving frame adds the whole scene to the
    file, so a low frame rate (`480p15`) keeps it small.
- `-o, --output`: the file or folder to write. By default it's named after the scene: the
  function for Python, the script for Rhai, as `NAME.mp4`, `NAME/` or `NAME.svg`. Without
  `-f`, its extension picks the format, and an extension that doesn't match `-f` is an error.
- `-q, --quality`: `480p15`, `720p30`, `1080p60` (default) or `4k60`.
- `--section <marker>`: render only from that marker to the next one.

On free-threaded Python (3.14t), video export runs a scene's updaters on every core. With the
GIL they run on one thread. maturin builds a separate 3.14t wheel for it.

## Still

```sh
python -m fastanim still hello.py --at 2.5s         # hello.png
python -m fastanim still hello.py --frame 120 -f svg -o frame.svg
```

`--at` takes seconds (`3.5` or `3.5s`); `--frame` takes a frame index at the chosen quality.
`-f` is `png` (the default) or `svg`, and `-o` works as for `render`.

## From code

A `@scene` function has the same commands as methods:

```python
hello.preview()                                   # blocks until the window closes
hello.render()                                    # hello.mp4; also quality=, section=
hello.render(format="animated-svg")               # hello.svg, one SVG that plays it
hello.still("frame.svg", at=3.5)                  # or frame=210
tl = hello.bake()                                 # the timeline: tl.duration, tl.svg(t), …
tl.render("frames", format="svg", quality="480p15")  # a folder of SVG frames
```

The preview window can be opened once per Python process. `Timeline.svg(t)` draws over the
scene's background unless given another `background=`.

## Jupyter

A cell that ends in a `@scene` function, or a timeline from `.bake()`, shows the scene as a
video under the cell, rendered at `720p30` with its sounds. The video is embedded in the
notebook, so long scenes make large `.ipynb` files; for those, `render` to a file and link it.
In JupyterLite (Pyodide), which can't render video, the cell shows the first frame instead.

```python
@scene
def hello(s):
    s.play(create(s.add(circle(1))))

hello
```

## Errors

Errors are raised as Python exceptions, with a traceback through the scene file:
`SceneError` for misuse of the scene, such as animating a mobject that isn't in it, and
`TypesetError` for text that doesn't typeset. An error in an [updater](updaters.md) or other
closure is raised at the next `s.play` or `s.wait` after it.
