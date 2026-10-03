//! `TransformDiff` (SPEC §7): turns an edit script between two keyed sequences of parts into
//! choreographed animation, so unchanged parts slide, moved parts arc and only real changes
//! fade or morph.

use std::f32::consts::PI;
use std::hash::Hash;
use std::ops::{Deref, Range};

use kurbo::{Affine, Point, Vec2};
use ranim_diff::{Differ, Op};

use crate::Interpolate;
use crate::anim::{Animation, RateFn};
use crate::color::{BLUE, Color, GREEN, GREY, ORANGE, RED, YELLOW};
use crate::geom::align;
use crate::mobject::{MobjectId, SceneState, VState};
use crate::timeline::Scene;

/// Mobjects in the scene grouped into keyed parts, e.g. a text's glyphs grouped into tokens.
/// Derefs to the ids, so it can be passed wherever `&[MobjectId]` is expected.
#[derive(Debug, Clone, PartialEq)]
pub struct Group<K> {
    /// The mobjects, in order.
    pub ids: Vec<MobjectId>,
    /// Each part's key and its range of [`ids`](Group::ids); contiguous and in order.
    pub parts: Vec<(K, Range<usize>)>,
}

impl<K> Group<K> {
    /// Adds `states` to the scene as one group with the given parts.
    pub fn add(s: &mut Scene, states: &[VState], parts: Vec<(K, Range<usize>)>) -> Self {
        Self {
            ids: states.iter().map(|m| s.add(m.clone())).collect(),
            parts,
        }
    }
}

impl<K> Deref for Group<K> {
    type Target = [MobjectId];
    fn deref(&self) -> &[MobjectId] {
        &self.ids
    }
}

/// When each kind of op animates within the clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phasing {
    /// Deletes, then everything that stays, then inserts, one after another.
    Sequential,
    /// The phases overlap (SPEC §7.3): deletes first, inserts last.
    #[default]
    Overlapped,
}

/// How replaced parts change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReplaceStyle {
    /// Aligned path morph.
    #[default]
    Morph,
    /// The old shape fades out, then the new one fades in.
    CrossFade,
}

/// Choreography of a [`TransformDiff`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffStyle {
    /// When each kind of op runs.
    pub phasing: Phasing,
    /// Stagger between mobjects within a phase, as a fraction of one mobject's duration.
    pub lag_ratio: f32,
    /// Moved parts travel along an arc of this angle (radians); 0 is a straight line.
    pub move_arc: f64,
    /// How replaced parts change.
    pub replace: ReplaceStyle,
    /// Briefly tint inserted and replaced parts.
    pub highlight_changes: bool,
    /// Color parts by op while the clip plays (SPEC §7.4): green insert, red delete, blue move,
    /// amber replace, grey equal.
    pub debug: bool,
}

impl Default for DiffStyle {
    fn default() -> Self {
        Self {
            phasing: Phasing::Overlapped,
            lag_ratio: 0.05,
            move_arc: std::f64::consts::FRAC_PI_3,
            replace: ReplaceStyle::Morph,
            highlight_changes: false,
            debug: false,
        }
    }
}

/// What happens to one mobject.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Delete,
    Equal,
    Move,
    Replace,
    Insert,
}

impl Kind {
    /// Phase as a fraction of the clip.
    fn phase(self, phasing: Phasing) -> (f32, f32) {
        match (phasing, self) {
            (Phasing::Overlapped, Kind::Delete) => (0.0, 0.35),
            (Phasing::Overlapped, Kind::Equal | Kind::Move) => (0.2, 0.8),
            (Phasing::Overlapped, Kind::Replace) => (0.3, 0.85),
            (Phasing::Overlapped, Kind::Insert) => (0.6, 1.0),
            (Phasing::Sequential, Kind::Delete) => (0.0, 1.0 / 3.0),
            (Phasing::Sequential, Kind::Equal | Kind::Move | Kind::Replace) => {
                (1.0 / 3.0, 2.0 / 3.0)
            }
            (Phasing::Sequential, Kind::Insert) => (2.0 / 3.0, 1.0),
        }
    }

    fn debug_color(self) -> Color {
        match self {
            Kind::Delete => RED,
            Kind::Equal => GREY,
            Kind::Move => BLUE,
            Kind::Replace => ORANGE,
            Kind::Insert => GREEN,
        }
    }
}

struct Track {
    id: MobjectId,
    kind: Kind,
    /// `None` for deletes: the mobject leaves the scene.
    target: Option<VState>,
    /// Aligned start and end, set by `plan`.
    ends: Option<(VState, VState)>,
    /// Start and length as fractions of the clip, set by `plan`.
    window: (f32, f32),
}

/// Animates a [`Group`] into a new arrangement planned by diffing part keys; made by
/// [`Group::transform_diff`].
pub struct TransformDiff {
    tracks: Vec<Track>,
    ops: Vec<Op>,
    style: DiffStyle,
}

impl<K: Eq + Hash + Clone> Group<K> {
    /// Plans the morph of this group into `to` (states grouped into `to_parts`) and updates
    /// the group to describe the result. Play the returned animation next.
    ///
    /// Parts are matched by key with Myers' diff; moves and replacements pair by distance, and
    /// non-adjacent deletes and inserts of the same `class` pair into a replacement (e.g.
    /// `+` → `−`). New mobjects are added now, invisible; deleted ones leave the scene when
    /// the animation ends.
    pub fn transform_diff<C: Eq + Hash>(
        &mut self,
        s: &mut Scene,
        to: &[VState],
        to_parts: &[(K, Range<usize>)],
        class: impl Fn(&K) -> Option<C>,
    ) -> TransformDiff {
        let state = s.state();
        let ca: Vec<Point> = (self.parts.iter())
            .map(|(_, r)| center(self.ids[r.clone()].iter().map(|id| &state[id])))
            .collect();
        let cb: Vec<Point> = (to_parts.iter())
            .map(|(_, r)| center(to[r.clone()].iter()))
            .collect();
        let ops = Differ::new(&self.parts, to_parts, |(k, _)| k.clone())
            .cost(|i, j| ca[i].distance(cb[j]))
            .class(|(k, _)| class(k))
            .run();

        let glyphs = |parts: &[(K, Range<usize>)], r: Range<usize>| -> Vec<usize> {
            parts[r].iter().flat_map(|(_, g)| g.clone()).collect()
        };
        let mut tracks = Vec::new();
        let mut new_ids = vec![None; to.len()];
        for op in &ops {
            let (a, b, kind) = match op {
                Op::Equal { a, b } => (*a..a + 1, *b..b + 1, Kind::Equal),
                Op::Move { a, b } => (*a..a + 1, *b..b + 1, Kind::Move),
                Op::Replace { a, b } => (a.clone(), b.clone(), Kind::Replace),
                Op::Delete { a } => (*a..a + 1, 0..0, Kind::Delete),
                Op::Insert { b } => (0..0, *b..b + 1, Kind::Insert),
            };
            let (a, b) = (glyphs(&self.parts, a), glyphs(to_parts, b));
            // Pair mobjects in order; leftovers fade out or in.
            for k in 0..a.len().max(b.len()) {
                let (id, kind) = match (a.get(k), b.get(k)) {
                    (Some(&i), Some(_)) => (self.ids[i], kind),
                    (Some(&i), None) => (self.ids[i], Kind::Delete),
                    (None, Some(&j)) => (
                        s.add(VState {
                            opacity: 0.0,
                            ..to[j].clone()
                        }),
                        Kind::Insert,
                    ),
                    (None, None) => unreachable!(),
                };
                let target = b.get(k).map(|&j| {
                    new_ids[j] = Some(id);
                    to[j].clone()
                });
                tracks.push(Track {
                    id,
                    kind,
                    target,
                    ends: None,
                    window: (0.0, 1.0),
                });
            }
        }

        self.ids = new_ids
            .into_iter()
            .map(|id| id.expect("ops cover every target part"))
            .collect();
        self.parts = to_parts.to_vec();
        TransformDiff {
            tracks,
            ops,
            style: DiffStyle::default(),
        }
    }
}

fn center<'a>(states: impl Iterator<Item = &'a VState>) -> Point {
    states
        .filter_map(|m| m.path.bbox())
        .reduce(|a, b| a.union(b))
        .map_or(Point::ORIGIN, |b| b.center())
}

/// Shrunk to a fifth about its center and transparent: where inserts come from and deletes go.
fn vanished(m: &VState) -> VState {
    VState {
        opacity: 0.0,
        ..m.clone().scale(0.2)
    }
}

impl TransformDiff {
    /// Sets the choreography.
    pub fn style(self, style: DiffStyle) -> Self {
        Self { style, ..self }
    }

    /// Colors parts by op while the clip plays; see [`DiffStyle::debug`].
    pub fn debug(mut self) -> Self {
        self.style.debug = true;
        self
    }

    /// The edit script between the parts, for inspection and tests.
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }
}

impl Animation for TransformDiff {
    fn plan(&mut self, state: &SceneState) {
        // Stagger like `write`: within a phase, each mobject starts `lag_ratio` of its duration
        // after the previous one.
        let phasing = self.style.phasing;
        // Kinds sharing a phase are staggered together (and recomputed identically per kind).
        for kind in [
            Kind::Delete,
            Kind::Equal,
            Kind::Move,
            Kind::Replace,
            Kind::Insert,
        ] {
            let (s, e) = kind.phase(phasing);
            let in_phase = |t: &&mut Track| t.kind.phase(phasing) == (s, e);
            let n = self.tracks.iter_mut().filter(in_phase).count();
            let lag = self.style.lag_ratio;
            let w = (e - s) / (1.0 + n.saturating_sub(1) as f32 * lag);
            let mut k = 0.0;
            for t in self.tracks.iter_mut().filter(in_phase) {
                t.window = (s + k * lag * w, w);
                k += 1.0;
            }
        }
        for t in &mut self.tracks {
            let from = state[&t.id].clone();
            t.ends = Some(match (&t.target, t.kind) {
                (None, _) => (from.clone(), vanished(&from)),
                (Some(to), Kind::Insert) => (vanished(to), to.clone()),
                (Some(to), _) => {
                    let (a, b) = align(&from.path, &to.path);
                    (
                        VState { path: a, ..from },
                        VState {
                            path: b,
                            ..to.clone()
                        },
                    )
                }
            });
        }
    }

    fn sample(&self, alpha: f32, state: &mut SceneState) {
        for t in &self.tracks {
            // Exact endpoint: alignment re-segments paths, and deletes leave the scene.
            if alpha >= 1.0 {
                match &t.target {
                    Some(to) => state.insert(t.id, to.clone()),
                    None => state.remove(&t.id),
                };
                continue;
            }
            let (a, b) = t.ends.as_ref().expect("sample before plan");
            let p = RateFn::Smooth.apply((alpha - t.window.0) / t.window.1);
            let mut m = if t.kind == Kind::Replace && self.style.replace == ReplaceStyle::CrossFade
            {
                let (m, f) = if p < 0.5 {
                    (a, 1.0 - 2.0 * p)
                } else {
                    (b, 2.0 * p - 1.0)
                };
                VState {
                    opacity: m.opacity * f,
                    ..m.clone()
                }
            } else {
                VState::lerp(a, b, p)
            };
            if t.kind == Kind::Move && self.style.move_arc != 0.0 {
                // ponytail: a parabolic bow with the arc's sagitta, not a true circular arc.
                // Moving right bows up and moving left bows down, so swapping parts pass on
                // opposite sides.
                let d = b.path.center() - a.path.center();
                let bow = Vec2::new(-d.y, d.x) * (0.5 * (self.style.move_arc / 4.0).tan());
                let h = f64::from(4.0 * p * (1.0 - p));
                m = m.transform(Affine::translate(bow * h));
            }
            if self.style.debug {
                m.fill = t.kind.debug_color().with_alpha(m.fill.a.max(0.5));
            } else if self.style.highlight_changes && matches!(t.kind, Kind::Insert | Kind::Replace)
            {
                m.fill = Color::lerp(&m.fill, &YELLOW.with_alpha(m.fill.a), 0.8 * (PI * p).sin());
            }
            state.insert(t.id, m);
        }
    }

    fn rate_fn(&self) -> RateFn {
        // Each mobject eases within its own phase.
        RateFn::Linear
    }
}
