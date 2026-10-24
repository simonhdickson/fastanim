# Shapes, points and colors

## Points

`point(x, y)` makes a point, which also serves as a vector. Points add, subtract, negate, and
multiply or divide by a number. Read the parts with `.x` and `.y`. Anywhere a point is taken,
an `(x, y)` tuple works too.

```python
p = point(1, 2) + RIGHT * 3   # point(4, 2)
mid = (p + ORIGIN) / 2
print(mid.y)                  # 1.0
d = dot((4, 3))
```

## Colors

| Function | Example |
| --- | --- |
| Named constants | `WHITE`, `BLACK`, `BLUE`, `RED`, `GREEN`, `YELLOW`, `GREY`, `ORANGE`, `PINK`, `PURPLE`, `TRANSPARENT` |
| `rgb(r, g, b, a=1.0)` | `rgb(1, 0.5, 0)`, each 0–1 |
| `hex(rgb)` | `hex(0xf4d345)` |
| A hex string | `"#f4d345"` |
| `.with_alpha(a)` | `BLUE.with_alpha(0.5)` |

## Shapes

All shapes are centered on the origin unless they take points.

| Function | Shape |
| --- | --- |
| `circle(radius)` | Circle |
| `square(side)` | Square |
| `rectangle(width, height)` | Rectangle |
| `ellipse(width, height)` | Ellipse |
| `arc(radius, start, sweep)` | Arc, `sweep` radians counter-clockwise from `start` |
| `line(a, b)` | Line from point `a` to point `b` |
| `dot(p)` | Small filled white dot at `p` |
| `polygon([p1, p2, ...])` | Closed polygon through the points |
| `polyline([p1, p2, ...])` | Open path of straight segments through the points |
| `dashed_line(a, b, dash_length=0.05, ratio=0.5)` | Dashed line from `a` to `b`, dashes about `dash_length` long |
| `function_graph(f, x0, x1, segments)` | Graph of `y = f(x)` for `x` in `x0..x1`, in scene units |

Each takes `stroke=`, `stroke_width=`, `fill=` and `fill_opacity=` keywords to style it as it is
made (`dashed_line` only the first two).

```python
from math import sin

wave = function_graph(sin, -PI, PI, 32)
ring = circle(1.5, stroke=BLUE, stroke_width=0.06)
```

`union(a, b)`, `intersection(a, b)`, `difference(a, b)` (`a` without `b`) and
`exclusion(a, b)` (one or the other, not both) make a new shape from the areas two shapes
cover, in the default style.

```python
lens = intersection(circle(1).shift(LEFT * 0.5), circle(1).shift(RIGHT * 0.5))
```

## Styling

Style methods return a new shape, so they chain.

| Method | Effect |
| --- | --- |
| `.fill(color)` | Fill color |
| `.fill(color, opacity)` | Fill color and opacity, 0–1 |
| `.fill_opacity(opacity)` | Fill opacity, keeping the color or gradient |
| `.stroke(color, width)` | Outline color and width |
| `.stroke_opacity(opacity)` | Outline opacity, keeping the color or gradient |
| `.fill_gradient([c1, c2, ...])` | Fill with a linear gradient from left to right |
| `.fill_gradient([c1, c2, ...], dir)` | The same, across the shape along `dir`, e.g. `UP` |
| `.stroke_gradient([c1, c2, ...])` | Outline with a linear gradient, left to right; `dir` as for fills |
| `.line_cap(cap)` | Ends of open outlines: `"round"` (default), `"butt"` or `"square"` |
| `.line_join(join)` | Corners: `"round"` (default), `"miter"` or `"bevel"` |
| `.dashed()` | The outline cut into 15 dashes, each half its period |
| `.dashed(n)` / `.dashed(n, ratio)` | `n` dashes, each `ratio` (0–1) of its period |
| `.shader(wgsl)` | Paint the fill with a WGSL shader; see [Shader fills](shaders.md) |
| `.z_index(n)` | Draw order; higher draws on top |
| `.rotate(angle)` | Rotate by `angle` radians |

```python
sq = s.add(square(2).fill(BLUE, 0.5).stroke(WHITE, 0.04).rotate(PI / 4))
warm = s.add(circle(1).fill_gradient([YELLOW, ORANGE, RED], UP))
ring = s.add(circle(1.5).stroke(WHITE, 0.1).stroke_gradient([BLUE, PINK]).dashed(24))
```

`.fill` and `.stroke` replace a gradient with a solid color, so set a gradient after them. A
gradient spans the shape's box when it is set and then moves, scales and rotates with the
shape; `transform` blends one gradient into another, or into a solid color. Fading changes
its opacity like a solid color's.

Dashes are geometry: a dashed shape is many short open pieces, so `create` draws it dash by
dash, and a fill of a dashed shape fills each piece on its own. An open outline starts and ends
on a dash; a closed one repeats evenly around.

Texts take `.fill(color, opacity)` and `.fill_gradient(...)` too, the gradient running across
the whole text. To color a group or a text item by item instead, use
`.set_color_by_gradient([c1, c2, ...])`: each shape or glyph, first to last, gets the next
color along the stops, keeping its fill and outline opacities.

```python
title = s.add(text("fastanim").fill_gradient([BLUE, GREEN]))
dots = s.add(group(dot(LEFT), dot(ORIGIN), dot(RIGHT)).set_color_by_gradient([RED, BLUE]))
```

## Positioning

These methods work on shapes, texts, axes, groups and Lotties alike.

| Method | Effect |
| --- | --- |
| `.shift(v)` | Move by vector `v` |
| `.move_to(p)` | Move the center to `p` |
| `.scale(k)` | Scale about the center |
| `.to_edge(dir)` | Move against a frame edge, e.g. `UP` |
| `.next_to(other, dir)` | Place beside `other` (a shape, text, axes, group or point, not a mobject handle), `DEFAULT_BUFF` apart |
| `.next_to(other, dir, buff)` | The same, `buff` apart |
| `.align_to(other, dir)` | Line up the `dir` edge with `other`'s |
| `.scale_to_fit_height(h)` | Scale about the center to be `h` tall |
| `.scale_to_fit_width(w)` | Scale about the center to be `w` wide |
| `.center()` | The bounding-box center, as a point |
| `.width`, `.height` | The bounding-box size |

```python
box = square(1)
label = text("box").next_to(box, DOWN)
```

To place something beside a mobject already in the scene, read its shape back with
[`s.get`](scene.md#reading-state-back): `text("box").next_to(s.get(m), DOWN)`.

## Images

`image(path)` loads a PNG or JPEG file (relative to the current directory), and `image(rows)`
builds one from pixels: rows top row first, as nested lists or a NumPy array (anything with
`tolist()`), each pixel a grey value or `(r, g, b)` or `(r, g, b, a)`, all from 0 to 255. Like
manim's `ImageMobject`, an image is sized so that one 1080 pixels tall would fill the frame's
height; the `height=` and `width=` keywords, or `scale_to_fit_height`, set it directly.

An image is a shape: a rectangle with the pixels drawn inside, and no outline. It moves,
scales, rotates, fades and groups like any other shape, and `.stroke(color, width)` gives it a
border. `create` fades it in rather than drawing it. `pixelated=True` (or `.pixelated()`) keeps
pixels sharp when scaled up, instead of smoothing them.

```python
import numpy as np

photo = s.add(image("photo.png", height=4))
ramp = np.tile(np.arange(256, dtype=np.uint8), (16, 1))
bar = s.add(image(ramp, pixelated=True).next_to(s.get(photo), DOWN))
s.play(rotate(group(photo, bar), PI / 8))
```

`transform` between two images moves one onto the other, and switches pixels at the end; use
`fade_out` and `fade_in` to cross-fade. Image files can't be loaded in the browser playground
yet, but images built from pixels can.

## Lottie animations

`lottie(path)` loads a [Lottie](https://lottiefiles.com) file: the JSON that After Effects'
Bodymovin plugin, LottieFiles and many design tools export. It's sized like an image (a
composition 1080 pixels tall fills the frame's height, or set `height=` or `width=`) and
positioned like a group: `.scale_to_fit_height`, `.next_to`, `.rotate` and the rest place its
whole frame. `.at_frame(f)` picks the frame it shows before it plays, the first by default, and
`.duration` is its length in seconds.

Added, it is a group with one shape per fill and stroke in the file, which every group
animation (`fade_in`, `shift`, `scale`, `indicate`, ...) works on. `play_lottie(m)` plays it
through once at its own frame rate. Its keywords are `start=` and `end=` frames (playing
backwards if `end` comes first) and `loops=`, and `run_time` speeds it up or slows it down:

```python
anim = s.add(lottie("bounce.json", height=5))
s.play(fade_in(anim))
s.play(play_lottie(anim, loops=2))
s.play(shift(anim, LEFT * 3), play_lottie(anim, end=30).run_time(2))
```

`play_lottie` follows the group, so animations that move, fade or reorder it can play
alongside it. They must come before `play_lottie` in the call, or the Lottie's own frames
overwrite them.

Lottie features drawn: shape, solid, null and precomposition layers, with parenting, time
stretch and time remapping; groups, paths, rectangles, ellipses, stars and polygons; fills,
strokes and trim paths; eased, held and spatial keyframes. Gradients are drawn in the average
of their colours, and rounded star corners are drawn sharp. Masks, mattes, repeaters,
dashes, line caps and joins, image layers and text layers are not drawn yet. Like image files,
Lottie files can't be loaded in the browser playground yet.

## Groups

`group(a, b, ...)` puts shapes, texts, axes and other groups together, like manim's `VGroup`.
A group positions as one, and `.arrange(dir)` (or `.arrange(dir, buff)`) lays its items out in
a row, keeping the group's center.

```python
row = group(square(1), circle(0.5), text("x")).arrange(RIGHT).to_edge(UP)
g = s.add(row)
s.play(fade_in(g))
s.play(rotate(g, PI))   # turns about the group's center
s.play(indicate(g[1]))  # members by index; g[-1] is the last
```

`s.add(group)` returns a group mobject with one member per item, in order. Every
animation that takes a mobject takes a group too, except those that need one shape
(`transform`, `update`, `apply_function`, `s.always`, `s.get`) or one text
(`transform_diff`). `len(g)` is the number of members.

`group(a, b)` of mobjects already added makes a group mobject from them, to play one
animation on all of them:

```python
s.play(fade_out(group(title, g)))
```

## Axes

`axes(x_range, y_range)` makes manim-style axes, 12 by 6 units. Ranges are `(min, max)` or
`(min, max, step)`; `axes(x_range, y_range, width, height)` sets the size.

`fit_axes(f, x_range)` (or `fit_axes(f, x_range, width, height)`) picks the y range for you:
it samples `f` across `x_range` and rounds out to about 8 ticks of 1, 2 or 5 × 10ⁿ. manim has
no equivalent.

| Method | Returns |
| --- | --- |
| `ax.c2p(x, y)` | Graph coordinates to a scene point |
| `ax.p2c(p)` | Scene point to graph coordinates |
| `ax.plot(f)` | Shape: graph of `f` across the whole x range |
| `ax.plot(f, x0, x1)` | Shape: graph of `f` in graph coordinates |
| `ax.plot(f, x0, x1, segments)` | The same in `segments` cubics instead of 64, for wiggly graphs |
| `ax.vertical_line(p)` | Shape: line from the x-axis up to `p` |
| `ax.numbers()` | Text: tick numbers |
| `ax.labels(x, y)` | Text: axis labels, as Typst math |
| `ax.shape()` | Shape: the axes themselves, with arrow tips (filled: recolor with `.stroke(c, w).fill(c)`) |

`s.add(ax)` adds the axes as one shape.

```python
from math import sin

ax = axes((-3, 3), (-1.5, 1.5, 0.5))
s.play(create(s.add(ax)))
s.add(ax.numbers())
graph = s.add(ax.plot(sin, -3, 3).stroke(YELLOW, 0.05))
s.play(create(graph))
```
