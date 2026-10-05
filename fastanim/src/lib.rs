//! fastanim: programmatic mathematical animation in Rust, in the spirit of manim, where
//! transforms are planned with Myers' diff.
//!
//! This is the facade crate; most users only need [`prelude`].

pub use fastanim_core as core;
pub use fastanim_diff as diff;
pub use fastanim_text as text;

/// Common imports for writing scenes.
pub mod prelude {
    pub use fastanim_core::color::*;
    pub use fastanim_core::{
        Animation, AnimationExt, Axes, BakedTimeline, DEFAULT_BUFF, DOWN, DiffStyle, Ease, Group,
        Interpolate, LEFT, Layout, MobjectId, ORIGIN, Parallel, Phasing, Point, Position, RIGHT,
        RateFn, ReplaceStyle, Scene, SceneState, Sequence, TransformDiff, UP, VPath, VState, Vec2,
        apply_function, arrange, circumscribe, create, draw_border_then_fill, fade_in, fade_out,
        flash, grow_from_center, indicate, lagged_start, move_to, replacement_transform, rotate,
        scale, shift, shrink_to_center, spin_in, to_svg, transform, uncreate, unwrite, wiggle,
        write,
    };
    pub use fastanim_diff::{Algorithm, Cleanup, DiffOptions, Differ, Op, TieBreak};
    pub use fastanim_text::{
        TextMobject, axis_labels, axis_numbers, latex, math_tex, text, transform_diff,
    };
}
