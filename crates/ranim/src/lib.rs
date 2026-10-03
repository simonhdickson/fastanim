//! ranim: programmatic mathematical animation in Rust, in the spirit of manim, where
//! transforms are planned with Myers' diff.
//!
//! This is the facade crate; most users only need [`prelude`].

pub use ranim_core as core;
pub use ranim_diff as diff;

/// Common imports for writing scenes.
pub mod prelude {
    pub use ranim_diff::{Algorithm, Cleanup, DiffOptions, Differ, Op, TieBreak};
}
