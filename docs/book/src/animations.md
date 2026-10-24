# Animations

An animation takes a mobject handle (from `s.add`) and is run with `s.play`. Most last
1 second and ease smoothly.

```python
c = s.add(circle(1))
s.play(create(c))
s.play(shift(c, RIGHT * 2), run_time=2, rate=THERE_AND_BACK)
```

## Appearing and disappearing

| Animation | Effect |
| --- | --- |
| `create(m)` / `uncreate(m)` | Draw / undraw the outline |
| `write(m)` / `unwrite(m)` | Write / unwrite, glyph by glyph for text |
| `fade_in(m)` / `fade_out(m)` | Fade |
| `fade_in(m, shift=v)` / `fade_out(m, shift=v)` | Fade while moving by `v`, e.g. `fade_in(m, shift=RIGHT)` enters from the left |
| `fade_in(m, scale=k)` / `fade_out(m, scale=k)` | Fade while scaling from / to `k` times the size |
| `fade_in(m, shift=v, scale=k)` | Both |
| `grow_from_center(m)` / `shrink_to_center(m)` | Grow from / shrink to a point |
| `spin_in(m)` | Spin and grow in |
| `draw_border_then_fill(m)` | Draw the outline, then fill |

Disappearing animations leave the mobject in the scene, invisible. Call `s.remove(m)` once
it is gone.

## Moving

| Animation | Effect |
| --- | --- |
| `shift(m, v)` | Move by `v` |
| `move_to(m, p)` | Move the center to `p` |
| `scale(m, k)` | Scale by `k` about the center |
| `rotate(m, angle)` | Rotate by `angle` radians about the center |

`m.animate` chains the positioning methods and `.fill` and `.stroke` into one animation, as in
manim:

```python
s.play(eq.animate.shift(UP).scale(0.8))
s.play(dots.animate.next_to(s.get(box), DOWN).fill(RED))
```

## Changing shape

| Animation | Effect |
| --- | --- |
| `transform(m, shape)` | Morph shape `m` into `shape` |
| `replacement_transform(m, other)` | Morph shape `m` into mobject `other`, which takes its place |
| `apply_function(m, f)` | Move every point `p` of `m` to `f(p)` |
| `update(m, f)` | Custom: `f(start, alpha)` returns `m`'s shape at progress `alpha` (0–1) |

```python
sq = s.add(square(2))
s.play(transform(sq, circle(1).fill(RED)))
s.play(apply_function(sq, lambda p: point(p.x, p.y + p.x * 0.3)))
s.play(update(sq, lambda start, a: start.rotate(a * PI)))
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

`run_time=` on `s.play` sets the step's length, and `rate=` its easing. An animation's own
`.run_time(secs)` and `.rate(r)` set them for that animation alone, inside a composition.

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
| `s.play(a, b)` | Together |
| `parallel(a, b)` | Together, as one animation |
| `sequence(a, b)` | One after another |
| `lagged_start(a, b, ..., lag_ratio=r)` | Staggered: each starts `r` of the way through the one before |

These return animations, so they nest and take `.run_time` and `.rate`.

```python
dots = [s.add(dot((i - 2, 0))) for i in range(5)]
s.play(lagged_start(*(grow_from_center(d) for d in dots), lag_ratio=0.2))
s.play(sequence(indicate(dots[0]), indicate(dots[4])))
```

The preview's timeline labels each block with the animation's kind (`FadeIn`,
`Write + Create`). Rhai scripts can name them with `.named(title)`; Python can't yet.
