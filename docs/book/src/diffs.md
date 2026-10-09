# Diff transforms

`transform_diff(m, target)` turns text `m` into text `target` by diffing their tokens with
Myers' diff. Unchanged tokens stay put or slide, moved tokens travel on an arc, and only real
changes fade or morph. It works on any text: `text`, `math_tex`, `latex` and `code`.

```rhai
let eq = scene.add(math_tex("a^2 + b^2 = c^2").scale(1.5));
scene.play(write(eq));
scene.play(transform_diff(eq, math_tex("a^2 = c^2 - b^2").scale(1.5)).run_time(1.5));
```

After the transform, `m` holds the new text, so it can be diffed again.

Code diffs line by line, then token by token within changed lines:

```rhai
let src = scene.add(code(BEFORE, "rust"));
scene.play(transform_diff(src, code(AFTER, "rust")).run_time(2.0));
```

## Options

A third argument, a map, tunes the choreography:

```rhai
scene.play(transform_diff(eq, math_tex("a = sqrt(c^2 - b^2)"), #{ highlight_changes: true }));
```

| Option | Type | Default | Effect |
| --- | --- | --- | --- |
| `highlight_changes` | bool | `false` | Flash the tokens that changed |
| `lag_ratio` | float | `0.05` | Stagger between token moves |
| `move_arc` | float | `PI / 3` | Arc angle of moved tokens, in radians |
| `debug` | bool | `false` | Tint tokens by how they were matched |

## Lists

`transform_list(m, target)` diffs two [`list`](text.md)s cell by cell. Two cells that swap
places trade with a pair of arcing moves, which makes sorting algorithms easy to show:

```rhai
let xs = scene.add(list([3, 1, 2]));
scene.play(transform_list(xs, list([1, 3, 2])));
scene.play(transform_list(xs, list([1, 2, 3])));
```
