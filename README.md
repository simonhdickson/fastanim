# fastanim

A programmatic mathematical-animation engine in Rust, built on [Bevy](https://bevyengine.org),
inspired by [manim](https://github.com/ManimCommunity/manim).

Its distinguishing feature: transforms between equations, text, code and groups of shapes are
planned with **Myers' diff**, so unchanged parts stay put or slide, moved parts travel, and only
real changes fade or morph.

## Install

Prebuilt binaries for Linux (x86_64), macOS (Apple Silicon) and Windows (x86_64) are attached
to each [GitHub release](https://github.com/simonhdickson/fastanim/releases). With
[cargo-binstall](https://github.com/cargo-bins/cargo-binstall) they install without compiling:

```sh
cargo binstall --git https://github.com/simonhdickson/fastanim fastanim-cli
```

Or build from source with `cargo install` (Rust 1.92+):

```sh
cargo install --locked --git https://github.com/simonhdickson/fastanim fastanim-cli
```

On Linux, building needs `libudev-dev`, `libwayland-dev` and `libxkbcommon-dev` (or your
distro's equivalents), plus `libasound2-dev` for sound in the preview. Either way you get a
`fastanim` binary; video export also needs `ffmpeg` on your `PATH`.

## A taste

Scenes can be written in Rust, as [Rhai](https://rhai.rs) scripts or in Python, with the same API:

```rhai
let title = scene.add(text("Classic manim parity").to_edge(UP));
scene.play(write(title));
let circle = scene.add(circle(1.5).stroke(BLUE, 0.06));
scene.play(create(circle).run_time(1.5));

let formula = scene.add(math_tex("y = sin(x)").scale(1.5).fill(GREEN).to_edge(DOWN));
scene.play(write(formula));
```

```sh
cargo run -p fastanim-cli -- preview fastanim-script/scenes/parity.rhai
```

The same scene in Python (`fastanim-py/scenes/parity.py`):

```python
from fastanim import *


@scene
def parity(s: Scene) -> None:
    title = s.add(text("Classic manim parity").to_edge(UP))
    s.play(write(title))
    ...


parity.still("parity.svg", at=3)  # or parity.bake().svg(t)
```

The [scripting docs](https://simonhdickson.github.io/fastanim/docs/) cover the Rhai API
(source in `docs/book`, `just docs` to serve locally).
What's planned next for manim parity is in [`docs/ROADMAP.md`](docs/ROADMAP.md).

## Features

- **Diff engine**: Myers (greedy and linear-space) and patience diff, with move detection,
  replace pairing and semantic cleanup.
- **Core model**: cubic-Bézier paths and shapes, path alignment, Oklab colors, a `Scene` builder
  baked into a seekable `BakedTimeline`, and SVG still export.
- **Animations**: `create`, `write`, `fade_in`/`fade_out` (optionally shifting or scaling),
  `transform`, `replacement_transform`, `grow_from_center`, `spin_in`, `draw_border_then_fill`, `uncreate`,
  `unwrite`, `shrink_to_center`, `move_to`, `apply_function`, `indicate`, `wiggle`,
  `circumscribe`, `flash`; composed with `Parallel`, `Sequence` and `lagged_start`; eased with
  quad, cubic, expo, back and spring rate functions.
- **Styles**: solid or linear-gradient fills and outlines with separate opacities,
  `set_color_by_gradient` across groups and texts, dashed outlines and `dashed_line`, line caps
  and joins, and a scene background colour.
- **Layout**: `next_to`, `align_to`, `to_edge`, `arrange`, `scale_to_fit_height`/`width`.
- **Graphs**: manim-style `axes` (with arrow tips and labels) and `fit_axes`, which picks the
  y range; `plot` graphs a function, `c2p` maps graph to scene coordinates.
- **Text & math**: [Typst](https://typst.app) (bundled fonts, no LaTeX) laid out into
  per-glyph shapes and diffable tokens via `text`, `math_tex` and `code`. `latex` takes
  LaTeX math instead, converted to Typst by [MiTeX](https://github.com/mitex-rs/mitex).
  `load_font` adds TTF/OTF fonts from files, chosen per text with a `font` option.
- **Images**: PNG and JPEG files, or pixel arrays (Rhai arrays, NumPy in Python), as shapes
  that move, scale, rotate, fade and group like any other; smooth or pixelated.
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
- **Scripting**: `fastanim preview` plays Rhai scenes, re-baking on save; `fastanim render` and `fastanim still` export them.
  Typeset text is memoized, so a re-bake only typesets what changed. `fastanim profile` shows
  where bake time goes; `fastanim script-api` lists every script function.
- **Python**: `fastanim-py` (built with [maturin](https://www.maturin.rs)) records `@scene`
  functions and bakes them to the same timelines as Rust and Rhai, with SVG stills. Preview and
  video from Python are not available yet.
- **Browser playground**: `fastanim-web` bakes and draws scripts in a Web Worker onto a
  Canvas 2D, reporting time spent typesetting vs. baking.

## Crates

| Crate             | Purpose                                                        |
| ----------------- | -------------------------------------------------------------- |
| `fastanim`        | Facade crate and prelude                                       |
| `fastanim-diff`   | Generic sequence diffing                                       |
| `fastanim-core`   | Geometry, mobjects, animations and timeline                    |
| `fastanim-text`   | Text, math and code layout into diffable tokens                |
| `fastanim-bevy`   | Bevy plugin: rendering, preview and export                     |
| `fastanim-script` | Rhai scenes                                                    |
| `fastanim-cli`    | The `fastanim` command-line tool                               |
| `fastanim-py`     | The `fastanim` Python package                                  |
| `fastanim-web`    | Browser player and playground                                  |

## Try it

```sh
cargo run -p fastanim-cli -- diff "a + b = c" "b + a = c"
# ↷b =+ ↷a == =c
cargo run -p fastanim-cli -- diff "a^2 + b^2 = c^2" "a^2 = c^2 - b^2" --math
# =𝑎 =2' == =𝑐 =2' ~(+→−) ↷𝑏 ↷2'

cargo run -p fastanim --example shapes -- frames   # writes frames/000.svg …
cargo run -p fastanim-bevy --example parity        # live preview window
cargo run -p fastanim-bevy --example parity -- render -q 720p30 -o parity.mp4
cargo run -p fastanim-bevy --example parity -- still --at 3s -o frame.png
cargo run -p fastanim-bevy --example diff          # equations, code refactor, bubble sort, morph
cargo run -p fastanim-cli -- preview fastanim-script/scenes/diff.rhai   # same scene as a script
cargo run -p fastanim-cli -- preview fastanim-script/scenes/sin_and_cos.rhai  # axes and plots
cargo run -p fastanim-cli -- profile fastanim-script/scenes/diff.rhai   # bake timings
(cd fastanim-py && maturin develop)   # Python package, into the active virtualenv
(cd fastanim-py/scenes && python -c "import parity; parity.parity.still('parity.svg', at=3)")
just web   # browser playground on http://localhost:8080, rebuilt on change
```

Scenes can also be written in Python. Build the package into a virtualenv with
[maturin](https://www.maturin.rs), adding the `preview` wheel for video, PNG and the window:

```sh
pip install maturin
maturin develop -m fastanim-py/pyproject.toml           # recording, baking and SVG
maturin develop -m fastanim-py/preview/pyproject.toml   # + video, PNG and preview
python -m fastanim preview fastanim-py/scenes/parity.py # reloads on save
python -m fastanim render fastanim-py/scenes/diff.py -q 720p30 -o diff.mp4
python -m fastanim still fastanim-py/scenes/parity.py --at 3s -o frame.svg
```

Video export needs `ffmpeg` on your `PATH`. The playground needs the
`wasm32-unknown-unknown` target and [Trunk](https://trunk-rs.github.io/trunk/)
(`cargo binstall trunk`), which fetches the matching `wasm-bindgen` itself.

## License

MIT
