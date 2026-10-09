# Shapes, points and colors

## Points

`point(x, y)` makes a point, which also serves as a vector. Points add, subtract, negate, and
multiply or divide by a number. Read the parts with `.x` and `.y`.

```rhai
let p = point(1.0, 2.0) + RIGHT * 3.0;   // point(4, 2)
let mid = (p + ORIGIN) / 2.0;
print(mid.y);                             // 1.0
```

## Colors

| Function | Example |
| --- | --- |
| Named constants | `WHITE`, `BLACK`, `BLUE`, `RED`, `GREEN`, `YELLOW`, `GREY`, `ORANGE`, `TRANSPARENT` |
| `rgb(r, g, b)` | `rgb(1.0, 0.5, 0.0)`, each 0–1 |
| `hex(rgb)` | `hex(0xf4d345)` |
| `with_alpha(c, a)` | `BLUE.with_alpha(0.5)` |

## Shapes

All shapes are centered on the origin unless they take points.

| Function | Shape |
| --- | --- |
| `circle(radius)` | Circle |
| `square(side)` | Square |
| `rectangle(width, height)` | Rectangle |
| `arc(radius, start, sweep)` | Arc, `sweep` radians counter-clockwise from `start` |
| `line(a, b)` | Line from point `a` to point `b` |
| `dot(p)` | Small filled white dot at `p` |
| `polygon([p1, p2, ...])` | Closed polygon through the points |
| `function_graph(f, x0, x1, segments)` | Graph of `y = f(x)` for `x` in `x0..x1`, in scene units |

```rhai
let wave = function_graph(|x| x.sin(), -PI, PI, 32);
```

## Styling

Style methods return a new shape, so they chain.

| Method | Effect |
| --- | --- |
| `.fill(color)` | Fill color |
| `.stroke(color, width)` | Outline color and width |
| `.z_index(n)` | Draw order; higher draws on top |
| `.rotate(angle)` | Rotate by `angle` radians |

```rhai
let s = scene.add(square(2.0).fill(BLUE.with_alpha(0.5)).stroke(WHITE, 0.04).rotate(PI / 4.0));
```

## Positioning

These methods work on shapes, texts and axes alike.

| Method | Effect |
| --- | --- |
| `.shift(v)` | Move by vector `v` |
| `.move_to(p)` | Move the center to `p` |
| `.scale(k)` | Scale about the center |
| `.to_edge(dir)` | Move against a frame edge, e.g. `UP` |
| `.next_to(other, dir)` | Place beside `other` (a shape, text or point, not a mobject handle), `DEFAULT_BUFF` apart |
| `.next_to(other, dir, buff)` | The same, `buff` apart |
| `.align_to(other, dir)` | Line up the `dir` edge with `other`'s |

```rhai
let box = square(1.0);
let label = text("box").next_to(box, DOWN);
```

## Axes

`axes(x_range, y_range)` makes manim-style axes, 12 by 6 units. Ranges are `[min, max]` or
`[min, max, step]`; `axes(x_range, y_range, width, height)` sets the size.

| Method | Returns |
| --- | --- |
| `ax.c2p(x, y)` | Graph coordinates to a scene point |
| `ax.p2c(p)` | Scene point to graph coordinates |
| `ax.plot(f, x0, x1)` | Shape: graph of `f` in graph coordinates |
| `ax.vertical_line(p)` | Shape: line from the x-axis up to `p` |
| `ax.numbers()` | Text: tick numbers |
| `ax.labels(x, y)` | Text: axis labels, as Typst math |
| `ax.shape()` | Shape: the axes themselves |

`scene.add(ax)` adds the axes as one shape.

```rhai
let ax = axes([-3, 3], [-1.5, 1.5, 0.5]);
scene.play(create(scene.add(ax)));
scene.add(ax.numbers());
let graph = scene.add(ax.plot(|x| x.sin(), -3.0, 3.0).stroke(YELLOW, 0.05));
scene.play(create(graph));
```
