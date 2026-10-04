//! SPEC §10 examples 1–5: diff-driven transforms of equations, code and lists, and a shape
//! morph.
//!
//! `cargo run -p ranim-bevy --example diff` previews it (`[`/`]` jump between the parts);
//! `cargo run -p ranim-bevy --example diff -- render --section commutativity -o comm.mp4`
//! exports one part.

use ranim_core::color::{BLUE, YELLOW};
use ranim_core::*;
use ranim_text::{code, list, math_tex, text, transform_diff, transform_list};

const BEFORE: &str = r#"fn main() {
    let scores = vec![3, 9, 4, 7];
    let mut total = 0;
    for s in &scores {
        total += s;
    }
    let mean = total as f64 / scores.len() as f64;
    println!("mean = {mean}");
}"#;

const AFTER: &str = r#"fn mean(scores: &[i32]) -> f64 {
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
}"#;

/// Records the scene; public so `ranim-script` can check its script version matches.
pub fn construct(s: &mut Scene) {
    // 1. Pythagoras rearrangement.
    s.marker("pythagoras");
    let title = text("Solving for a").to_edge(UP).add_to(s);
    s.play(write(&title));
    let mut eq = math_tex("a^2 + b^2 = c^2").scale(1.5).add_to(s);
    s.play(write(&eq).run_time(1.5));
    s.wait(0.5);
    let d = transform_diff(s, &mut eq, &math_tex("a^2 = c^2 - b^2").scale(1.5));
    s.play(d.run_time(1.5));
    s.wait(0.5);
    let d = transform_diff(s, &mut eq, &math_tex("a = sqrt(c^2 - b^2)").scale(1.5));
    let style = DiffStyle {
        highlight_changes: true,
        ..DiffStyle::default()
    };
    s.play(d.style(style).run_time(1.5));
    let c = circumscribe(s, &eq);
    s.play(c);
    s.wait(1.0);
    fade_all(s, title.iter().chain(eq.iter()));

    // 2. Commutativity: two pairs of arcing moves, `+` and `=` fixed.
    s.marker("commutativity");
    let mut eq = math_tex("a + b = b + a").scale(2.0).add_to(s);
    s.play(write(&eq));
    let d = transform_diff(s, &mut eq, &math_tex("b + a = a + b").scale(2.0));
    s.play(d.run_time(1.5));
    s.wait(1.0);
    fade_all(s, eq.iter());

    // 5. Shape morph: square → circle → "8" (two holes).
    s.marker("morph");
    let shape = s.add(VState::square(3.0).stroke(BLUE, 0.06));
    s.play(create(shape));
    s.play(transform(shape, VState::circle(1.5).stroke(BLUE, 0.06)));
    let eight = math_tex("8").scale(8.0).glyphs.remove(0);
    s.play(transform(
        shape,
        eight.fill(YELLOW.with_alpha(0.5)).stroke(YELLOW, 0.04),
    ));
    s.wait(1.0);
    fade_all(s, [shape].iter());

    // 3. Code refactor: extracting a function.
    s.marker("code");
    let mut src = code(BEFORE, "rust").scale(0.7).add_to(s);
    s.play(write(&src).run_time(2.0));
    s.wait(0.5);
    let d = transform_diff(s, &mut src, &code(AFTER, "rust").scale(0.7));
    s.play(d.run_time(2.5));
    s.wait(1.0);
    fade_all(s, src.iter());

    // 4. Bubble sort: each swap is two moves.
    s.marker("bubble-sort");
    let mut v = vec![5, 1, 4, 2, 8, 3];
    let mut cells = list(&v).scale(1.2).add_to(s);
    s.play(write(&cells));
    for i in 0..v.len() {
        for j in 0..v.len() - 1 - i {
            if v[j] > v[j + 1] {
                v.swap(j, j + 1);
                let d = transform_list(s, &mut cells, &list(&v).scale(1.2));
                s.play(d.run_time(0.8));
            }
        }
    }
    s.wait(1.0);
}

/// Fades out and removes `ids`.
fn fade_all<'a>(s: &mut Scene, ids: impl Iterator<Item = &'a MobjectId>) {
    let ids: Vec<MobjectId> = ids.copied().collect();
    s.play(Parallel(
        ids.iter()
            .map(|&id| Box::new(fade_out(id)) as Box<dyn Animation>)
            .collect(),
    ));
    ids.iter().for_each(|&id| s.remove(id));
}

fn main() {
    ranim_bevy::run(construct);
}
