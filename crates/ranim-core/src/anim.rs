//! Animations (SPEC §4.4): planned once at bake time, then sampled as a pure function of progress.

use std::f64::consts::{FRAC_PI_2, TAU};

use kurbo::{Affine, CubicBez, Point, Vec2};

use crate::Interpolate;
use crate::color::{Color, YELLOW};
use crate::geom::{VPath, align};
use crate::mobject::{MobjectId, SceneState, Stroke, VState};
use crate::timeline::Scene;
use crate::transform_diff::center;

/// Maps linear progress `0..=1` to eased progress.
#[derive(Debug, Clone, Copy)]
pub enum RateFn {
    /// No easing.
    Linear,
    /// Smootherstep; close to manim's default `smooth`.
    Smooth,
    /// Smooth out to 1 at the midpoint and back to 0.
    ThereAndBack,
    /// Starts slow.
    EaseIn(Ease),
    /// Ends slow.
    EaseOut(Ease),
    /// Starts and ends slow.
    EaseInOut(Ease),
    /// Damped oscillation that overshoots and settles at 1: `stiffness` is the angular
    /// frequency and `damping` the decay rate, both per clip (e.g. 20 and 6).
    Spring {
        /// Angular frequency in radians per clip.
        stiffness: f32,
        /// Exponential decay rate per clip.
        damping: f32,
    },
    /// Any function with `f(0) == 0`.
    Custom(fn(f32) -> f32),
}

/// Curve shape for [`RateFn::EaseIn`] and friends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ease {
    /// `t²`.
    Quad,
    /// `t³`.
    Cubic,
    /// `2^(10t − 10)`.
    Expo,
    /// Pulls back slightly before going.
    Back,
}

impl Ease {
    /// The ease-in curve, exact at both ends.
    fn ease_in(self, t: f32) -> f32 {
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }
        match self {
            Ease::Quad => t * t,
            Ease::Cubic => t * t * t,
            Ease::Expo => 2f32.powf(10.0 * t - 10.0),
            Ease::Back => {
                const C1: f32 = 1.70158;
                (C1 + 1.0) * t * t * t - C1 * t * t
            }
        }
    }
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
            RateFn::EaseIn(e) => e.ease_in(t),
            RateFn::EaseOut(e) => 1.0 - e.ease_in(1.0 - t),
            RateFn::EaseInOut(e) if t < 0.5 => e.ease_in(2.0 * t) / 2.0,
            RateFn::EaseInOut(e) => 1.0 - e.ease_in(2.0 - 2.0 * t) / 2.0,
            RateFn::Spring { .. } if t >= 1.0 => 1.0,
            RateFn::Spring { stiffness, damping } => {
                let f = |t: f32| 1.0 - (-damping * t).exp() * (stiffness * t).cos();
                // Normalized so it lands on exactly 1.
                f(t) / f(1.0)
            }
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

/// Grows from a point at its center, manim's `GrowFromCenter`.
pub fn grow_from_center(id: MobjectId) -> Update {
    Update::new(id, |s, a| s.clone().scale(f64::from(a)))
}

/// Grows from a point while turning a quarter turn into place, manim's `SpinInFromNothing`.
pub fn spin_in(id: MobjectId) -> Update {
    Update::new(id, |s, a| {
        let c = s.path.center();
        let a = f64::from(a);
        s.clone()
            .transform(Affine::rotate_about(FRAC_PI_2 * (a - 1.0), c) * Affine::scale_about(a, c))
    })
}

/// Shrinks to a point at its center. The mobject stays in the scene until removed.
pub fn shrink_to_center(id: MobjectId) -> Update {
    Update::new(id, |s, a| s.clone().scale(f64::from(1.0 - a)))
}

/// Erases the outline backwards along its arc length; the reverse of [`create`]. The mobject
/// stays in the scene until removed.
pub fn uncreate(id: MobjectId) -> Update {
    Update::new(id, |s, a| VState {
        draw_range: 0.0..1.0 - a,
        ..s.clone()
    })
}

/// Moves so the bounding-box center ends at `p`.
pub fn move_to(id: MobjectId, p: Point) -> Update {
    Update::new(id, move |s, a| {
        s.clone().shift((p - s.path.center()) * f64::from(a))
    })
}

/// Morphs every control point `q` towards `f(q)`, manim's `ApplyFunction` / `ApplyPointwise`.
pub fn apply_function(id: MobjectId, f: impl Fn(Point) -> Point + Send + Sync + 'static) -> Update {
    Update::new(id, move |s, a| {
        let t = f64::from(a);
        let mut m = s.clone();
        for seg in m.path.subpaths.iter_mut().flat_map(|sp| &mut sp.segments) {
            let g = |q: Point| q.lerp(f(q), t);
            *seg = CubicBez::new(g(seg.p0), g(seg.p1), g(seg.p2), g(seg.p3));
        }
        m
    })
}

type GroupFn = Box<dyn Fn(&VState, Point, f32) -> VState + Send + Sync>;

/// Rewrites several mobjects together from each one's start state, their joint bounding-box
/// center and progress: the building block for the emphasis animations below.
pub struct UpdateGroup {
    ids: Vec<MobjectId>,
    starts: Vec<VState>,
    center: Point,
    f: GroupFn,
}

impl UpdateGroup {
    /// `f(start_state, group_center, alpha)` gives each mobject's state at `alpha`.
    pub fn new(
        ids: &[MobjectId],
        f: impl Fn(&VState, Point, f32) -> VState + Send + Sync + 'static,
    ) -> Self {
        Self {
            ids: ids.to_vec(),
            starts: Vec::new(),
            center: Point::ORIGIN,
            f: Box::new(f),
        }
    }
}

impl Animation for UpdateGroup {
    fn plan(&mut self, state: &SceneState) {
        self.starts = self.ids.iter().map(|&id| get(state, id).clone()).collect();
        self.center = center(self.starts.iter());
    }
    fn sample(&self, alpha: f32, state: &mut SceneState) {
        for (&id, s) in self.ids.iter().zip(&self.starts) {
            state.insert(id, (self.f)(s, self.center, alpha));
        }
    }
}

/// Briefly scales up by 1.2 and tints yellow, then settles back, manim's `Indicate`.
pub fn indicate(ids: &[MobjectId]) -> Timed<UpdateGroup> {
    UpdateGroup::new(ids, |s, c, a| {
        if a <= 0.0 {
            return s.clone();
        }
        let tint = |col: Color| Color::lerp(&col, &YELLOW.with_alpha(col.a), a);
        VState {
            fill: tint(s.fill),
            stroke: Stroke {
                color: tint(s.stroke.color),
                ..s.stroke
            },
            ..s.clone()
        }
        .transform(Affine::scale_about(1.0 + 0.2 * f64::from(a), c))
    })
    .rate(RateFn::ThereAndBack)
}

/// Wobbles in place while swelling slightly, manim's `Wiggle`; lasts 2 s.
pub fn wiggle(ids: &[MobjectId]) -> Timed<UpdateGroup> {
    UpdateGroup::new(ids, |s, c, a| {
        if a <= 0.0 || a >= 1.0 {
            return s.clone();
        }
        let a = f64::from(a);
        let swell = RateFn::ThereAndBack.apply(a as f32) as f64;
        let angle = 0.1 * swell * (3.0 * TAU * a).sin();
        s.clone()
            .transform(Affine::rotate_about(angle, c) * Affine::scale_about(1.0 + 0.1 * swell, c))
    })
    .run_time(2.0)
    .rate(RateFn::Linear)
}

/// Helper shapes added to the scene now, invisible, animated by `f` from their added state, and
/// removed when the clip ends: e.g. [`circumscribe`] and [`flash`].
pub struct Overlay {
    ids: Vec<MobjectId>,
    starts: Vec<VState>,
    f: UpdateFn,
}

impl Overlay {
    /// Adds `shapes` to `s`, hidden (`draw_range` empty); `f(shape, alpha)` gives each one's
    /// state during the clip.
    pub fn new(
        s: &mut Scene,
        shapes: Vec<VState>,
        f: impl Fn(&VState, f32) -> VState + Send + Sync + 'static,
    ) -> Self {
        let ids = (shapes.iter())
            .map(|m| {
                s.add(VState {
                    draw_range: 0.0..0.0,
                    ..m.clone()
                })
            })
            .collect();
        Self {
            ids,
            starts: shapes,
            f: Box::new(f),
        }
    }
}

impl Animation for Overlay {
    fn plan(&mut self, _: &SceneState) {}
    fn sample(&self, alpha: f32, state: &mut SceneState) {
        for (&id, m) in self.ids.iter().zip(&self.starts) {
            if alpha >= 1.0 {
                state.remove(&id);
            } else {
                state.insert(id, (self.f)(m, alpha));
            }
        }
    }
}

/// Draws a yellow rectangle around `ids` and erases it again, manim's `Circumscribe`.
pub fn circumscribe(s: &mut Scene, ids: &[MobjectId]) -> Overlay {
    const BUFF: f64 = 0.2;
    let st = s.state();
    let bbox = (ids.iter())
        .filter_map(|id| get(st, *id).path.bbox())
        .reduce(|a, b| a.union(b))
        .unwrap_or_default()
        .inflate(BUFF, BUFF);
    let rect = VState::rectangle(bbox.width(), bbox.height())
        .move_to(bbox.center())
        .stroke(YELLOW, 0.06);
    Overlay::new(s, vec![rect], |m, a| VState {
        // Traces in over the first half and out over the second.
        draw_range: (2.0 * a - 1.0).max(0.0)..(2.0 * a).min(1.0),
        ..m.clone()
    })
}

/// Twelve short yellow rays burst outwards from `p`, manim's `Flash`.
pub fn flash(s: &mut Scene, p: Point) -> Overlay {
    let rays = (0..12)
        .map(|i| {
            let d = Vec2::from_angle(TAU * f64::from(i) / 12.0);
            VState::line(p + d * 0.3, p + d * 0.8).stroke(YELLOW, 0.04)
        })
        .collect();
    Overlay::new(s, rays, |m, a| VState {
        draw_range: (2.0 * a - 1.0).max(0.0)..(2.0 * a).min(1.0),
        ..m.clone()
    })
}

/// `s` part drawn at progress `p`: over the first half its outline traces in (in its fill color
/// if it has no stroke), over the second its fill fades up while the outline settles back.
fn border_then_fill(s: &VState, p: f32) -> VState {
    if p >= 1.0 {
        return s.clone();
    }
    let outline = if s.stroke.width > 0.0 && s.stroke.color.a > 0.0 {
        s.stroke
    } else {
        Stroke {
            color: s.fill.with_alpha(1.0),
            width: 0.02,
        }
    };
    if p < 0.5 {
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
    }
}

/// Traces the outline in, then fills it, manim's `DrawBorderThenFill`; lasts 2 s. Like one
/// mobject of [`write`].
pub fn draw_border_then_fill(id: MobjectId) -> Timed<Update> {
    Update::new(id, border_then_fill).run_time(2.0)
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
            state.insert(id, border_then_fill(s, p));
        }
    }
    fn duration(&self) -> f32 {
        // manim: 1 s for short texts, up to 2 s for long ones.
        (self.ids.len() as f32 / 15.0).clamp(1.0, 2.0)
    }
}

/// Erases mobjects last to first, manim's `Unwrite`: [`write`] played backwards. The mobjects
/// stay in the scene until removed.
pub struct Unwrite(Write);

/// Unwrites `ids` (e.g. the glyphs of a text); the reverse of [`write`].
pub fn unwrite(ids: &[MobjectId]) -> Unwrite {
    Unwrite(write(ids))
}

impl Animation for Unwrite {
    fn plan(&mut self, state: &SceneState) {
        self.0.plan(state);
    }
    fn sample(&self, alpha: f32, state: &mut SceneState) {
        self.0.sample(1.0 - alpha, state);
    }
    fn duration(&self) -> f32 {
        self.0.duration()
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

/// Morphs one mobject into another already in the scene, then swaps them, manim's
/// `ReplacementTransform`: `target` is hidden until the end, when it takes over and `id` is
/// removed. See [`replacement_transform`].
pub struct ReplacementTransform {
    id: MobjectId,
    target: MobjectId,
    morph: Transform,
}

/// Morphs mobject `id` into mobject `target` (e.g. just `add`ed), leaving only `target`.
pub fn replacement_transform(id: MobjectId, target: MobjectId) -> ReplacementTransform {
    ReplacementTransform {
        id,
        target,
        morph: transform(id, VState::new(VPath::default())),
    }
}

impl Animation for ReplacementTransform {
    fn plan(&mut self, state: &SceneState) {
        self.morph.target = get(state, self.target).clone();
        self.morph.plan(state);
    }
    fn sample(&self, alpha: f32, state: &mut SceneState) {
        if alpha >= 1.0 {
            // `target` is already in `state` as it was added.
            state.remove(&self.id);
        } else {
            state.remove(&self.target);
            self.morph.sample(alpha, state);
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

/// Plays animations one after another, manim's `Succession`; each plans against the scene as
/// the previous one left it.
pub struct Sequence(pub Vec<Box<dyn Animation>>);

impl Animation for Sequence {
    fn plan(&mut self, state: &SceneState) {
        let mut st = state.clone();
        for a in &mut self.0 {
            a.plan(&st);
            a.sample(a.rate_fn().apply(1.0), &mut st);
        }
    }
    fn sample(&self, alpha: f32, state: &mut SceneState) {
        let mut t = alpha * self.duration();
        for a in &self.0 {
            if t < 0.0 {
                break;
            }
            let d = a.duration();
            // `alpha >= 1` finishes everything exactly; the subtraction can land just short.
            let p = if alpha >= 1.0 || d <= 0.0 { 1.0 } else { t / d };
            a.sample(a.rate_fn().apply(p), state);
            t -= d;
        }
    }
    fn duration(&self) -> f32 {
        self.0.iter().map(|a| a.duration()).sum()
    }
    fn rate_fn(&self) -> RateFn {
        RateFn::Linear
    }
}

/// Runs animations together, each starting `lag_ratio` of the previous one's duration after
/// it, manim's `LaggedStart`. Like [`Parallel`], all plan against the same start state.
pub struct LaggedStart {
    anims: Vec<Box<dyn Animation>>,
    starts: Vec<f32>,
}

/// Staggers `anims` by `lag_ratio` (0 is [`Parallel`], 1 is back to back).
pub fn lagged_start(lag_ratio: f32, anims: Vec<Box<dyn Animation>>) -> LaggedStart {
    let starts = (anims.iter())
        .scan(0.0, |t, a| {
            let s = *t;
            *t += a.duration() * lag_ratio;
            Some(s)
        })
        .collect();
    LaggedStart { anims, starts }
}

impl Animation for LaggedStart {
    fn plan(&mut self, state: &SceneState) {
        self.anims.iter_mut().for_each(|a| a.plan(state));
    }
    fn sample(&self, alpha: f32, state: &mut SceneState) {
        let t = alpha * self.duration();
        for (a, s) in self.anims.iter().zip(&self.starts) {
            let d = a.duration();
            let p = if alpha >= 1.0 || d <= 0.0 {
                1.0
            } else {
                (t - s) / d
            };
            a.sample(a.rate_fn().apply(p), state);
        }
    }
    fn duration(&self) -> f32 {
        (self.anims.iter().zip(&self.starts))
            .map(|(a, s)| s + a.duration())
            .fold(0.0, f32::max)
    }
    fn rate_fn(&self) -> RateFn {
        RateFn::Linear
    }
}
