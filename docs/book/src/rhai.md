# Rhai scripts

Scenes can also be written as [Rhai](https://rhai.rs) scripts and run with the `fastanim` CLI,
with no Python install. The API is the one in the rest of this book; the differences are
below. Rhai scripts run locally only: the browser playground runs Python.

```rhai
// hello.rhai: fastanim preview hello.rhai
let title = scene.add(text("Hello, fastanim").to_edge(UP));
scene.play(write(title));

let ring = scene.add(circle(1.5).stroke(BLUE, 0.06));
scene.play(create(ring).run_time(1.5));

let formula = scene.add(math_tex("y = sin(x)").scale(1.5).fill(GREEN).to_edge(DOWN));
scene.play(write(formula));
scene.wait(1.0);
```

The example scenes are in
[`fastanim-script/scenes`](https://github.com/simonhdickson/fastanim/tree/main/fastanim-script/scenes).

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

## Running

| Command | What it does |
| --- | --- |
| `fastanim preview <script>` | Opens a window with a timeline, re-baking on save |
| `fastanim render <script>` | Exports video or frames |
| `fastanim still <script>` | Exports one frame |
| `fastanim check <script>...` | Runs scripts without a window and reports their errors |
| `fastanim profile <script> [--runs N]` | Bakes the scene cold, then N times warm, and prints time spent typesetting, planning animations, copying keyframes and running the script |
| `fastanim script-api` | Lists every function and constant a script can call |

`preview`, `render` and `still` take the keys and options of
[`python -m fastanim`](running.md).

### Check

```sh
fastanim check scene.rhai
fastanim check scenes/*.rhai --fps 60
```

Runs each script without a window, then samples its timeline `--fps` times a second (30 by
default, 0 to skip), running every [updater](updaters.md) and animation closure along the way.
A script only tries an updater when it is added and an animation's closure at its end, so
sampling finds closures that fail part way through. Good scripts print `path: ok` and their
duration; errors go to stderr in the format below, and the exit status is non-zero if any
script failed, so it suits CI and an editor's `:make`. Errors found while sampling are
prefixed with the time, as in `scene.rhai: at 0.50s: ...`.

### Errors

Errors are reported as `path:line:col: message`, for example:

```text
scene.rhai:3:14: Function not found: point (i64, i64)
```

## Editors

`fastanim lsp` is a language server on stdin and stdout. Point your editor's LSP client for
`.rhai` files at it. It bakes each open script as you type and marks the error, if any, at its
line and column. It also completes function and constant names, and the variables, functions and
parameters the script declares. Hover shows a function's signatures or the line declaring a name.
Output from a script's `print` and `debug` goes to stderr.

Both setups below expect `fastanim` on your `PATH`.

### Neovim

Neovim 0.11 or later. Add to your `init.lua`:

```lua
vim.filetype.add({ extension = { rhai = "rhai" } })
vim.lsp.config("fastanim", {
  cmd = { "fastanim", "lsp" },
  filetypes = { "rhai" },
  root_markers = { ".git" },
})
vim.lsp.enable("fastanim")
```

Errors show as diagnostics. Completion comes through `<C-x><C-o>` or your completion plugin.
Hover is `K`. `:checkhealth vim.lsp` shows whether the server started.

### Zed

Zed starts a language server only when an extension declares it, so fastanim ships one in
`editors/zed`. Zed compiles it to WebAssembly itself, which needs Rust installed through
[rustup](https://rustup.rs).

1. Install the `rhai` extension from the extensions page (`zed: extensions`). It gives Zed the
   Rhai language that the server attaches to.
2. Clone the fastanim repository. Run `zed: install dev extension` and pick its `editors/zed`
   directory.
3. Open a `.rhai` file. `dev: open language server logs` shows the server's output and any
   error starting it.

## Differences from Python

| Python | Rhai |
| --- | --- |
| `def construct(s)` / `@scene` | A global `scene`; the script runs top to bottom |
| `circle(1, stroke=BLUE, stroke_width=0.06)` | `circle(1.0).stroke(BLUE, 0.06)` |
| `s.play(a, b, run_time=2)` | `scene.play([a, b])`, `scene.play(a.run_time(2.0))` |
| `fade_in(m, shift=RIGHT, scale=0.5)` | `fade_in(m, RIGHT, 0.5)` |
| `group(a, b)`, `parallel(a, b)`, `sequence(a, b)` | `group([a, b])`, `parallel([a, b])`, `sequence([a, b])` |
| `lagged_start(a, b, lag_ratio=0.2)` | `lagged_start(0.2, [a, b])` |
| `transform_diff(m, t, highlight_changes=True)` | `transform_diff(m, t, #{ highlight_changes: true })` |
| `text("x", font="Inter")` | `text("x", #{ font: "Inter" })` |
| `play_lottie(m, loops=2)` | `play_lottie(m, #{ loops: 2 })` |
| `play_video(m, start=1)` | `play_video(m, #{ start: 1.0 })` |
| `image("a.png", height=4)` | `image("a.png").scale_to_fit_height(4.0)` |
| `.width`, `.height` | `.width()`, `.height()` |
| `lambda x: x * 2`, called as `f(1.5)` | `\|x\| x * 2.0`, called as `f.call(1.5)` |
| `@s.always(m)` decorator | `scene.always(m, \|state, t\| ...)` only |
| Paths relative to the current directory | Paths relative to the script's directory |
| Animations can't be named yet | `.named("title")` labels a block in the preview's timeline |

### Write floats as floats

API functions take floats, and Rhai does not convert integers. `point(1, 0)` fails with
`Function not found: point (i64, i64)`. Write `point(1.0, 0.0)`, and convert loop indices with
`.to_float()`:

```rhai
for i in 0..5 {
    scene.add(dot(point(i.to_float(), 0.0)));
}
```

Axes ranges are the exception: `axes([-3, 3], [-1, 1])` accepts integers.

### Closures and functions

Closures capture variables from where they are written. Functions declared with `fn` cannot
see outer variables; pass `scene` and anything else in as arguments:

```rhai
fn fade_all(scene, mobs) {
    scene.play(mobs.map(|m| fade_out(m)));
    for m in mobs {
        scene.remove(m);
    }
}
```

Errors in closures are reported at the next `scene.play`.

### Strings

Backtick strings span lines, which suits `code` and LaTeX:

```rhai
const SRC = `fn main() {
    println!("hi");
}`;
scene.add(code(SRC, "rust").scale(0.8));
scene.add(latex(`\frac{a}{b}`));
```

### The standard library

Rhai's standard library is available: `sin`, `cos`, `min`, `max`, `abs`, `sqrt`, `to_float`,
`len`, `map`, `filter`, `print` and so on. See the [Rhai book](https://rhai.rs/book/).

### Limits

A script that runs too long or recurses too deep fails rather than hanging: it gets 10 million
operations and 64 call levels.
