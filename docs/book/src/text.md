# Text, math and code

Text is typeset with [Typst](https://typst.app), using bundled fonts; no LaTeX install is needed.
Each text is laid out into per-glyph shapes, split into tokens that
[diff transforms](diffs.md) can match up.

| Function | Typesets |
| --- | --- |
| `text(s)` | Plain text |
| `math_tex(s)` | Math, in [Typst math syntax](https://typst.app/docs/reference/math/) |
| `latex(s)` | Math, in LaTeX syntax (converted to Typst by [MiTeX](https://github.com/mitex-rs/mitex)) |
| `code(src, lang)` | Syntax-highlighted source code, e.g. `code(src, "rust")` |
| `list([v1, v2, ...])` | A row of boxed values, centered on the origin |

```rhai
let a = math_tex("a = sqrt(c^2 - b^2)");
scene.add(a);
scene.add(latex(`\frac{a}{b} + \sqrt{c}`).next_to(a, DOWN));
scene.add(list([3, 1, 2]).to_edge(DOWN));
```

Rhai backtick strings span lines, which suits `code`:

```rhai
const SRC = `fn main() {
    println!("hi");
}`;
let c = scene.add(code(SRC, "rust").scale(0.8));
```

## Methods

Texts take the [positioning methods](shapes.md#positioning), plus:

| Method | Effect |
| --- | --- |
| `.fill(color)` | Color every glyph |
| `.rotate(angle)` | Rotate about the center |
| `.glyphs` | An array of the glyph shapes |

## Animating text

Most animations apply to every glyph at once. `write` and `unwrite` draw glyph by glyph;
`move_to`, `scale` and `rotate` move the text as a whole.

A text cannot be read back with `scene.get`, used with `scene.always`, or passed to `transform`
or `replacement_transform`; those need shapes. Use [`transform_diff`](diffs.md) to change a
text into another.
