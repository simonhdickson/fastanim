# Updaters and closures

## `scene.always`

`scene.always(m, |state, t| ...)` makes shape `m` follow a function from then on. The closure is
called every frame and returns `m`'s whole shape at time `t`, in seconds since the `always` call.

```rhai
// A dot orbits the origin once every four seconds.
let at = |t| point((TAU * t / 4.0).cos(), (TAU * t / 4.0).sin()) * 2.0;
let d = scene.add(dot(at.call(0.0)).fill(YELLOW));
scene.always(d, |s, t| dot(at.call(t)).fill(YELLOW));
scene.wait(4.0);
```

Index `state` with a mobject to read another shape's current state, so one shape can follow
another:

```rhai
let c = scene.add(circle(0.5));
let label = scene.add(dot(ORIGIN));
scene.always(label, |s, t| dot(ORIGIN).next_to(s[c], UP));
scene.play(shift(c, RIGHT * 3.0));
```

## Pure in `t`

Every frame must be a function of `t` and the scene state alone. The timeline can be scrubbed,
rendered from a marker or sampled out of order, so a closure cannot carry state from one frame
to the next.

Manim updaters often accumulate (`dot.shift(dt * v)` every frame). Rewrite them in terms of `t`:

```rhai
// Instead of moving the dot a little each frame, say where it is at time t.
scene.always(d, |s, t| dot(point(-4.0 + t, 0.0)));
```

## Closures

Closures are values. Call them with `.call`:

```rhai
let f = |x| x * 2.0;
print(f.call(1.5));   // 3.0
```

They capture variables from where they are written, so helpers like `at` above can be shared
between updaters. Functions declared with `fn` cannot see outer variables; pass `scene` and
anything else in as arguments.

```rhai
fn fade_all(scene, mobs) {
    scene.play(mobs.map(|m| fade_out(m)));
    for m in mobs {
        scene.remove(m);
    }
}
```

`scene` itself cannot be used inside a closure that runs during playback (updaters,
`apply_function`, `update`, graph functions). Use `state` instead.
