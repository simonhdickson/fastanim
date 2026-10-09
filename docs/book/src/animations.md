# Animations

An animation takes a mobject handle (from `scene.add`) and is run with `scene.play`. Most last
1 second and ease smoothly.

```rhai
let c = scene.add(circle(1.0));
scene.play(create(c));
scene.play(shift(c, RIGHT * 2.0).run_time(2.0).rate(THERE_AND_BACK));
```

## Appearing and disappearing

| Animation | Effect |
| --- | --- |
| `create(m)` / `uncreate(m)` | Draw / undraw the outline |
| `write(m)` / `unwrite(m)` | Write / unwrite, glyph by glyph for text |
| `fade_in(m)` / `fade_out(m)` | Fade |
| `grow_from_center(m)` / `shrink_to_center(m)` | Grow from / shrink to a point |
| `spin_in(m)` | Spin and grow in |
| `draw_border_then_fill(m)` | Draw the outline, then fill |

Disappearing animations leave the mobject in the scene, invisible. Call `scene.remove(m)` once
it is gone.

## Moving

| Animation | Effect |
| --- | --- |
| `shift(m, v)` | Move by `v` |
| `move_to(m, p)` | Move the center to `p` |
| `scale(m, k)` | Scale by `k` about the center |
| `rotate(m, angle)` | Rotate by `angle` radians about the center |

## Changing shape

| Animation | Effect |
| --- | --- |
| `transform(m, shape)` | Morph shape `m` into `shape` |
| `replacement_transform(m, other)` | Morph shape `m` into mobject `other`, which takes its place |
| `apply_function(m, f)` | Move every point `p` of `m` to `f(p)` |
| `update(m, f)` | Custom: `f(start, alpha)` returns `m`'s shape at progress `alpha` (0–1) |

```rhai
let s = scene.add(square(2.0));
scene.play(transform(s, circle(1.0).fill(RED)));
scene.play(apply_function(s, |p| point(p.x, p.y + p.x * 0.3)));
scene.play(update(s, |start, a| start.rotate(a * PI)));
```

For texts, see [diff transforms](diffs.md).

## Emphasis

| Animation | Effect |
| --- | --- |
| `indicate(m)` | Briefly enlarge and color |
| `wiggle(m)` | Wiggle in place |
| `circumscribe(m)` | Draw a box around `m`, then remove it |
| `flash(p)` | Lines bursting out from point `p` |

## Timing and easing

`.run_time(secs)` sets an animation's length. `.rate(r)` sets its easing:

| Rate | Shape |
| --- | --- |
| `SMOOTH` | Ease in and out (most animations' default) |
| `LINEAR` | Constant speed |
| `THERE_AND_BACK` | Go to the end and return |
| `ease_in(name)`, `ease_out(name)`, `ease_in_out(name)` | `name` is `"quad"`, `"cubic"`, `"expo"` or `"back"` |
| `spring(stiffness, damping)` | Overshoot and settle |

## Composition

| Function | Plays |
| --- | --- |
| `[a, b]` passed to `scene.play` | Together |
| `parallel([a, b])` | Together, as one animation |
| `sequence([a, b])` | One after another |
| `lagged_start(lag_ratio, [a, b, ...])` | Staggered: each starts `lag_ratio` of the way through the one before |

These return animations, so they nest and take `.run_time` and `.rate`.

```rhai
let dots = [];
for i in 0..5 {
    dots.push(scene.add(dot(point(i.to_float() - 2.0, 0.0))));
}
scene.play(lagged_start(0.2, dots.map(|d| grow_from_center(d))));
scene.play(sequence([indicate(dots[0]), indicate(dots[4])]));
```
