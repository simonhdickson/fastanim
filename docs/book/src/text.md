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

```python
a = math_tex("a = sqrt(c^2 - b^2)")
s.add(a)
s.add(latex(r"\frac{a}{b} + \sqrt{c}").next_to(a, DOWN))
s.add(list([3, 1, 2]).to_edge(DOWN))
```

`from fastanim import *` replaces Python's `list` with this one; `builtins.list` is still
Python's. Raw strings (`r"..."`) keep LaTeX's backslashes, and
triple-quoted strings suit `code`:

```python
SRC = """fn main() {
    println!("hi");
}"""

c = s.add(code(SRC, "rust").scale(0.8))
```

## Fonts

`text`, `math_tex`, `latex` and `code` take a `font=` keyword, a font family name: one of the
bundled fonts (Libertinus Serif, New Computer Modern, New Computer Modern Math, DejaVu Sans
Mono), or one loaded from a file with `load_font(path)`. `load_font` takes a TTF, OTF, TTC or OTC
file, relative to the current directory, and returns the families it holds; loading the same
file again does nothing.

```python
families = load_font("fonts/Inter-Regular.ttf")   # ["Inter"]
title = s.add(text("Hello", font="Inter").to_edge(UP))
s.add(code("let x = 1;", "rust", font="DejaVu Sans Mono"))
```

An unknown family is an error rather than a silent fallback. Math in another font falls back
to New Computer Modern Math for the symbols that font lacks. Fonts can't be loaded in the
browser playground yet.

## Methods

Texts take the [positioning methods](shapes.md#positioning), plus:

| Method | Effect |
| --- | --- |
| `.fill(color)` | Color every glyph |
| `.rotate(angle)` | Rotate about the center |
| `.glyphs` | A list of the glyph shapes |

## Animating text

Most animations apply to every glyph at once. `write` and `unwrite` draw glyph by glyph;
`move_to`, `scale` and `rotate` move the text as a whole.

A text cannot be read back with `s.get`, used with `s.always`, or passed to `transform`
or `replacement_transform`; those need shapes. Use [`transform_diff`](diffs.md) to change a
text into another.
