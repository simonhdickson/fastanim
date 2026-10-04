// SPEC §10 examples 1–5: diff-driven transforms of equations, code and lists, and a shape
// morph. The script version of crates/ranim-bevy/examples/diff.rs.
//
// ranim run crates/ranim-script/scenes/diff.rhai

const BEFORE = `fn main() {
    let scores = vec![3, 9, 4, 7];
    let mut total = 0;
    for s in &scores {
        total += s;
    }
    let mean = total as f64 / scores.len() as f64;
    println!("mean = {mean}");
}`;

const AFTER = `fn mean(scores: &[i32]) -> f64 {
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
}`;

/// Fades out and removes `mobs`.
fn fade_all(scene, mobs) {
    scene.play(mobs.map(|m| fade_out(m)));
    for m in mobs {
        scene.remove(m);
    }
}

// 1. Pythagoras rearrangement.
scene.marker("pythagoras");
let title = scene.add(text("Solving for a").to_edge(UP));
scene.play(write(title));
let eq = scene.add(math_tex("a^2 + b^2 = c^2").scale(1.5));
scene.play(write(eq).run_time(1.5));
scene.wait(0.5);
scene.play(transform_diff(eq, math_tex("a^2 = c^2 - b^2").scale(1.5)).run_time(1.5));
scene.wait(0.5);
scene.play(
    transform_diff(eq, math_tex("a = sqrt(c^2 - b^2)").scale(1.5), #{ highlight_changes: true })
        .run_time(1.5),
);
scene.play(circumscribe(eq));
scene.wait(1.0);
fade_all(scene, [title, eq]);

// 2. Commutativity: two pairs of arcing moves, `+` and `=` fixed.
scene.marker("commutativity");
let eq = scene.add(math_tex("a + b = b + a").scale(2.0));
scene.play(write(eq));
scene.play(transform_diff(eq, math_tex("b + a = a + b").scale(2.0)).run_time(1.5));
scene.wait(1.0);
fade_all(scene, [eq]);

// 5. Shape morph: square → circle → "8" (two holes).
scene.marker("morph");
let shape = scene.add(square(3.0).stroke(BLUE, 0.06));
scene.play(create(shape));
scene.play(transform(shape, circle(1.5).stroke(BLUE, 0.06)));
let eight = math_tex("8").scale(8.0).glyphs[0];
scene.play(transform(shape, eight.fill(YELLOW.with_alpha(0.5)).stroke(YELLOW, 0.04)));
scene.wait(1.0);
fade_all(scene, [shape]);

// 3. Code refactor: extracting a function.
scene.marker("code");
let src = scene.add(code(BEFORE, "rust").scale(0.7));
scene.play(write(src).run_time(2.0));
scene.wait(0.5);
scene.play(transform_diff(src, code(AFTER, "rust").scale(0.7)).run_time(2.5));
scene.wait(1.0);
fade_all(scene, [src]);

// 4. Bubble sort: each swap is two moves.
scene.marker("bubble-sort");
let v = [5, 1, 4, 2, 8, 3];
let cells = scene.add(list(v).scale(1.2));
scene.play(write(cells));
for i in 0..v.len() {
    for j in 0..v.len() - 1 - i {
        if v[j] > v[j + 1] {
            let t = v[j];
            v[j] = v[j + 1];
            v[j + 1] = t;
            scene.play(transform_list(cells, list(v).scale(1.2)).run_time(0.8));
        }
    }
}
scene.wait(1.0);
