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
python -m fastanim render hello.py -q 720p30 -o hello.mp4
python -m fastanim render hello.py --section intro -o intro.webm
```

- `-q, --quality`: `480p15`, `720p30`, `1080p60` (default) or `4k60`.
- `-o, --output`: the extension picks the format: `mp4`, `webm`, `gif`, `png` or `svg`.
  `png` and `svg` write one file per frame. `mp4` and `webm` include the scene's
  [sounds](audio.md).
- `--section <marker>`: render only from that marker to the next one.

On free-threaded Python (3.14t), video export runs a scene's updaters on every core. With the
GIL they run on one thread. maturin builds a separate 3.14t wheel for it.

## Still

```sh
python -m fastanim still hello.py --at 2.5s -o frame.png
python -m fastanim still hello.py --frame 120 -o frame.svg
```

`--at` takes seconds (`3.5` or `3.5s`); `--frame` takes a frame index at the chosen quality.

## From code

A `@scene` function has the same commands as methods:

```python
hello.preview()                                   # blocks until the window closes
hello.render("hello.mp4", quality="1080p60")      # also section="intro"
hello.still("frame.svg", at=3.5)                  # or frame=210
tl = hello.bake()                                 # the timeline: tl.duration, tl.svg(t), …
tl.render("frames.svg", quality="480p15")         # a directory of SVG frames
```

The preview window can be opened once per Python process. `Timeline.svg(t)` draws over the
scene's background unless given another `background=`.

## Errors

Errors are raised as Python exceptions, with a traceback through the scene file:
`SceneError` for misuse of the scene, such as animating a mobject that isn't in it, and
`TypesetError` for text that doesn't typeset. An error in an [updater](updaters.md) or other
closure is raised at the next `s.play` or `s.wait` after it.
