# Introduction

fastanim is a mathematical-animation engine in the spirit of
[manim](https://github.com/ManimCommunity/manim). Scenes can be written in Rust or as
[Rhai](https://rhai.rs) scripts; this book covers the scripts.

A scene script runs once, top to bottom. It adds shapes and text to a global `scene` and plays
animations on it. What it records is baked into a timeline, so playback, scrubbing and export
never run the script again.

```rhai
// A circle draws itself, then a formula is written under it.
// fastanim preview hello.rhai

let title = scene.add(text("Hello, fastanim").to_edge(UP));
scene.play(write(title));

let ring = scene.add(circle(1.5).stroke(BLUE, 0.06));
scene.play(create(ring).run_time(1.5));

let formula = scene.add(math_tex("y = sin(x)").scale(1.5).fill(GREEN).to_edge(DOWN));
scene.play(write(formula));
scene.wait(1.0);
```

Save that as `hello.rhai` and run `fastanim preview hello.rhai`. The window re-bakes the scene
every time you save the file.

## Install

Prebuilt binaries are attached to each
[GitHub release](https://github.com/simonhdickson/fastanim/releases). With
[cargo-binstall](https://github.com/cargo-bins/cargo-binstall):

```sh
cargo binstall --git https://github.com/simonhdickson/fastanim fastanim-cli
```

Or build from source (Rust 1.92+):

```sh
cargo install --locked --git https://github.com/simonhdickson/fastanim fastanim-cli
```

Video export also needs `ffmpeg` on your `PATH`.

## Try it in the browser

The [playground](../) runs scripts in the browser with no install. It ships with the example
scenes from
[`fastanim-script/scenes`](https://github.com/simonhdickson/fastanim/tree/main/fastanim-script/scenes).
