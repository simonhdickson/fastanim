//! ranim: programmatic mathematical animation in Rust, in the spirit of manim, where
//! transforms are planned with Myers' diff.
//!
//! This is the facade crate; most users only need [`prelude`].

pub use ranim_core as core;
pub use ranim_diff as diff;
pub use ranim_text as text;

/// Common imports for writing scenes.
pub mod prelude {
    pub use ranim_core::color::*;
    pub use ranim_core::{
        Animation, AnimationExt, BakedTimeline, DOWN, DiffStyle, Ease, Group, Interpolate, LEFT,
        Layout, MobjectId, ORIGIN, Parallel, Phasing, Point, RIGHT, RateFn, ReplaceStyle, Scene,
        SceneState, Sequence, TransformDiff, UP, VPath, VState, Vec2, apply_function, circumscribe,
        create, fade_in, fade_out, flash, grow_from_center, indicate, lagged_start, move_to,
        rotate, scale, shift, shrink_to_center, spin_in, to_svg, transform, uncreate, unwrite,
        wiggle, write,
    };
    pub use ranim_diff::{Algorithm, Cleanup, DiffOptions, Differ, Op, TieBreak};
    pub use ranim_text::{TextMobject, math_tex, text, transform_diff};
}
