//! Mobject state (SPEC §4.2): a plain value type that can be cloned, interpolated and snapshotted.

use std::collections::BTreeMap;
use std::f64::consts::TAU;
use std::ops::Range;
use std::sync::Arc;

use kurbo::{Affine, Point, Rect, Vec2};

use crate::Interpolate;
use crate::color::{Color, WHITE};
use crate::geom::VPath;
use crate::image::{Image, ImageFill, lerp_affine};

/// Identifies a mobject within a [`Scene`](crate::Scene).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MobjectId(pub u32);

/// Every mobject's state at one instant, in id order.
pub type SceneState = BTreeMap<MobjectId, VState>;

/// How open outline ends are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineCap {
    /// Flat, at the end point.
    Butt,
    /// A half circle past the end point.
    #[default]
    Round,
    /// A half square past the end point.
    Square,
}

/// How outline corners are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineJoin {
    /// A sharp point (bevelled past a miter limit of 4, as SVG and Canvas do).
    Miter,
    /// Rounded.
    #[default]
    Round,
    /// Cut off flat.
    Bevel,
}

/// Outline style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    /// Stroke color.
    pub color: Color,
    /// Width in scene units.
    pub width: f64,
    /// Ends of open contours.
    pub cap: LineCap,
    /// Corners.
    pub join: LineJoin,
}

impl Stroke {
    /// A `width` outline in `color`, with round caps and joins.
    pub const fn new(color: Color, width: f64) -> Self {
        Self {
            color,
            width,
            cap: LineCap::Round,
            join: LineJoin::Round,
        }
    }
}

/// A linear gradient: `stops` evenly spaced from `from` to `to` (scene units), padded past
/// either end with the end color. It moves with the shape it paints.
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    /// Where the first stop is.
    pub from: Point,
    /// Where the last stop is.
    pub to: Point,
    /// At least one color.
    pub stops: Vec<Color>,
}

impl Gradient {
    /// `stops` across `bbox` along `direction`, e.g. [`RIGHT`](crate::RIGHT) for left to right.
    pub fn across(bbox: Option<Rect>, stops: &[Color], direction: Vec2) -> Self {
        let c = bbox.map_or(Point::ORIGIN, |r| r.center());
        let d = direction.normalize();
        let d = if d.is_finite() {
            d
        } else {
            Vec2::new(1.0, 0.0)
        };
        // Half the box's extent along `d`; never zero, which renderers disagree on.
        let half = bbox.map_or(0.0, |r| {
            (r.width() * d.x.abs() + r.height() * d.y.abs()) / 2.0
        });
        let half = half.max(1e-3);
        Self {
            from: c - d * half,
            to: c + d * half,
            stops: if stops.is_empty() {
                vec![WHITE]
            } else {
                stops.to_vec()
            },
        }
    }

    /// The stop colors with each alpha scaled by `alpha`, and where each sits, `0..=1`.
    pub fn stops(&self, alpha: f32) -> impl Iterator<Item = (f32, Color)> + '_ {
        let n = self.stops.len().saturating_sub(1).max(1) as f32;
        (self.stops.iter().enumerate()).map(move |(i, c)| (i as f32 / n, c.with_alpha(c.a * alpha)))
    }

    /// The same gradient with `n` stops, sampled from this one.
    fn resampled(&self, n: usize) -> Vec<Color> {
        if self.stops.len() == n {
            return self.stops.clone();
        }
        let k = n.saturating_sub(1).max(1) as f32;
        (0..n)
            .map(|i| crate::color::gradient_color(&self.stops, i as f32 / k))
            .collect()
    }
}

/// The animatable state of a shape.
// ponytail: solid fills only and no stored transform (animations bake affines into `path`);
// gradients / decomposed transforms when something needs them.
#[derive(Debug, Clone, PartialEq)]
pub struct VState {
    /// The geometry, in scene units (y up).
    pub path: VPath,
    /// Fill color; transparent by default. Under a [`fill_gradient`](VState::fill_gradient)
    /// only its alpha counts, scaling the gradient's.
    pub fill: Color,
    /// Outline. Under a [`stroke_gradient`](VState::stroke_gradient) only the color's alpha
    /// counts, scaling the gradient's.
    pub stroke: Stroke,
    /// Paints the fill instead of the solid color; boxed, as most shapes have none.
    pub fill_gradient: Option<Box<Gradient>>,
    /// Paints the outline instead of the solid color.
    pub stroke_gradient: Option<Box<Gradient>>,
    /// Overall opacity multiplier.
    pub opacity: f32,
    /// Draw order; higher is on top, ties broken by id.
    pub z_index: i32,
    /// Fraction `0..1` of arc length that is drawn; driven by `create`.
    pub draw_range: Range<f32>,
    /// A raster image drawn under the fill and stroke, moving with the shape; boxed, as
    /// most shapes have none.
    pub image: Option<Box<ImageFill>>,
    /// A WGSL shader painting the fill on the GPU instead of the solid color; renderers
    /// without one (SVG, the web player) draw the solid fill.
    pub shader: Option<Box<ShaderFill>>,
}

/// A WGSL fragment shader painting a shape's fill, over the unit square `transform` places.
///
/// `source` defines `fn shade(uv: vec2<f32>) -> vec4<f32>`, returning a straight-alpha sRGB
/// color for `uv`, `(0, 0)` top-left and `(1, 1)` bottom-right. It can read the uniform
/// `shape`: `shape.color` (the fill color), `shape.size` (the square's size in scene units)
/// and `shape.time` (scene time in seconds).
#[derive(Debug, Clone, PartialEq)]
pub struct ShaderFill {
    /// The WGSL source.
    pub source: Arc<str>,
    /// Maps the unit square into scene units; it moves with the shape.
    pub transform: Affine,
}

impl VState {
    /// A shape with manim's default style: white 0.04-unit outline, no fill.
    pub fn new(path: VPath) -> Self {
        Self {
            path,
            fill: Color::TRANSPARENT,
            stroke: Stroke::new(WHITE, 0.04),
            fill_gradient: None,
            stroke_gradient: None,
            opacity: 1.0,
            z_index: 0,
            draw_range: 0.0..1.0,
            image: None,
            shader: None,
        }
    }

    /// `img` on a rectangle of its size centered on the origin, manim's `ImageMobject`: as
    /// tall as [`Image::default_height`], and with no outline.
    pub fn image(img: Image) -> Self {
        let h = img.default_height();
        let w = h * f64::from(img.width()) / f64::from(img.height());
        let place = Affine::new([w, 0.0, 0.0, -h, -w / 2.0, h / 2.0]);
        Self {
            image: Some(Box::new(ImageFill {
                image: img,
                transform: place,
                smooth: true,
            })),
            ..Self::rectangle(w, h).stroke(Color::TRANSPARENT, 0.0)
        }
    }

    /// Draws the image with nearest-neighbour resampling, so pixels stay sharp squares.
    pub fn pixelated(mut self) -> Self {
        if let Some(i) = &mut self.image {
            i.smooth = false;
        }
        self
    }

    /// Circle centered on the origin.
    pub fn circle(radius: f64) -> Self {
        Self::new(VPath::arc(radius, 0.0, TAU))
    }

    /// Arc around the origin, `sweep` radians counter-clockwise from `start`.
    pub fn arc(radius: f64, start: f64, sweep: f64) -> Self {
        Self::new(VPath::arc(radius, start, sweep))
    }

    /// Axis-aligned ellipse centered on the origin.
    pub fn ellipse(width: f64, height: f64) -> Self {
        let a = Affine::scale_non_uniform(width / 2.0, height / 2.0);
        Self::new(VPath::arc(1.0, 0.0, TAU).transform(a))
    }

    /// The region `op` makes of the two shapes' areas, in the default style (manim's
    /// `Union`, `Intersection`, `Difference` and `Exclusion`).
    pub fn boolean(&self, other: &Self, op: crate::geom::BinaryOp) -> Self {
        Self::new(self.path.boolean(&other.path, op))
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

    /// Open path of straight segments through `points`.
    pub fn polyline(points: &[Point]) -> Self {
        Self::new(VPath::polyline(points, false))
    }

    /// Line segment.
    pub fn line(a: Point, b: Point) -> Self {
        Self::new(VPath::polyline(&[a, b], false))
    }

    /// Graph of `y = f(x)` over `x_range` (scene units), as `segments` smooth cubics
    /// (Hermite, slopes by central difference).
    pub fn function_graph(f: impl Fn(f64) -> f64, x_range: Range<f64>, segments: usize) -> Self {
        let n = segments.max(1);
        let h = (x_range.end - x_range.start) / n as f64;
        let slope = |x: f64| (f(x + h * 1e-3) - f(x - h * 1e-3)) / (h * 2e-3);
        // Each knot is shared by two segments; sample it once, as `f` may be a slow script.
        let knots: Vec<_> = (0..=n)
            .map(|i| {
                let x = x_range.start + h * i as f64;
                (Point::new(x, f(x)), Vec2::new(h, h * slope(x)))
            })
            .collect();
        let segs = knots
            .windows(2)
            .map(|k| {
                let ((p0, d0), (p3, d1)) = (k[0], k[1]);
                kurbo::CubicBez::new(p0, p0 + d0 / 3.0, p3 - d1 / 3.0, p3)
            })
            .collect();
        Self::new(VPath {
            subpaths: vec![crate::geom::SubPath {
                segments: segs,
                closed: false,
            }],
        })
    }

    /// Small filled white dot at `p`.
    pub fn dot(p: Point) -> Self {
        Self::circle(0.08).fill(WHITE).shift(p.to_vec2())
    }

    /// Sets the fill color, replacing any gradient.
    pub fn fill(self, fill: Color) -> Self {
        Self {
            fill,
            fill_gradient: None,
            ..self
        }
    }

    /// Sets the fill's opacity, keeping its color or gradient.
    pub fn fill_opacity(self, opacity: f32) -> Self {
        let fill = self.fill.with_alpha(opacity);
        Self { fill, ..self }
    }

    /// Sets the outline color and width, replacing any gradient; keeps the caps and joins.
    pub fn stroke(self, color: Color, width: f64) -> Self {
        Self {
            stroke: Stroke {
                color,
                width,
                ..self.stroke
            },
            stroke_gradient: None,
            ..self
        }
    }

    /// Sets the outline's opacity, keeping its color or gradient.
    pub fn stroke_opacity(mut self, opacity: f32) -> Self {
        self.stroke.color = self.stroke.color.with_alpha(opacity);
        self
    }

    /// Sets the fill and outline color, keeping their opacities, manim's `set_color`.
    pub fn color(self, c: Color) -> Self {
        let (f, s) = (self.fill.a, self.stroke.color.a);
        let w = self.stroke.width;
        self.fill(c.with_alpha(f)).stroke(c.with_alpha(s), w)
    }

    /// Fills with `stops` from one side of the bounding box to the other along `direction`,
    /// opaque.
    pub fn fill_gradient(self, stops: &[Color], direction: Vec2) -> Self {
        let g = Gradient::across(self.path.bbox(), stops, direction);
        Self {
            fill: WHITE,
            fill_gradient: Some(Box::new(g)),
            ..self
        }
    }

    /// Paints the fill with the WGSL `source` (see [`ShaderFill`]) over the bounding box,
    /// keeping the fill color, or white if there is none, for the shader and other renderers.
    pub fn shader(self, source: &str) -> Self {
        let r = self.path.bbox().unwrap_or_default();
        let place = Affine::new([r.width(), 0.0, 0.0, -r.height(), r.x0, r.y1]);
        Self {
            fill: if self.fill.a > 0.0 { self.fill } else { WHITE },
            shader: Some(Box::new(ShaderFill {
                source: source.into(),
                transform: place,
            })),
            ..self
        }
    }

    /// Strokes the outline with `stops` from one side of the bounding box to the other along
    /// `direction`, opaque, keeping its width.
    pub fn stroke_gradient(mut self, stops: &[Color], direction: Vec2) -> Self {
        let g = Gradient::across(self.path.bbox(), stops, direction);
        self.stroke.color = WHITE;
        self.stroke_gradient = Some(Box::new(g));
        self
    }

    /// Sets how open outline ends are drawn.
    pub fn line_cap(mut self, cap: LineCap) -> Self {
        self.stroke.cap = cap;
        self
    }

    /// Sets how outline corners are drawn.
    pub fn line_join(mut self, join: LineJoin) -> Self {
        self.stroke.join = join;
        self
    }

    /// The outline cut into `dashes` dashes, each `ratio` (`0..=1`) of its period long, manim's
    /// `DashedVMobject`. Open contours start and end on a dash. Being geometry, a dashed shape
    /// still `create`s, dash by dash.
    pub fn dashed(self, dashes: usize, ratio: f64) -> Self {
        let path = self.path.dashed(dashes, ratio);
        Self { path, ..self }
    }

    /// A dashed line from `a` to `b` with dashes about `dash_length` long, `ratio` of each
    /// period, manim's `DashedLine` (whose defaults are 0.05 and 0.5).
    pub fn dashed_line(a: Point, b: Point, dash_length: f64, ratio: f64) -> Self {
        let ratio = ratio.clamp(0.0, 1.0);
        let n = (a.distance(b) * ratio / dash_length.max(1e-6))
            .ceil()
            .max(2.0);
        Self::line(a, b).dashed(n as usize, ratio)
    }

    /// Sets the draw order.
    pub fn z_index(self, z_index: i32) -> Self {
        Self { z_index, ..self }
    }

    /// Applies an affine transform to the geometry, and any image and gradients with it.
    pub fn transform(self, a: Affine) -> Self {
        let tf = |g: Option<Box<Gradient>>| {
            g.map(|g| {
                Box::new(Gradient {
                    from: a * g.from,
                    to: a * g.to,
                    ..*g
                })
            })
        };
        Self {
            path: self.path.transform(a),
            fill_gradient: tf(self.fill_gradient),
            stroke_gradient: tf(self.stroke_gradient),
            image: self.image.map(|i| {
                Box::new(ImageFill {
                    transform: a * i.transform,
                    ..*i
                })
            }),
            shader: self.shader.map(|f| {
                Box::new(ShaderFill {
                    transform: a * f.transform,
                    ..*f
                })
            }),
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

impl crate::position::Position for VState {
    fn bbox(&self) -> Option<kurbo::Rect> {
        self.path.bbox()
    }
    fn transform(self, a: Affine) -> Self {
        VState::transform(self, a)
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
                ..if t < 1.0 { a.stroke } else { b.stroke }
            },
            fill_gradient: lerp_gradient(&a.fill_gradient, a.fill, &b.fill_gradient, b.fill, t),
            stroke_gradient: lerp_gradient(
                &a.stroke_gradient,
                a.stroke.color,
                &b.stroke_gradient,
                b.stroke.color,
                t,
            ),
            opacity: f32::lerp(&a.opacity, &b.opacity, t),
            z_index: if t < 1.0 { a.z_index } else { b.z_index },
            draw_range: f32::lerp(&a.draw_range.start, &b.draw_range.start, t)
                ..f32::lerp(&a.draw_range.end, &b.draw_range.end, t),
            image: lerp_image(&a.image, &b.image, t),
            shader: lerp_shader(&a.shader, &b.shader, t),
        }
    }
}

/// Gradients blend stop by stop, a solid end standing in as a gradient of its one color, so a
/// shape turns from solid to gradient smoothly. Ends are exact.
fn lerp_gradient(
    a: &Option<Box<Gradient>>,
    a_solid: Color,
    b: &Option<Box<Gradient>>,
    b_solid: Color,
    t: f32,
) -> Option<Box<Gradient>> {
    if t <= 0.0 {
        return a.clone();
    }
    if t >= 1.0 {
        return b.clone();
    }
    let solid = |c: Color, like: &Gradient| Gradient {
        stops: vec![c.with_alpha(1.0); like.stops.len()],
        ..like.clone()
    };
    let (x, y) = match (a, b) {
        (None, None) => return None,
        (Some(x), Some(y)) => ((**x).clone(), (**y).clone()),
        (Some(x), None) => ((**x).clone(), solid(b_solid, x)),
        (None, Some(y)) => (solid(a_solid, y), (**y).clone()),
    };
    let n = x.stops.len().max(y.stops.len());
    let t64 = f64::from(t);
    let stops = (x.resampled(n).iter().zip(y.resampled(n)))
        .map(|(p, q)| Color::lerp(p, &q, t))
        .collect();
    Some(Box::new(Gradient {
        from: x.from.lerp(y.from, t64),
        to: x.to.lerp(y.to, t64),
        stops,
    }))
}

/// Placements blend when both ends have an image; the pixels (and a lone image) switch at the
/// end, like `z_index`. Cross-fading two images is left to `fade_out` and `fade_in`.
fn lerp_image(
    a: &Option<Box<ImageFill>>,
    b: &Option<Box<ImageFill>>,
    t: f32,
) -> Option<Box<ImageFill>> {
    let pick = if t < 1.0 { a } else { b };
    match (a, b) {
        (Some(x), Some(y)) => Some(Box::new(ImageFill {
            transform: lerp_affine(x.transform, y.transform, f64::from(t)),
            ..(**pick.as_ref()?).clone()
        })),
        _ => pick.clone(),
    }
}

/// Like [`lerp_image`]: placements blend, the source switches at the end.
fn lerp_shader(
    a: &Option<Box<ShaderFill>>,
    b: &Option<Box<ShaderFill>>,
    t: f32,
) -> Option<Box<ShaderFill>> {
    let pick = if t < 1.0 { a } else { b };
    match (a, b) {
        (Some(x), Some(y)) => Some(Box::new(ShaderFill {
            transform: lerp_affine(x.transform, y.transform, f64::from(t)),
            ..(**pick.as_ref()?).clone()
        })),
        _ => pick.clone(),
    }
}
