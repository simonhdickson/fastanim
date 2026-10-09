# Running scenes

The `fastanim` CLI runs scripts in four ways.

| Command | What it does |
| --- | --- |
| `fastanim preview <script>` | Opens a window with a scrubber, re-baking on save |
| `fastanim render <script>` | Exports video or frames |
| `fastanim still <script>` | Exports one frame |
| `fastanim bundle <script>` | Writes the scene's typeset text to a `.bundle` for the web player |

## Preview

```sh
fastanim preview scene.rhai
```

| Key | Action |
| --- | --- |
| Space | Play / pause |
| ← / → | Step a frame |
| `[` / `]` | Jump to the previous / next [marker](scene.md#markers) |
| Drag the bar | Scrub |

Typeset text is cached, so a re-bake only typesets text that changed.

## Render

```sh
fastanim render scene.rhai -q 720p30 -o scene.mp4
fastanim render scene.rhai --section intro -o intro.webm
```

- `-q, --quality`: `480p15`, `720p30`, `1080p60` (default) or `4k60`.
- `-o, --output`: the extension picks the format: `mp4`, `webm`, `gif`, `png` or `svg`.
  `png` and `svg` write one file per frame.
- `--section <marker>`: render only from that marker to the next one.

## Still

```sh
fastanim still scene.rhai --at 2.5s -o frame.png
fastanim still scene.rhai --frame 120 -o frame.svg
```

`--at` takes seconds (`3.5` or `3.5s`); `--frame` takes a frame index at the chosen quality.

## Bundle

The web player typesets text slowly. `fastanim bundle scene.rhai` runs the script without a
window and writes `scene.bundle` beside it, holding the pre-typeset text. It is also the quickest
way to check a script for errors.

## Errors

Errors are reported as `path:line:col: message`, for example:

```text
scene.rhai:3:14: Function not found: point (i64, i64)
```
