//! Vector geometry (SPEC §4.1) and path alignment (§4.3).
//!
//! Everything is normalized to cubic Béziers so interpolating two aligned paths is a plain
//! control-point lerp.

use std::f64::consts::FRAC_PI_2;
use std::ops::Range;

use kurbo::{Affine, CubicBez, ParamCurve, ParamCurveArclen, ParamCurveExtrema, Point, Rect};

const ARCLEN_ACCURACY: f64 = 1e-6;

/// A single closed or open contour made of cubic Béziers.
#[derive(Debug, Clone, PartialEq)]
pub struct SubPath {
    /// The segments, each starting where the previous one ends.
    pub segments: Vec<CubicBez>,
    /// Whether the contour joins back to its start.
    pub closed: bool,
}

/// A vector shape: possibly many contours (e.g. the glyph "B" has 3).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VPath {
    /// The contours, in drawing order.
    pub subpaths: Vec<SubPath>,
}

/// A straight line as a cubic (degree-elevated so it interpolates like any other segment).
pub fn line_segment(a: Point, b: Point) -> CubicBez {
    CubicBez::new(a, a.lerp(b, 1.0 / 3.0), a.lerp(b, 2.0 / 3.0), b)
}

impl SubPath {
    /// Straight segments through `points`; `closed` adds the segment back to the first point.
    pub fn polyline(points: &[Point], closed: bool) -> Self {
        let mut segments: Vec<_> = points
            .windows(2)
            .map(|w| line_segment(w[0], w[1]))
            .collect();
        if closed && points.len() > 2 {
            segments.push(line_segment(points[points.len() - 1], points[0]));
        }
        Self { segments, closed }
    }

    /// The mean of the segment start points.
    pub fn centroid(&self) -> Point {
        let n = self.segments.len().max(1) as f64;
        let sum = self
            .segments
            .iter()
            .fold(Point::ORIGIN, |acc, s| acc + s.p0.to_vec2());
        Point::new(sum.x / n, sum.y / n)
    }

    /// Same topology as `self`, shrunk to a single point at its centroid.
    fn collapsed(&self) -> Self {
        let c = self.centroid();
        let seg = CubicBez::new(c, c, c, c);
        Self {
            segments: vec![seg; self.segments.len()],
            closed: self.closed,
        }
    }
}

impl VPath {
    /// A circular arc around the origin, `sweep` radians from `start`, one cubic per quarter turn.
    pub fn arc(radius: f64, start: f64, sweep: f64) -> Self {
        let n = (sweep.abs() / FRAC_PI_2).ceil().max(1.0) as usize;
        let step = sweep / n as f64;
        let k = 4.0 / 3.0 * (step / 4.0).tan() * radius;
        let at = |th: f64| Point::new(radius * th.cos(), radius * th.sin());
        let segments = (0..n)
            .map(|i| {
                let (a, b) = (start + step * i as f64, start + step * (i + 1) as f64);
                let (p0, p3) = (at(a), at(b));
                let t0 = kurbo::Vec2::new(-a.sin(), a.cos()) * k;
                let t1 = kurbo::Vec2::new(-b.sin(), b.cos()) * k;
                CubicBez::new(p0, p0 + t0, p3 - t1, p3)
            })
            .collect();
        let closed = (sweep.abs() - std::f64::consts::TAU).abs() < 1e-9;
        Self {
            subpaths: vec![SubPath { segments, closed }],
        }
    }

    /// Straight segments through `points`.
    pub fn polyline(points: &[Point], closed: bool) -> Self {
        Self {
            subpaths: vec![SubPath::polyline(points, closed)],
        }
    }

    /// Applies an affine transform to every control point.
    pub fn transform(&self, a: Affine) -> Self {
        let subpaths = self
            .subpaths
            .iter()
            .map(|sp| SubPath {
                segments: sp.segments.iter().map(|s| a * *s).collect(),
                closed: sp.closed,
            })
            .collect();
        Self { subpaths }
    }

    /// Tight bounding box, or `None` for an empty path.
    pub fn bbox(&self) -> Option<Rect> {
        self.subpaths
            .iter()
            .flat_map(|sp| &sp.segments)
            .map(|s| s.bounding_box())
            .reduce(|a, b| a.union(b))
    }

    /// Center of the bounding box (the origin for an empty path).
    pub fn center(&self) -> Point {
        self.bbox().map_or(Point::ORIGIN, |r| r.center())
    }

    /// The part between fractions `range` of total arc length, as drawn by `Create`.
    /// A partial path is always open.
    pub fn trim(&self, range: Range<f32>) -> Self {
        if range.start <= 0.0 && range.end >= 1.0 {
            return self.clone();
        }
        let lens: Vec<Vec<f64>> = self
            .subpaths
            .iter()
            .map(|sp| {
                sp.segments
                    .iter()
                    .map(|s| s.arclen(ARCLEN_ACCURACY))
                    .collect()
            })
            .collect();
        let total: f64 = lens.iter().flatten().sum();
        let (from, to) = (f64::from(range.start) * total, f64::from(range.end) * total);
        let mut acc = 0.0;
        let mut subpaths = Vec::new();
        for (sp, lens) in self.subpaths.iter().zip(&lens) {
            let mut segments = Vec::new();
            for (seg, &len) in sp.segments.iter().zip(lens) {
                let (l0, l1) = (acc, acc + len);
                acc = l1;
                if len == 0.0 || l1 <= from || l0 >= to {
                    continue;
                }
                let t0 = if from > l0 {
                    seg.inv_arclen(from - l0, ARCLEN_ACCURACY)
                } else {
                    0.0
                };
                let t1 = if to < l1 {
                    seg.inv_arclen(to - l0, ARCLEN_ACCURACY)
                } else {
                    1.0
                };
                segments.push(seg.subsegment(t0..t1));
            }
            if !segments.is_empty() {
                subpaths.push(SubPath {
                    segments,
                    closed: false,
                });
            }
        }
        Self { subpaths }
    }
}

/// Brings two paths to the same topology so they can be lerped (SPEC §4.3).
///
/// Subpaths pair by index; missing ones grow from / shrink to a point. Within a pair the shorter
/// side's longest segments are split until counts match, and closed contours are rotated to the
/// start point that minimizes travel.
// ponytail: subpaths pair by index; diff-based pairing via shape signatures (§5.6) lands with glyphs in M5/M6.
pub fn align(a: &VPath, b: &VPath) -> (VPath, VPath) {
    let n = a.subpaths.len().max(b.subpaths.len());
    let (mut out_a, mut out_b) = (VPath::default(), VPath::default());
    for i in 0..n {
        let (sa, sb) = match (a.subpaths.get(i), b.subpaths.get(i)) {
            (Some(x), Some(y)) => (x.clone(), y.clone()),
            (Some(x), None) => (x.clone(), x.collapsed()),
            (None, Some(y)) => (y.collapsed(), y.clone()),
            (None, None) => unreachable!(),
        };
        let (sa, sb) = align_subpaths(sa, sb);
        out_a.subpaths.push(sa);
        out_b.subpaths.push(sb);
    }
    (out_a, out_b)
}

fn align_subpaths(mut a: SubPath, mut b: SubPath) -> (SubPath, SubPath) {
    if a.segments.is_empty() {
        a = SubPath {
            closed: a.closed,
            ..b.collapsed()
        };
    } else if b.segments.is_empty() {
        b = SubPath {
            closed: b.closed,
            ..a.collapsed()
        };
    }
    let n = a.segments.len().max(b.segments.len());
    subdivide_to(&mut a.segments, n);
    subdivide_to(&mut b.segments, n);
    if a.closed && b.closed {
        let travel = |r: usize| -> f64 {
            (0..n)
                .map(|i| (a.segments[(i + r) % n].p0 - b.segments[i].p0).hypot2())
                .sum()
        };
        let best = (0..n)
            .min_by(|&x, &y| travel(x).total_cmp(&travel(y)))
            .unwrap_or(0);
        a.segments.rotate_left(best);
    }
    (a, b)
}

// ponytail: O(n²) re-scan for the longest segment; fine for shapes, revisit if glyph runs get long.
fn subdivide_to(segs: &mut Vec<CubicBez>, n: usize) {
    while segs.len() < n {
        let lens: Vec<f64> = segs.iter().map(|s| s.arclen(ARCLEN_ACCURACY)).collect();
        let i = (0..segs.len())
            .max_by(|&x, &y| lens[x].total_cmp(&lens[y]).then(y.cmp(&x)))
            .unwrap();
        let (l, r) = segs[i].subdivide();
        segs[i] = l;
        segs.insert(i + 1, r);
    }
}
