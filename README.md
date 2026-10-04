# fastanim

A programmatic mathematical-animation engine in Rust, built on [Bevy](https://bevyengine.org),
inspired by [manim](https://github.com/ManimCommunity/manim).

Its distinguishing feature: transforms between equations, text, code and groups of shapes are
planned with **Myers' diff**, so unchanged parts stay put or slide, moved parts travel, and only
real changes fade or morph.

Status: early development. See the [design specification](docs/SPEC.md).

Done so far: the workspace skeleton (M0), the `fastanim-diff` engine (M1): Myers greedy and
linear-space, patience, move detection, replace pairing and semantic cleanup; and the core model
(M2): cubic-Bézier paths, shapes, path alignment, Oklab colors, animations, the `Scene` builder,
the seekable `BakedTimeline` and SVG still export; and the Bevy preview (M3): `FastanimPlugin` with
Vello rendering, a scene clock and a scrubber (Space play/pause, ←/→ step, `[`/`]` markers, drag
the bar); and video export (M4): headless Vello rendering piped to ffmpeg as MP4, WebM, GIF, PNG
or SVG, with quality presets, `--section` and stills; and text & math (M5): Typst (bundled
fonts, no LaTeX) laid out into per-glyph shapes and diffable tokens, `math_tex`, `text` and
`write`; and `TransformDiff` (M6): token diffs choreographed into slides, arcing moves,
morphs and fades, with `DiffStyle` phasing, highlights and a debug tint; and code & lists (M7):
`code` highlighted by Typst, diffed line by line then token by token, and `list` cells whose
swaps are two arcing moves. Polish (M8) so far: the rest of the standard animations
(`grow_from_center`, `spin_in`, `draw_border_then_fill`, `uncreate`, `unwrite`,
`shrink_to_center`, `move_to`, `replacement_transform`, `apply_function`, `indicate`, `wiggle`,
`circumscribe`, `flash`), the `Position` helpers (`next_to`, `align_to`, `to_edge`, `arrange`), `Sequence` and
`lagged_start`, and ease-in/out (quad, cubic, expo, back) and spring rate functions. Scripting
(M9) so far: `fastanim-script` runs [Rhai](https://rhai.rs) scenes with the same API, and
`fastanim run` previews them (re-baking on save) or exports them; typeset text is memoized, so a
re-bake only typesets what changed; and `fastanim-web` plays scripts in the browser on a Canvas 2D
playground, baking and drawing in a Web Worker and reporting time spent typesetting vs. baking. Try them with:

```sh
cargo run -p fastanim-cli -- diff "a + b = c" "b + a = c"
# ↷b =+ ↷a == =c
cargo run -p fastanim-cli -- diff "a^2 + b^2 = c^2" "a^2 = c^2 - b^2" --math
# =𝑎 =2' == =𝑐 =2' ~(+→−) ↷𝑏 ↷2'

cargo run -p fastanim --example shapes -- frames   # writes frames/000.svg …
cargo run -p fastanim-bevy --example parity        # live preview window
cargo run -p fastanim-bevy --example parity -- render -q 720p30 -o parity.mp4   # needs ffmpeg
cargo run -p fastanim-bevy --example parity -- still --at 3s -o frame.png
cargo run -p fastanim-bevy --example diff          # equations, code refactor, bubble sort, morph
cargo run -p fastanim-cli -- run fastanim-script/scenes/diff.rhai   # same scene as a script
just web   # browser playground
```
