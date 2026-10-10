# SPEC §10 examples 1-5: diff-driven transforms of equations, code and lists, and a shape
# morph. The Python version of fastanim-bevy/examples/diff.rs and
# fastanim-script/scenes/diff.rhai.
from fastanim import *

BEFORE = """fn main() {
    let scores = vec![3, 9, 4, 7];
    let mut total = 0;
    for s in &scores {
        total += s;
    }
    let mean = total as f64 / scores.len() as f64;
    println!("mean = {mean}");
}"""

AFTER = """fn mean(scores: &[i32]) -> f64 {
    let mut total = 0;
    for s in scores {
        total += s;
    }
    total as f64 / scores.len() as f64
}

fn main() {
    let scores = vec![3, 9, 4, 7];
    let mean = mean(&scores);
    println!("mean = {mean}");
}"""


def fade_all(s: Scene, *mobs: Mobject) -> None:
    """Fades out and removes `mobs`."""
    s.play(*(fade_out(m) for m in mobs))
    for m in mobs:
        s.remove(m)


@scene
def diff(s: Scene) -> None:
    # 1. Pythagoras rearrangement.
    s.marker("pythagoras")
    title = s.add(text("Solving for a").to_edge(UP))
    s.play(write(title))
    eq = s.add(math_tex("a^2 + b^2 = c^2").scale(1.5))
    s.play(write(eq), run_time=1.5)
    s.wait(0.5)
    s.play(transform_diff(eq, math_tex("a^2 = c^2 - b^2").scale(1.5)), run_time=1.5)
    s.wait(0.5)
    s.play(
        transform_diff(eq, math_tex("a = sqrt(c^2 - b^2)").scale(1.5), highlight_changes=True),
        run_time=1.5,
    )
    s.play(circumscribe(eq))
    s.wait(1)
    fade_all(s, title, eq)

    # 2. Commutativity: two pairs of arcing moves, `+` and `=` fixed.
    s.marker("commutativity")
    eq = s.add(math_tex("a + b = b + a").scale(2))
    s.play(write(eq))
    s.play(transform_diff(eq, math_tex("b + a = a + b").scale(2)), run_time=1.5)
    s.wait(1)
    fade_all(s, eq)

    # 5. Shape morph: square, circle, then "8" (two holes).
    s.marker("morph")
    shape = s.add(square(3, stroke=BLUE, stroke_width=0.06))
    s.play(create(shape))
    s.play(transform(shape, circle(1.5, stroke=BLUE, stroke_width=0.06)))
    eight = math_tex("8").scale(8).glyphs[0]
    s.play(transform(shape, eight.fill(YELLOW.with_alpha(0.5)).stroke(YELLOW, 0.04)))
    s.wait(1)
    fade_all(s, shape)

    # 3. Code refactor: extracting a function.
    s.marker("code")
    src = s.add(code(BEFORE, "rust").scale(0.7))
    s.play(write(src), run_time=2)
    s.wait(0.5)
    s.play(transform_diff(src, code(AFTER, "rust").scale(0.7)), run_time=2.5)
    s.wait(1)
    fade_all(s, src)

    # 4. Bubble sort: each swap is two moves.
    s.marker("bubble-sort")
    v = [5, 1, 4, 2, 8, 3]
    cells = s.add(list(v).scale(1.2))
    s.play(write(cells))
    for i in range(len(v)):
        for j in range(len(v) - 1 - i):
            if v[j] > v[j + 1]:
                v[j], v[j + 1] = v[j + 1], v[j]
                s.play(transform_list(cells, list(v).scale(1.2)), run_time=0.8)
    s.wait(1)
