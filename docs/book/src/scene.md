# The scene

Every script has a global `scene`. Calls on it append to the timeline in order.

## Adding and removing

`scene.add(x)` puts a shape, text or axes into the scene and returns a **mobject** handle. The
handle is what animations take.

```rhai
let c = scene.add(circle(1.0));     // shown immediately
scene.play(fade_out(c));
scene.remove(c);                     // gone from the scene
```

Adding a shape shows it at once. To bring it in with an animation, add it and play the
animation straight away: `create`, `write` and `fade_in` start from nothing.

```rhai
let c = scene.add(circle(1.0));
scene.play(create(c));
```

Values like `circle(1.0)` are plain descriptions. Changing one after `add` does not change the
scene; animate the mobject instead.

## Time

| Call | Effect |
| --- | --- |
| `scene.play(anim)` | Plays an animation, advancing time by its run time |
| `scene.play([a, b, ...])` | Plays several animations together |
| `scene.wait()` | Waits 1 second |
| `scene.wait(secs)` | Waits `secs` seconds |
| `scene.time` | The current time, in seconds |

## Markers

`scene.marker("name")` names the current time. The preview jumps between markers with `[` and
`]`, and `fastanim render --section name` renders from that marker to the next.

```rhai
scene.marker("intro");
// ...
scene.marker("proof");
```

## Reading state back

`scene.get(m)` returns the shape of mobject `m` as it is now, at the current time. Only shapes
can be read back; texts are many glyphs.

```rhai
let c = scene.add(circle(1.0).shift(LEFT));
scene.play(shift(c, RIGHT * 2.0));
let now = scene.get(c);    // the circle, one unit right of center
```

## Coordinates

Coordinates follow manim. The origin is the center of the frame, x points right and y points
up. The frame is `FRAME_WIDTH` (about 14.2) by `FRAME_HEIGHT` (8) units, 16:9.

The direction constants are unit points: `UP`, `DOWN`, `LEFT`, `RIGHT` and `ORIGIN`.
