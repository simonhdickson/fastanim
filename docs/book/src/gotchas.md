# Gotchas

## Write floats as floats

API functions take floats, and Rhai does not convert integers. `point(1, 0)` fails with
`Function not found: point (i64, i64)`. Write `point(1.0, 0.0)`, and convert loop indices with
`.to_float()`:

```rhai
for i in 0..5 {
    scene.add(dot(point(i.to_float(), 0.0)));
}
```

Axes ranges are the exception: `axes([-3, 3], [-1, 1])` accepts integers.

## Add before animating

Animations take the handle `scene.add` returns, not the shape. Animating something that was
never added, or was removed, fails with `not in the scene`.

## Shapes vs. texts

`scene.get`, `scene.always`, `transform` and `replacement_transform` need shapes. Texts are many
glyphs; change them with `transform_diff`.

## Closures run later

Closures passed to `always`, `update`, `apply_function`, `function_graph` and `plot` are called
during baking, not where they are written. Errors in them are reported at the next
`scene.play`. They cannot use `scene`; see [updaters](updaters.md).

## The standard library

Rhai's standard library is available: `sin`, `cos`, `min`, `max`, `abs`, `sqrt`, `to_float`,
`len`, `map`, `filter`, `print` and so on. See the [Rhai book](https://rhai.rs/book/).

## Limits

A script that runs too long or recurses too deep fails rather than hanging: it gets 10 million
operations and 64 call levels.
