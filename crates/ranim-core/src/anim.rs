//! Animations (SPEC §4.4): planned once at bake time, then sampled as a pure function of progress.

use kurbo::{Affine, Vec2};

use crate::Interpolate;
use crate::color::Color;
use crate::geom::align;
use crate::mobject::{MobjectId, SceneState, Stroke, VState};

/// Maps linear progress `0..=1` to eased progress.
#[derive(Debug, Clone, Copy)]
pub enum RateFn {
    /// No easing.
    Linear,
    /// Smootherstep; close to manim's default `smooth`.
    Smooth,
    /// Smooth out to 1 at the midpoint and back to 0.
    ThereAndBack,
    /// Any function with `f(0) == 0`.
    Custom(fn(f32) -> f32),
}

impl RateFn {
    /// Eases `t` (clamped to `0..=1`).
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let smooth = |t: f32| t * t * t * (t * (6.0 * t - 15.0) + 10.0);
        match self {
            RateFn::Linear => t,
            RateFn::Smooth => smooth(t),
            RateFn::ThereAndBack => smooth(if t < 0.5 { 2.0 * t } else { 2.0 - 2.0 * t }),
            RateFn::Custom(f) => f(t),
        }
    }
}

/// Something that changes the scene over a clip.
pub trait Animation: Send + Sync {
    /// Called once when played, with the scene state at the start of the clip. Do the expensive
    /// work (alignment, diffing) here.
    fn plan(&mut self, state: &SceneState);

    /// Writes the targets' state at eased progress `alpha` into `state`, which holds the scene
    /// as it was at the start of the clip. Must be cheap and pure.
    fn sample(&self, alpha: f32, state: &mut SceneState);

    /// Length in seconds.
    fn duration(&self) -> f32 {
        1.0
    }

    /// Easing applied to progress before [`sample`](Animation::sample).
    fn rate_fn(&self) -> RateFn {
        RateFn::Smooth
    }
}

/// Builder methods available on every animation.
pub trait AnimationExt: Animation + Sized {
    /// Overrides the length in seconds.
    fn run_time(self, secs: f32) -> Timed<Self> {
        let rate = self.rate_fn();
        Timed {
            anim: self,
            secs,
            rate,
        }
    }

    /// Overrides the easing.
    fn rate(self, rate: RateFn) -> Timed<Self> {
        let secs = self.duration();
        Timed {
            anim: self,
            secs,
            rate,
        }
    }
}

impl<A: Animation> AnimationExt for A {}

/// An animation with overridden timing; see [`AnimationExt`].
pub struct Timed<A> {
    anim: A,
    secs: f32,
    rate: RateFn,
}

impl<A: Animation> Timed<A> {
    /// Overrides the length in seconds.
    pub fn run_time(self, secs: f32) -> Self {
        Self { secs, ..self }
    }

    /// Overrides the easing.
    pub fn rate(self, rate: RateFn) -> Self {
        Self { rate, ..self }
    }
}

impl<A: Animation> Animation for Timed<A> {
    fn plan(&mut self, state: &SceneState) {
        self.anim.plan(state);
    }
    fn sample(&self, alpha: f32, state: &mut SceneState) {
        self.anim.sample(alpha, state);
    }
    fn duration(&self) -> f32 {
        self.secs
    }
    fn rate_fn(&self) -> RateFn {
        self.rate
    }
}

fn get(state: &SceneState, id: MobjectId) -> &VState {
    state
        .get(&id)
        .unwrap_or_else(|| panic!("{id:?} is not in the scene; `add` it first"))
}

type UpdateFn = Box<dyn Fn(&VState, f32) -> VState + Send + Sync>;

/// Rewrites one mobject from its start-of-clip state and progress: the building block for the
/// simple animations below.
pub struct Update {
    id: MobjectId,
    start: Option<VState>,
    f: UpdateFn,
}

impl Update {
    /// `f(start_state, alpha)` gives the mobject's state at `alpha`.
    pub fn new(id: MobjectId, f: impl Fn(&VState, f32) -> VState + Send + Sync + 'static) -> Self {
        Self {
            id,
            start: None,
            f: Box::new(f),
        }
    }
}

impl Animation for Update {
    fn plan(&mut self, state: &SceneState) {
        self.start = Some(get(state, self.id).clone());
    }
    fn sample(&self, alpha: f32, state: &mut SceneState) {
        let start = self.start.as_ref().expect("sample before plan");
        state.insert(self.id, (self.f)(start, alpha));
    }
}

/// Draws the outline in along its arc length.
pub fn create(id: MobjectId) -> Update {
    Update::new(id, |s, a| VState {
        draw_range: 0.0..a,
        ..s.clone()
    })
}

/// Fades from transparent to the current opacity.
pub fn fade_in(id: MobjectId) -> Update {
    Update::new(id, |s, a| VState {
        opacity: s.opacity * a,
        ..s.clone()
    })
}

/// Fades to transparent. The mobject stays in the scene until removed.
pub fn fade_out(id: MobjectId) -> Update {
    Update::new(id, |s, a| VState {
        opacity: s.opacity * (1.0 - a),
        ..s.clone()
    })
}

/// Translates by `v`.
pub fn shift(id: MobjectId, v: Vec2) -> Update {
    Update::new(id, move |s, a| {
        s.clone().transform(Affine::translate(v * f64::from(a)))
    })
}

/// Rotates counter-clockwise by `angle` radians about the start bounding-box center.
pub fn rotate(id: MobjectId, angle: f64) -> Update {
    Update::new(id, move |s, a| {
        let c = s.path.center();
        s.clone()
            .transform(Affine::rotate_about(angle * f64::from(a), c))
    })
}

/// Scales by `factor` about the start bounding-box center.
pub fn scale(id: MobjectId, factor: f64) -> Update {
    Update::new(id, move |s, a| {
        let c = s.path.center();
        s.clone()
            .transform(Affine::scale_about(1.0 + (factor - 1.0) * f64::from(a), c))
    })
}

/// Draws mobjects in one after another, manim's `Write`: each outline traces in, then its fill
/// fades up while the outline fades back to its own style. See [`write`].
pub struct Write {
    ids: Vec<MobjectId>,
    starts: Vec<VState>,
}

/// Writes `ids` in order (e.g. the glyphs of a text), staggered like manim's `Write`.
pub fn write(ids: &[MobjectId]) -> Write {
    Write {
        ids: ids.to_vec(),
        starts: Vec::new(),
    }
}

impl Animation for Write {
    fn plan(&mut self, state: &SceneState) {
        self.starts = self.ids.iter().map(|&id| get(state, id).clone()).collect();
    }
    fn sample(&self, alpha: f32, state: &mut SceneState) {
        let n = self.ids.len() as f32;
        // manim's lag ratio: each item starts `lag` of an item's duration after the previous.
        let lag = (4.0 / n.max(1.0)).min(0.2);
        let w = 1.0 / (1.0 + (n - 1.0).max(0.0) * lag);
        for (i, (&id, s)) in self.ids.iter().zip(&self.starts).enumerate() {
            // `alpha >= 1` must give back the exact start; the division can land just short.
            let p = if alpha >= 1.0 {
                1.0
            } else {
                ((alpha - i as f32 * lag * w) / w).clamp(0.0, 1.0)
            };
            if p >= 1.0 {
                state.insert(id, s.clone());
                continue;
            }
            let outline = if s.stroke.width > 0.0 && s.stroke.color.a > 0.0 {
                s.stroke
            } else {
                Stroke {
                    color: s.fill.with_alpha(1.0),
                    width: 0.02,
                }
            };
            let m = if p < 0.5 {
                VState {
                    stroke: outline,
                    fill: s.fill.with_alpha(0.0),
                    draw_range: 0.0..p * 2.0,
                    ..s.clone()
                }
            } else {
                let q = p * 2.0 - 1.0;
                VState {
                    stroke: Stroke {
                        color: Color::lerp(&outline.color, &s.stroke.color, q),
                        width: outline.width + (s.stroke.width - outline.width) * f64::from(q),
                    },
                    fill: s.fill.with_alpha(s.fill.a * q),
                    ..s.clone()
                }
            };
            state.insert(id, m);
        }
    }
    fn duration(&self) -> f32 {
        // manim: 1 s for short texts, up to 2 s for long ones.
        (self.ids.len() as f32 / 15.0).clamp(1.0, 2.0)
    }
}

/// Morphs a mobject into `target` (path, style and all).
pub struct Transform {
    id: MobjectId,
    target: VState,
    ends: Option<(VState, VState)>,
}

/// Morphs mobject `id` into `target`.
pub fn transform(id: MobjectId, target: VState) -> Transform {
    Transform {
        id,
        target,
        ends: None,
    }
}

impl Animation for Transform {
    fn plan(&mut self, state: &SceneState) {
        let from = get(state, self.id);
        let (a, b) = align(&from.path, &self.target.path);
        self.ends = Some((
            VState {
                path: a,
                ..from.clone()
            },
            VState {
                path: b,
                ..self.target.clone()
            },
        ));
    }
    fn sample(&self, alpha: f32, state: &mut SceneState) {
        let (a, b) = self.ends.as_ref().expect("sample before plan");
        // Exact endpoints: alignment re-segments both paths. At 0, `state` already holds the start.
        if alpha >= 1.0 {
            state.insert(self.id, self.target.clone());
        } else if alpha > 0.0 {
            state.insert(self.id, VState::lerp(a, b, alpha));
        }
    }
}

/// Runs animations together; lasts as long as the longest. Each child keeps its own easing.
/// Children targeting the same mobject: the last one wins.
pub struct Parallel(pub Vec<Box<dyn Animation>>);

impl Animation for Parallel {
    fn plan(&mut self, state: &SceneState) {
        self.0.iter_mut().for_each(|a| a.plan(state));
    }
    fn sample(&self, alpha: f32, state: &mut SceneState) {
        let t = alpha * self.duration();
        for a in &self.0 {
            let p = if a.duration() > 0.0 {
                t / a.duration()
            } else {
                1.0
            };
            a.sample(a.rate_fn().apply(p), state);
        }
    }
    fn duration(&self) -> f32 {
        self.0.iter().map(|a| a.duration()).fold(0.0, f32::max)
    }
    fn rate_fn(&self) -> RateFn {
        RateFn::Linear
    }
}
