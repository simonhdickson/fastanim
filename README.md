# ranim

A programmatic mathematical-animation engine in Rust, built on [Bevy](https://bevyengine.org),
inspired by [manim](https://github.com/ManimCommunity/manim).

Its distinguishing feature: transforms between equations, text, code and groups of shapes are
planned with **Myers' diff**, so unchanged parts stay put or slide, moved parts travel, and only
real changes fade or morph.

Status: early development. See the [design specification](docs/SPEC.md).

Done so far: the workspace skeleton (M0) and the `ranim-diff` engine (M1): Myers greedy and
linear-space, patience, move detection, replace pairing and semantic cleanup. Try it with:

```sh
cargo run -p ranim-cli -- diff "a + b = c" "b + a = c"
# ↷b =+ ↷a == =c
```
