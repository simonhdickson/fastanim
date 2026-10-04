# fastanim

A programmatic mathematical-animation engine in Rust, built on [Bevy](https://bevyengine.org),
inspired by [manim](https://github.com/ManimCommunity/manim).

Its distinguishing feature: transforms between equations, text, code and groups of shapes are
planned with **Myers' diff**, so unchanged parts stay put or slide, moved parts travel, and only
real changes fade or morph.

## A taste

Scenes can be written in Rust or as [Rhai](https://rhai.rs) scripts with the same API:

```rhai
let title = scene.add(text("Classic manim parity").to_edge(UP));
scene.play(write(title));
let circle = scene.add(circle(1.5).stroke(BLUE, 0.06));
scene.play(create(circle).run_time(1.5));

let formula = scene.add(math_tex("y = sin(x)").scale(1.5).fill(GREEN).to_edge(DOWN));
scene.play(write(formula));
```

```sh
cargo run -p fastanim-cli -- run fastanim-script/scenes/parity.rhai
```

## Features

- **Diff engine**: Myers (greedy and linear-space) and patience diff, with move detection,
  replace pairing and semantic cleanup.
- **Core model**: cubic-Bézier paths and shapes, path alignment, Oklab colors, a `Scene` builder
  baked into a seekable `BakedTimeline`, and SVG still export.
- **Animations**: `create`, `write`, `fade_in`/`fade_out`, `transform`,
  `replacement_transform`, `grow_from_center`, `spin_in`, `draw_border_then_fill`, `uncreate`,
  `unwrite`, `shrink_to_center`, `move_to`, `apply_function`, `indicate`, `wiggle`,
  `circumscribe`, `flash`; composed with `Parallel`, `Sequence` and `lagged_start`; eased with
  quad, cubic, expo, back and spring rate functions.
- **Layout**: `next_to`, `align_to`, `to_edge`, `arrange`.
- **Text & math**: [Typst](https://typst.app) (bundled fonts, no LaTeX) laid out into
  per-glyph shapes and diffable tokens via `text`, `math_tex` and `code`.
- **`TransformDiff`**: token diffs choreographed into slides, arcing moves, morphs and fades,
  with `DiffStyle` phasing, highlights and a debug tint. Code diffs line by line, then token by
  token; `list` cells swap with two arcing moves.
- **Preview**: `FastanimPlugin` renders with Vello, with a scene clock and scrubber
  (Space play/pause, ←/→ step, `[`/`]` markers, drag the bar).
- **Export**: headless rendering piped to ffmpeg as MP4, WebM or GIF, or PNG/SVG frames, with
  quality presets, `--section` and stills.
- **Scripting**: `fastanim run` previews Rhai scenes, re-baking on save, or exports them.
  Typeset text is memoized, so a re-bake only typesets what changed.
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
cargo run -p fastanim-cli -- run fastanim-script/scenes/diff.rhai   # same scene as a script
just web   # browser playground on http://localhost:8000
```

Video export needs `ffmpeg` on your `PATH`. The playground needs the
`wasm32-unknown-unknown` target and `wasm-bindgen-cli` at the version in `Cargo.lock`
(see `fastanim-web/build.sh`).

## License

MIT
