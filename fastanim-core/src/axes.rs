//! manim's `Axes`: a coordinate system mapped onto the scene, for plotting in graph units.

use std::ops::Range;

use kurbo::{Affine, Point, Rect, Vec2};

use crate::color::WHITE;
use crate::geom::{SubPath, VPath};
use crate::mobject::VState;
use crate::position::Position;

/// Half the length of a tick mark, in scene units (manim's `tick_size`).
const TICK: f64 = 0.1;

/// x and y axes over `[min, max, step]` ranges, placed in the scene by an affine map from
/// graph coordinates. Centered on the origin when made; move it with [`Position`] or
/// [`shift`](Axes::shift), and [`c2p`](Axes::c2p) follows.
///
/// The axes cross at graph `(0, 0)`, or at the nearest end of a range that excludes 0.
// ponytail: no arrow tips; add with `Arrow`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axes {
    /// `[min, max, step]` along x, in graph units.
    pub x_range: [f64; 3],
    /// `[min, max, step]` along y, in graph units.
    pub y_range: [f64; 3],
    /// Graph coordinates to scene coordinates.
    pub to_scene: Affine,
}

impl Axes {
    /// manim's default size: 12 units wide, 6 tall.
    pub fn new(x_range: [f64; 3], y_range: [f64; 3]) -> Self {
        Self::sized(x_range, y_range, 12.0, 6.0)
    }

    /// Axes `x_length` by `y_length` scene units, centered on the origin.
    pub fn sized(x_range: [f64; 3], y_range: [f64; 3], x_length: f64, y_length: f64) -> Self {
        let [x0, x1, _] = x_range;
        let [y0, y1, _] = y_range;
        let to_scene = Affine::scale_non_uniform(x_length / (x1 - x0), y_length / (y1 - y0))
            * Affine::translate((-(x0 + x1) / 2.0, -(y0 + y1) / 2.0));
        Self {
            x_range,
            y_range,
            to_scene,
        }
    }

    /// Graph coordinates to a scene point, manim's `c2p`.
    pub fn c2p(&self, x: f64, y: f64) -> Point {
        self.to_scene * Point::new(x, y)
    }

    /// A scene point to graph coordinates, manim's `p2c`.
    pub fn p2c(&self, p: Point) -> Point {
        self.to_scene.inverse() * p
    }

    /// Where the axes cross, in graph coordinates.
    pub fn crossing(&self) -> Point {
        let cross = |[lo, hi, _]: [f64; 3]| 0.0f64.clamp(lo, hi);
        Point::new(cross(self.x_range), cross(self.y_range))
    }

    /// Tick values along x: `min`, `min + step`, ... up to `max`.
    pub fn x_ticks(&self) -> Vec<f64> {
        ticks(self.x_range)
    }

    /// Tick values along y.
    pub fn y_ticks(&self) -> Vec<f64> {
        ticks(self.y_range)
    }

    /// The axis lines and tick marks, in manim's default style.
    pub fn shape(&self) -> VState {
        let o = self.crossing();
        let [x0, x1, _] = self.x_range;
        let [y0, y1, _] = self.y_range;
        let line = |a: Point, b: Point| SubPath::polyline(&[a, b], false);
        // Ticks stay `2 * TICK` long however the axes are scaled.
        let tick = |p: Point, along: Vec2| {
            let n = Vec2::new(-along.y, along.x).normalize() * TICK;
            line(p - n, p + n)
        };
        let (ex, ey) = (
            self.c2p(1.0, 0.0) - self.c2p(0.0, 0.0),
            self.c2p(0.0, 1.0) - self.c2p(0.0, 0.0),
        );
        let mut subpaths = vec![
            line(self.c2p(x0, o.y), self.c2p(x1, o.y)),
            line(self.c2p(o.x, y0), self.c2p(o.x, y1)),
        ];
        subpaths.extend(
            self.x_ticks()
                .into_iter()
                .map(|x| tick(self.c2p(x, o.y), ex)),
        );
        subpaths.extend(
            self.y_ticks()
                .into_iter()
                .map(|y| tick(self.c2p(o.x, y), ey)),
        );
        VState::new(VPath { subpaths }).stroke(WHITE, 0.03)
    }

    /// Graph of `y = f(x)` over `x_range` in graph units, manim's `plot`.
    // ponytail: fixed 64 segments; take a count if a plot shows its corners.
    pub fn plot(&self, f: impl Fn(f64) -> f64, x_range: Range<f64>) -> VState {
        VState::function_graph(f, x_range, 64).transform(self.to_scene)
    }

    /// Line from scene point `p` straight to the x-axis, manim's `get_vertical_line`
    /// (solid, not dashed).
    pub fn vertical_line(&self, p: Point) -> VState {
        let c = self.p2c(p);
        VState::line(p, self.c2p(c.x, self.crossing().y))
    }

    /// Translates by `v`.
    pub fn shift(self, v: Vec2) -> Self {
        Position::transform(self, Affine::translate(v))
    }

    /// Moves so the bounding-box center is at `p`.
    pub fn move_to(self, p: Point) -> Self {
        let c = self.bbox().map_or(p, |b| b.center());
        self.shift(p - c)
    }

    /// Scales about the bounding-box center.
    pub fn scale(self, factor: f64) -> Self {
        let c = self.bbox().map_or(Point::ORIGIN, |b| b.center());
        Position::transform(self, Affine::scale_about(factor, c))
    }
}

fn ticks([lo, hi, step]: [f64; 3]) -> Vec<f64> {
    let n = ((hi - lo) / step + 1e-9).floor() as usize;
    (0..=n).map(|i| lo + step * i as f64).collect()
}

impl Position for Axes {
    /// The span of the axis lines, ticks excluded.
    fn bbox(&self) -> Option<Rect> {
        let [x0, x1, _] = self.x_range;
        let [y0, y1, _] = self.y_range;
        let (a, b) = (self.c2p(x0, y0), self.c2p(x1, y1));
        Some(
            Rect::from_points(a, b)
                .union_pt(self.c2p(x0, y1))
                .union_pt(self.c2p(x1, y0)),
        )
    }
    fn transform(self, a: Affine) -> Self {
        Self {
            to_scene: a * self.to_scene,
            ..self
        }
    }
}
