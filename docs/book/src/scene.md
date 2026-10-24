# The scene

A scene function gets the scene as its argument, called `s` in this book. Calls on it append to
the timeline in order.

## Adding and removing

`s.add(x)` puts a shape, text, axes, group or Lottie into the scene and returns a **mobject**
handle. The handle is what animations take.

```python
c = s.add(circle(1))   # shown immediately
s.play(fade_out(c))
s.remove(c)            # gone from the scene
```

Adding a shape shows it at once. To bring it in with an animation, add it and play the
animation straight away: `create`, `write` and `fade_in` start from nothing.

```python
c = s.add(circle(1))
s.play(create(c))
```

Values like `circle(1)` are plain descriptions. Changing one after `add` does not change the
scene; animate the mobject instead.

## Background

`s.background(color)` sets the color behind every frame; it is black until set. It is one
color for the whole scene, so the last call wins wherever it is in the function. The preview,
video and still exports, SVG and the browser player all use it.

```python
s.background("#1c1c28")
```

## Time

| Call | Effect |
| --- | --- |
| `s.play(anim)` | Plays an animation, advancing time by its run time |
| `s.play(a, b, ...)` | Plays several animations together |
| `s.play(..., run_time=secs, rate=r)` | The same, with its length or [easing](animations.md#timing-and-easing) set |
| `s.wait()` | Waits 1 second |
| `s.wait(secs)` | Waits `secs` seconds |
| `s.time` | The current time, in seconds |
| `s.add_sound(a)` | Starts a sound without waiting; see [Sound](audio.md) |
| `s.wait_for_sounds()` | Waits until every sound started so far has finished |

## Markers

`s.marker("name")` names the current time. The preview jumps between markers with `[` and
`]`, and `render --section name` renders from that marker to the next.

```python
s.marker("intro")
# ...
s.marker("proof")
```

## Reading state back

`s.get(m)` returns the shape of mobject `m` as it is now, at the current time. Only shapes
can be read back; texts are many glyphs.

```python
c = s.add(circle(1).shift(LEFT))
s.play(shift(c, RIGHT * 2))
now = s.get(c)   # the circle, one unit right of center
```

## Coordinates

Coordinates follow manim. The origin is the center of the frame, x points right and y points
up. The frame is `FRAME_WIDTH` (about 14.2) by `FRAME_HEIGHT` (8) units, 16:9.

The direction constants are unit points: `UP`, `DOWN`, `LEFT`, `RIGHT` and `ORIGIN`.
