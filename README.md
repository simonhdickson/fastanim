# fastanim

[![PyPI](https://img.shields.io/pypi/v/fastanim)](https://pypi.org/project/fastanim/)

A programmatic mathematical-animation engine in Rust, built on [Bevy](https://bevyengine.org),
inspired by [manim](https://github.com/ManimCommunity/manim).

Its distinguishing feature: transforms between equations, text, code and groups of shapes are
planned with **Myers' diff**, so unchanged parts stay put or slide, moved parts travel, and only
real changes fade or morph.

## Install

```sh
pip install fastanim
```

[`fastanim` on PyPI](https://pypi.org/project/fastanim/) has wheels for Linux (x86_64), macOS
(Apple Silicon) and Windows (x86_64), for CPython 3.10+ and free-threaded 3.14t. Video export also needs `ffmpeg` on your `PATH`. Or try it with no install in the
[browser playground](https://simonhdickson.github.io/fastanim/), which runs Python scenes on
[Pyodide](https://pyodide.org).

## A taste

Scenes are written in Python (or in Rust, or as [Rhai](https://rhai.rs) scripts, with the same
API):

```python
from fastanim import *


@scene
def parity(s: Scene) -> None:
    title = s.add(text("Classic manim parity").to_edge(UP))
    s.play(write(title))
    ring = s.add(circle(1.5, stroke=BLUE, stroke_width=0.06))
    s.play(create(ring), run_time=1.5)

    formula = s.add(math_tex("y = sin(x)").scale(1.5).fill(GREEN).to_edge(DOWN))
    s.play(write(formula))
```

```sh
python -m fastanim preview fastanim-py/scenes/parity.py   # reloads on save
python -m fastanim render fastanim-py/scenes/parity.py -q 720p30 -o parity.mp4
python -m fastanim still fastanim-py/scenes/parity.py --at 3s -o frame.svg
```

The [scripting docs](https://simonhdickson.github.io/fastanim/docs/) cover the Python API
(source in `docs/book`, `just docs` to serve locally).
What's planned next for manim parity is in [`docs/ROADMAP.md`](docs/ROADMAP.md).

## Features

- **Diff engine**: Myers (greedy and linear-space) and patience diff, with move detection,
  replace pairing and semantic cleanup.
- **Core model**: cubic-Bézier paths and shapes, path alignment, Oklab colors, a `Scene` builder
  baked into a seekable `BakedTimeline`, and SVG export of stills or whole animations.
- **Animations**: `create`, `write`, `fade_in`/`fade_out` (optionally shifting or scaling),
  `transform`, `replacement_transform`, `grow_from_center`, `spin_in`, `draw_border_then_fill`, `uncreate`,
  `unwrite`, `shrink_to_center`, `move_to`, `apply_function`, `indicate`, `wiggle`,
  `circumscribe`, `flash`; composed with `Parallel`, `Sequence` and `lagged_start`; eased with
  quad, cubic, expo, back and spring rate functions.
- **Styles**: solid or linear-gradient fills and outlines with separate opacities,
  `set_color_by_gradient` across groups and texts, dashed outlines and `dashed_line`, line caps
  and joins, and a scene background colour.
- **Shader fills**: `shader(wgsl)` paints a shape's fill with a WGSL fragment shader
  (`fn shade(uv: vec2<f32>) -> vec4<f32>`, reading `shape.color`, `shape.size` and
  `shape.time`) on the GPU in preview and export; SVG and the web player draw the solid fill.
- **Layout**: `next_to`, `align_to`, `to_edge`, `arrange`, `scale_to_fit_height`/`width`.
- **Graphs**: manim-style `axes` (with arrow tips and labels) and `fit_axes`, which picks the
  y range; `plot` graphs a function, `c2p` maps graph to scene coordinates.
- **Text & math**: [Typst](https://typst.app) (bundled fonts, no LaTeX) laid out into
  per-glyph shapes and diffable tokens via `text`, `math_tex` and `code`. `latex` takes
  LaTeX math instead, converted to Typst by [MiTeX](https://github.com/mitex-rs/mitex).
  `load_font` adds TTF/OTF fonts from files, chosen per text with a `font` option.
- **Images**: PNG and JPEG files, or pixel arrays (NumPy or nested lists), as shapes
  that move, scale, rotate, fade and group like any other; smooth or pixelated.
- **Video**: any file `ffmpeg` reads, as an image shape whose frames `play_video` plays, with
  its sound, while it moves, rotates and distorts like any other shape.
- **Lottie**: Lottie (Bodymovin JSON) files evaluated frame by frame into shapes, so they
  render everywhere and move, fade and group like any other mobject; `play_lottie` plays all
  or part of one, looped or backwards.
- **`TransformDiff`**: token diffs choreographed into slides, arcing moves, morphs and fades,
  with `DiffStyle` phasing, highlights and a debug tint. Code diffs line by line, then token by
  token; `list` cells swap with two arcing moves.
- **Preview**: `FastanimPlugin` renders with Vello, with a scene clock and scrubber
  (Space play/pause, ←/→ step, `[`/`]` markers, drag the bar).
- **Export**: headless rendering piped to ffmpeg as MP4, WebM or GIF, or PNG/SVG frames, with
  quality presets, `--section` and stills. Frames are evaluated in parallel across cores.
- **Sound**: WAV clips played whole or in part, as animations that `play` waits for or in the
  background with `add_sound`; heard in the preview and mixed into MP4 and WebM exports.
- **Python**: the `fastanim` package (built with [maturin](https://www.maturin.rs)) records
  `@scene` functions and bakes them to the same timelines as Rust and Rhai. `python -m fastanim
  preview` re-runs a scene on save; `render` and `still` export it. Typeset text is memoized,
  so a re-run only typesets what changed.
- **Rhai, locally**: the `fastanim` CLI plays Rhai scripts with `preview`, `render` and
  `still`; `fastanim profile` shows where bake time goes, `fastanim script-api` lists every
  script function, and `fastanim lsp` gives editors errors, completion and hover.
- **Browser playground**: `fastanim-web` runs Python scenes on Pyodide in a Web Worker and
  draws them onto a Canvas 2D.

## Crates

| Crate             | Purpose                                                        |
| ----------------- | -------------------------------------------------------------- |
| `fastanim`        | Facade crate and prelude                                       |
| `fastanim-diff`   | Generic sequence diffing                                       |
| `fastanim-core`   | Geometry, mobjects, animations and timeline                    |
| `fastanim-text`   | Text, math and code layout into diffable tokens                |
| `fastanim-bevy`   | Bevy plugin: rendering, preview and export                     |
| `fastanim-script` | Rhai scenes, and the bindings Python shares                    |
| `fastanim-cli`    | The `fastanim` command-line tool                               |
| `fastanim-py`     | The `fastanim` Python package                                  |
| `fastanim-web`    | Browser player and playground, for Python scenes               |

## Try it

From a checkout, build the package into a virtualenv with [maturin](https://www.maturin.rs):

```sh
pip install maturin
maturin develop -m fastanim-py/pyproject.toml
python -m fastanim preview fastanim-py/scenes/diff.py   # equations, code refactor, bubble sort, morph
python -m fastanim preview fastanim-py/scenes/sin_and_cos.py   # axes and plots
python -m fastanim render fastanim-py/scenes/diff.py -q 720p30 -o diff.mp4
just web   # browser playground on http://localhost:8080, rebuilt on change
```

The playground needs [Trunk](https://trunk-rs.github.io/trunk/) (`cargo binstall trunk`) and
the Pyodide wheel from `just pyodide-wheel`, which needs [uv](https://docs.astral.sh/uv/).

The Rust side, and Rhai scripts with the `fastanim` CLI (Rust 1.92+; on Linux, building needs
`libudev-dev`, `libwayland-dev` and `libxkbcommon-dev`, plus `libasound2-dev` for sound):

```sh
cargo install --locked --git https://github.com/simonhdickson/fastanim fastanim-cli
fastanim preview fastanim-script/scenes/parity.rhai   # the same scene as a Rhai script

cargo run -p fastanim-cli -- diff "a + b = c" "b + a = c"
# ↷b =+ ↷a == =c
cargo run -p fastanim-cli -- diff "a^2 + b^2 = c^2" "a^2 = c^2 - b^2" --math
# =𝑎 =2' == =𝑐 =2' ~(+→−) ↷𝑏 ↷2'

cargo run -p fastanim --example shapes -- frames   # writes frames/000.svg …
cargo run -p fastanim-bevy --example parity        # live preview window
cargo run -p fastanim-bevy --example parity -- render -q 720p30 -o parity.mp4
cargo run -p fastanim-cli -- profile fastanim-script/scenes/diff.rhai   # bake timings
```

Prebuilt `fastanim` binaries for Linux (x86_64), macOS (Apple Silicon) and Windows (x86_64) are
attached to each [GitHub release](https://github.com/simonhdickson/fastanim/releases), and
install with `cargo binstall --git https://github.com/simonhdickson/fastanim fastanim-cli`.

## License

MIT
