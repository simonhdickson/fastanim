//! The epicycles in plain Rust against `fastanim-core`, as a baseline for the Python and Rhai
//! scenes: the same coefficients, shapes and an updater per shape, with no scripting language.
//! `fourier.py` ported as it is, `tips` worked out once per frame per thread.
//!
//!     N=100 cargo run --release -p fastanim-bevy --example epicycles -- render -o epicycles.mp4

use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;
use std::sync::Arc;

use fastanim_core::color::{BLUE, WHITE, YELLOW};
use fastanim_core::{Point, Scene, VState, Vec2};

const STROKE: f64 = 0.01;
/// Points in the finished trace.
const TRACE: usize = 1024;

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.into())
}

/// Complex product, with `Vec2` as a complex number.
fn mul(a: Vec2, b: Vec2) -> Vec2 {
    Vec2::new(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x)
}

fn cis(a: f64) -> Vec2 {
    Vec2::new(a.cos(), a.sin())
}

/// The corners of a star, alternating outer and inner.
fn star(points: usize, outer: f64, inner: f64) -> Vec<Vec2> {
    (0..2 * points)
        .map(|i| {
            cis(PI / 2.0 + PI * i as f64 / points as f64) * if i % 2 == 0 { outer } else { inner }
        })
        .collect()
}

/// (frequency, coefficient) for frequencies 0, 1, -1, 2, -2, ..., `n` of them; see `fourier.py`.
fn coefficients(corners: &[Vec2], n: usize) -> Vec<(f64, Vec2)> {
    let m = corners.len();
    let edges: Vec<f64> = (0..m)
        .map(|j| (corners[(j + 1) % m] - corners[j]).hypot())
        .collect();
    let total: f64 = edges.iter().sum();
    let at: Vec<f64> = (0..=m)
        .map(|j| edges[..j].iter().sum::<f64>() / total)
        .collect();
    let v: Vec<Vec2> = (0..m)
        .map(|j| (corners[(j + 1) % m] - corners[j]) / (at[j + 1] - at[j]))
        .collect();
    let c = |f: i64| {
        if f == 0 {
            return (0..m)
                .map(|j| (corners[j] + corners[(j + 1) % m]) / 2.0 * (at[j + 1] - at[j]))
                .fold(Vec2::ZERO, |a, b| a + b);
        }
        let w = 2.0 * PI * f as f64;
        -(0..m)
            .map(|j| mul(v[j] - v[(j + m - 1) % m], cis(-w * at[j])))
            .fold(Vec2::ZERO, |a, b| a + b)
            / (w * w)
    };
    let freqs = std::iter::once(0)
        .chain((1..).flat_map(|k| [k, -k]))
        .take(n);
    freqs.map(|f| (f as f64, c(f))).collect()
}

/// Where each vector ends at time `t`; vector `k` starts where vector `k - 1` ends.
fn tips(coeffs: &[(f64, Vec2)], duration: f64, t: f64) -> Vec<Vec2> {
    let a = 2.0 * PI * (t / duration).min(1.0);
    let mut z = Vec2::ZERO;
    coeffs
        .iter()
        .map(|&(f, c)| {
            z += mul(c, cis(f * a));
            z
        })
        .collect()
}

fn p(z: Vec2) -> Point {
    z.to_point()
}

/// Records the scene, `N` vectors drawing for `DURATION` seconds.
pub fn epicycles(s: &mut Scene) {
    let n: usize = env("N", "5000").parse().expect("N");
    let duration: f64 = env("DURATION", "10").parse().expect("DURATION");
    let coeffs = Arc::new(coefficients(&star(5, 3.0, 1.2), n));
    let curve: Arc<Vec<Vec2>> = Arc::new(
        (0..=TRACE)
            .map(|k| {
                *tips(&coeffs, duration, duration * k as f64 / TRACE as f64)
                    .last()
                    .unwrap()
            })
            .collect(),
    );

    // Every updater asks for the tips at its frame's `t`; work them out once per frame on each
    // export thread.
    thread_local! {
        static AT: RefCell<(f32, Rc<Vec<Vec2>>)> = RefCell::new((f32::NAN, Rc::default()));
    }
    let at = {
        let coeffs = coeffs.clone();
        move |t: f32| {
            AT.with_borrow_mut(|(last, tips_at)| {
                if *last != t {
                    *tips_at = Rc::new(tips(&coeffs, duration, t as f64));
                    *last = t;
                }
                tips_at.clone()
            })
        }
    };

    for (k, &(_, c)) in coeffs.iter().enumerate() {
        let start = move |tips: &[Vec2]| if k > 0 { tips[k - 1] } else { Vec2::ZERO };
        let r = c.hypot();
        let ring = s.add(VState::circle(r));
        let at_ = at.clone();
        s.always(ring, move |_, t| {
            VState::circle(r)
                .move_to(p(start(&at_(t))))
                .stroke(BLUE, STROKE)
                .stroke_opacity(0.5)
        });
        let v = s.add(VState::line(Point::ZERO, Point::ZERO));
        let at_ = at.clone();
        s.always(v, move |_, t| {
            let tips = at_(t);
            VState::line(p(start(&tips)), p(tips[k])).stroke(WHITE, STROKE)
        });
    }

    let trace = s.add(VState::line(Point::ZERO, Point::ZERO));
    s.always(trace, move |_, t| {
        let done = &curve[..(TRACE as f64 * (t as f64 / duration).min(1.0)) as usize + 1];
        let pts: Vec<Point> = done
            .iter()
            .chain([at(t).last().unwrap()])
            .map(|&z| p(z))
            .collect();
        VState::polyline(&pts).stroke(YELLOW, 2.0 * STROKE)
    });

    s.wait(duration as f32);
}

fn main() {
    fastanim_bevy::run(epicycles);
}
