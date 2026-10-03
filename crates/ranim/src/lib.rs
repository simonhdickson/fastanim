//! ranim: programmatic mathematical animation in Rust, in the spirit of manim, where
//! transforms are planned with Myers' diff.
//!
//! This is the facade crate; most users only need [`prelude`].

pub use ranim_core as core;
pub use ranim_diff as diff;

/// Common imports for writing scenes.
pub mod prelude {
    pub use ranim_core::color::*;
    pub use ranim_core::{
        Animation, AnimationExt, BakedTimeline, DOWN, Interpolate, LEFT, MobjectId, ORIGIN,
        Parallel, Point, RIGHT, RateFn, Scene, SceneState, UP, VPath, VState, Vec2, create,
        fade_in, fade_out, rotate, scale, shift, to_svg, transform,
    };
    pub use ranim_diff::{Algorithm, Cleanup, DiffOptions, Differ, Op, TieBreak};
}
