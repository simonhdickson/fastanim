//! Positioning helpers (SPEC §8.3): `next_to`, `align_to`, `to_edge` and [`arrange`], for
//! anything with a bounding box that can be transformed.

use kurbo::{Affine, Point, Rect, Vec2};

use crate::{FRAME_HEIGHT, FRAME_WIDTH};

/// manim's default gap between mobjects placed with [`Position::next_to`] or [`arrange`].
pub const DEFAULT_BUFF: f64 = 0.25;
/// manim's default gap to the frame edge for [`Position::to_edge`].
pub const EDGE_BUFF: f64 = 0.5;

/// The bounding-box point in direction `dir`: the center moved to the edge (or corner) along
/// each axis where `dir` is nonzero, e.g. the top-center for `UP`.
pub fn critical_point(b: Rect, dir: Vec2) -> Point {
    // Not `total_cmp`: `-dir` turns 0 into -0, which must still pick the middle.
    let pick = |d: f64, lo: f64, mid: f64, hi: f64| {
        if d > 0.0 {
            hi
        } else if d < 0.0 {
            lo
        } else {
            mid
        }
    };
    let c = b.center();
    Point::new(pick(dir.x, b.x0, c.x, b.x1), pick(dir.y, b.y0, c.y, b.y1))
}

/// Something that can be placed relative to other things by its bounding box. Empty things
/// (no bounding box) are left where they are.
pub trait Position: Sized {
    /// Tight bounding box, or `None` if empty.
    fn bbox(&self) -> Option<Rect>;

    /// Applies an affine transform to the geometry.
    fn transform(self, a: Affine) -> Self;

    /// Places this `buff` units from `other` in direction `dir`, centered on it along the other
    /// axis, manim's `next_to` (e.g. `label.next_to(&dot, UP, DEFAULT_BUFF)`). A diagonal
    /// `dir` places it off a corner.
    fn next_to(self, other: &impl Position, dir: Vec2, buff: f64) -> Self {
        let (Some(me), Some(them)) = (self.bbox(), other.bbox()) else {
            return self;
        };
        let target = critical_point(them, dir) + dir * buff;
        let v = target - critical_point(me, -dir);
        self.transform(Affine::translate(v))
    }

    /// Lines up this edge in direction `dir` with the same edge of `other`, moving only along
    /// the axes where `dir` is nonzero, manim's `align_to` (e.g. `LEFT` aligns left edges).
    fn align_to(self, other: &impl Position, dir: Vec2) -> Self {
        let (Some(me), Some(them)) = (self.bbox(), other.bbox()) else {
            return self;
        };
        let mut v = critical_point(them, dir) - critical_point(me, dir);
        if dir.x == 0.0 {
            v.x = 0.0;
        }
        if dir.y == 0.0 {
            v.y = 0.0;
        }
        self.transform(Affine::translate(v))
    }

    /// Moves against the frame edge in direction `dir` (e.g. `UP`, or `UP + LEFT` for a
    /// corner), leaving manim's 0.5-unit gap.
    fn to_edge(self, dir: Vec2) -> Self {
        let frame = Rect::new(
            -FRAME_WIDTH / 2.0 + EDGE_BUFF,
            -FRAME_HEIGHT / 2.0 + EDGE_BUFF,
            FRAME_WIDTH / 2.0 - EDGE_BUFF,
            FRAME_HEIGHT / 2.0 - EDGE_BUFF,
        );
        self.align_to(&frame, dir)
    }
}

impl Position for Rect {
    fn bbox(&self) -> Option<Rect> {
        Some(*self)
    }
    fn transform(self, a: Affine) -> Self {
        a.transform_rect_bbox(self)
    }
}

impl Position for Point {
    fn bbox(&self) -> Option<Rect> {
        Some(Rect::from_points(*self, *self))
    }
    fn transform(self, a: Affine) -> Self {
        a * self
    }
}

/// Lays `items` out in a row in direction `dir`, `buff` apart and centered on each other along
/// the other axis, keeping the group's center where it was, manim's `arrange`.
pub fn arrange<T: Position>(items: Vec<T>, dir: Vec2, buff: f64) -> Vec<T> {
    let before = union(&items);
    let mut out: Vec<T> = Vec::with_capacity(items.len());
    for item in items {
        let placed = match out.iter().rev().find(|p| p.bbox().is_some()) {
            Some(prev) => item.next_to(prev, dir, buff),
            None => item,
        };
        out.push(placed);
    }
    let (Some(before), Some(after)) = (before, union(&out)) else {
        return out;
    };
    let v = before.center() - after.center();
    out.into_iter()
        .map(|m| m.transform(Affine::translate(v)))
        .collect()
}

fn union<T: Position>(items: &[T]) -> Option<Rect> {
    items
        .iter()
        .filter_map(Position::bbox)
        .reduce(|a, b| a.union(b))
}
