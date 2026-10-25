# Gotchas

## Add before animating

Animations take the handle `s.add` returns, not the shape. Animating something that was
never added, or was removed, fails with `not in the scene`.

## Shapes vs. texts

`s.get`, `s.always`, `transform` and `replacement_transform` need shapes. Texts are many
glyphs; change them with `transform_diff`.

## Methods vs. animations

`circle(1).shift(UP)` is a shape, moved before it is added. `shift(m, UP)` and
`m.animate.shift(UP)` are animations of a mobject already in the scene. The same goes for
`move_to`, `scale` and `rotate`.

## Closures run later

Functions passed to `s.always`, `update`, `apply_function`, `function_graph` and `plot` are
called during baking, not where they are written. They cannot use the scene; see
[updaters](updaters.md).

## `list` is fastanim's

`from fastanim import *` brings in [`list`](text.md), replacing Python's own. Use
`builtins.list` for Python's, or import fastanim by name.

## Paths are relative to the current directory

`image`, `video`, `lottie`, `audio` and `load_font` resolve a relative path against the current
directory, not the scene file's. Build paths from `__file__` to load files beside the scene
wherever it is run from:

```python
from pathlib import Path

HERE = Path(__file__).parent
photo = image(HERE / "photo.png")
```
