//! Mobject state (SPEC §4.2): a plain value type that can be cloned, interpolated and snapshotted.

use std::collections::BTreeMap;
use std::f64::consts::TAU;
use std::ops::Range;

use kurbo::{Affine, Point, Vec2};

use crate::Interpolate;
use crate::color::{Color, WHITE};
use crate::geom::VPath;

/// Identifies a mobject within a [`Scene`](crate::Scene).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MobjectId(pub u32);

/// Every mobject's state at one instant, in id order.
pub type SceneState = BTreeMap<MobjectId, VState>;

/// Outline style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    /// Stroke color.
    pub color: Color,
    /// Width in scene units.
    pub width: f64,
}

/// The animatable state of a shape.
// ponytail: solid fills only and no stored transform (animations bake affines into `path`);
// gradients / decomposed transforms when something needs them.
#[derive(Debug, Clone, PartialEq)]
pub struct VState {
    /// The geometry, in scene units (y up).
    pub path: VPath,
    /// Fill color; transparent by default.
    pub fill: Color,
    /// Outline.
    pub stroke: Stroke,
    /// Overall opacity multiplier.
    pub opacity: f32,
    /// Draw order; higher is on top, ties broken by id.
    pub z_index: i32,
    /// Fraction `0..1` of arc length that is drawn; driven by `create`.
    pub draw_range: Range<f32>,
}

impl VState {
    /// A shape with manim's default style: white 0.04-unit outline, no fill.
    pub fn new(path: VPath) -> Self {
        Self {
            path,
            fill: Color::TRANSPARENT,
            stroke: Stroke {
                color: WHITE,
                width: 0.04,
            },
            opacity: 1.0,
            z_index: 0,
            draw_range: 0.0..1.0,
        }
    }

    /// Circle centered on the origin.
    pub fn circle(radius: f64) -> Self {
        Self::new(VPath::arc(radius, 0.0, TAU))
    }

    /// Arc around the origin, `sweep` radians counter-clockwise from `start`.
    pub fn arc(radius: f64, start: f64, sweep: f64) -> Self {
        Self::new(VPath::arc(radius, start, sweep))
    }

    /// Axis-aligned rectangle centered on the origin.
    pub fn rectangle(width: f64, height: f64) -> Self {
        let (w, h) = (width / 2.0, height / 2.0);
        let pts = [(w, h), (-w, h), (-w, -h), (w, -h)].map(Point::from);
        Self::new(VPath::polyline(&pts, true))
    }

    /// Square centered on the origin.
    pub fn square(side: f64) -> Self {
        Self::rectangle(side, side)
    }

    /// Closed polygon through `points`.
    pub fn polygon(points: &[Point]) -> Self {
        Self::new(VPath::polyline(points, true))
    }

    /// Line segment.
    pub fn line(a: Point, b: Point) -> Self {
        Self::new(VPath::polyline(&[a, b], false))
    }

    /// Small filled white dot at `p`.
    pub fn dot(p: Point) -> Self {
        Self::circle(0.08).fill(WHITE).shift(p.to_vec2())
    }

    /// Sets the fill color.
    pub fn fill(self, fill: Color) -> Self {
        Self { fill, ..self }
    }

    /// Sets the outline.
    pub fn stroke(self, color: Color, width: f64) -> Self {
        Self {
            stroke: Stroke { color, width },
            ..self
        }
    }

    /// Sets the draw order.
    pub fn z_index(self, z_index: i32) -> Self {
        Self { z_index, ..self }
    }

    /// Applies an affine transform to the geometry.
    pub fn transform(self, a: Affine) -> Self {
        Self {
            path: self.path.transform(a),
            ..self
        }
    }

    /// Translates by `v`.
    pub fn shift(self, v: Vec2) -> Self {
        self.transform(Affine::translate(v))
    }

    /// Moves so the bounding-box center is at `p`.
    pub fn move_to(self, p: Point) -> Self {
        let c = self.path.center();
        self.shift(p - c)
    }

    /// Scales about the bounding-box center.
    pub fn scale(self, factor: f64) -> Self {
        let c = self.path.center();
        self.transform(Affine::scale_about(factor, c))
    }

    /// Rotates counter-clockwise by `angle` radians about the bounding-box center.
    pub fn rotate(self, angle: f64) -> Self {
        let c = self.path.center();
        self.transform(Affine::rotate_about(angle, c))
    }
}

impl Interpolate for f32 {
    fn lerp(a: &Self, b: &Self, t: f32) -> Self {
        a + (b - a) * t
    }
}

impl Interpolate for VPath {
    /// Lerps control points. Both paths must already be [aligned](crate::geom::align).
    fn lerp(a: &Self, b: &Self, t: f32) -> Self {
        assert_eq!(
            a.subpaths.len(),
            b.subpaths.len(),
            "lerp of unaligned paths"
        );
        let t64 = f64::from(t);
        let subpaths = a
            .subpaths
            .iter()
            .zip(&b.subpaths)
            .map(|(sa, sb)| {
                assert_eq!(
                    sa.segments.len(),
                    sb.segments.len(),
                    "lerp of unaligned paths"
                );
                let segments = sa
                    .segments
                    .iter()
                    .zip(&sb.segments)
                    .map(|(x, y)| {
                        kurbo::CubicBez::new(
                            x.p0.lerp(y.p0, t64),
                            x.p1.lerp(y.p1, t64),
                            x.p2.lerp(y.p2, t64),
                            x.p3.lerp(y.p3, t64),
                        )
                    })
                    .collect();
                // Only fully closed at an endpoint that is closed.
                let closed = if t <= 0.0 {
                    sa.closed
                } else if t >= 1.0 {
                    sb.closed
                } else {
                    sa.closed && sb.closed
                };
                crate::geom::SubPath { segments, closed }
            })
            .collect();
        VPath { subpaths }
    }
}

impl Interpolate for VState {
    /// Lerps every field. Paths must already be [aligned](crate::geom::align).
    fn lerp(a: &Self, b: &Self, t: f32) -> Self {
        Self {
            path: VPath::lerp(&a.path, &b.path, t),
            fill: Color::lerp(&a.fill, &b.fill, t),
            stroke: Stroke {
                color: Color::lerp(&a.stroke.color, &b.stroke.color, t),
                width: a.stroke.width + (b.stroke.width - a.stroke.width) * f64::from(t),
            },
            opacity: f32::lerp(&a.opacity, &b.opacity, t),
            z_index: if t < 1.0 { a.z_index } else { b.z_index },
            draw_range: f32::lerp(&a.draw_range.start, &b.draw_range.start, t)
                ..f32::lerp(&a.draw_range.end, &b.draw_range.end, t),
        }
    }
}
