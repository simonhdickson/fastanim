# Updaters and closures

## `s.always`

`s.always(m, f)` makes shape `m` follow a function from then on. `f(state, t)` is called every
frame and returns `m`'s whole shape at time `t`, in seconds since the `always` call. Without
`f`, `s.always(m)` is a decorator, for updaters longer than a `lambda`.

```python
from math import cos, sin


# A dot orbits the origin once every four seconds.
def at(t: float) -> Point:
    return point(cos(TAU * t / 4), sin(TAU * t / 4)) * 2


d = s.add(dot(at(0)).fill(YELLOW))
s.always(d, lambda st, t: dot(at(t)).fill(YELLOW))
s.wait(4)
```

Index `state` with a mobject to read another shape's current state, so one shape can follow
another:

```python
c = s.add(circle(0.5))
label = s.add(dot(ORIGIN))


@s.always(label)
def follow(st: State, t: float) -> Shape:
    return dot(ORIGIN).next_to(st[c], UP)


s.play(shift(c, RIGHT * 3))
```

## Pure in `t`

Every frame must be a function of `t` and the scene state alone. The timeline can be scrubbed,
rendered from a marker or sampled out of order, so a closure cannot carry state from one frame
to the next.

Manim updaters often accumulate (`dot.shift(dt * v)` every frame). Rewrite them in terms of `t`:

```python
# Instead of moving the dot a little each frame, say where it is at time t.
s.always(d, lambda st, t: dot((-4 + t, 0)))
```

`sine_curve.py` in `fastanim-py/scenes` traces manim's SineCurveUnitCircle this way: the dot,
the lines to it and the traced curve are all functions of `t`.

## Closures

Updaters, `apply_function`, `update`, `function_graph` and `plot` keep their Python functions in
the baked timeline and call them while baking and playing. They can use anything in scope,
so helpers like `at` above can be shared between updaters. Plain helper functions that take the
scene work as in any Python code:

```python
def fade_all(s: Scene, *mobs: Mobject) -> None:
    s.play(*(fade_out(m) for m in mobs))
    for m in mobs:
        s.remove(m)
```

The scene itself cannot be used inside a function that runs during playback (updaters,
`apply_function`, `update`, graph functions); doing so raises `SceneError`. Use `state`
instead.
