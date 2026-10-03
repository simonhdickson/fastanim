//! Core model for ranim: geometry, mobject state, animations, the scene builder and the
//! baked, seekable timeline (see `docs/SPEC.md` §4).
//!
//! Has no Bevy dependency so it can be tested with plain `cargo test`.

pub mod anim;
pub mod color;
pub mod geom;
pub mod mobject;
pub mod svg;
pub mod timeline;
pub mod transform_diff;

pub use kurbo;
pub use kurbo::{Point, Vec2};
pub use ranim_diff::Op;

pub use anim::{
    Animation, AnimationExt, Ease, LaggedStart, Overlay, Parallel, RateFn, Sequence, Transform,
    Unwrite, Update, UpdateGroup, apply_function, circumscribe, create, fade_in, fade_out, flash,
    grow_from_center, indicate, lagged_start, move_to, rotate, scale, shift, shrink_to_center,
    spin_in, transform, uncreate, unwrite, wiggle, write,
};
pub use color::Color;
pub use geom::{SubPath, VPath, align};
pub use mobject::{MobjectId, SceneState, Stroke, VState};
pub use svg::to_svg;
pub use timeline::{BakedTimeline, Scene};
pub use transform_diff::{DiffStyle, Group, Layout, Phasing, ReplaceStyle, TransformDiff};

/// Values that can be blended; `lerp(a, b, 0) == a` and `lerp(a, b, 1) == b`.
pub trait Interpolate: Clone {
    /// Blends `a` towards `b` by `t` in `0..=1`.
    fn lerp(a: &Self, b: &Self, t: f32) -> Self;
}

/// Frame height in scene units (manim's).
pub const FRAME_HEIGHT: f64 = 8.0;
/// Frame width in scene units: 16:9.
pub const FRAME_WIDTH: f64 = FRAME_HEIGHT * 16.0 / 9.0;

/// Unit vector up (y is up).
pub const UP: Vec2 = Vec2::new(0.0, 1.0);
/// Unit vector down.
pub const DOWN: Vec2 = Vec2::new(0.0, -1.0);
/// Unit vector left.
pub const LEFT: Vec2 = Vec2::new(-1.0, 0.0);
/// Unit vector right.
pub const RIGHT: Vec2 = Vec2::new(1.0, 0.0);
/// The origin.
pub const ORIGIN: Point = Point::ORIGIN;
