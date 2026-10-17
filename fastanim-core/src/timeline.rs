//! Scene builder and baked, seekable timeline (SPEC §4.5).

use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use kurbo::Rect;

use crate::anim::{Animation, RateFn};
use crate::audio::{Sound, SoundEvent};
use crate::color::{BLACK, Color};
use crate::mobject::{MobjectId, SceneState, VState};

/// A clip and the scene as it was when the clip started (the keyframe).
struct Clip {
    start: f32,
    end: f32,
    before: SceneState,
    anim: Option<(Box<dyn Animation>, RateFn)>,
}

static CLOCK: OnceLock<fn() -> f64> = OnceLock::new();

/// Starts timing bakes with `now`, a clock in milliseconds (`std::time` has none on
/// `wasm32-unknown-unknown`). Only the first call takes effect; until then [`BakeStats`] times
/// are zero.
pub fn time_baking(now: fn() -> f64) {
    let _ = CLOCK.set(now);
}

/// Runs `f`, adding the milliseconds it took to `ms` if a clock is set.
fn timed<T>(ms: &mut f64, f: impl FnOnce() -> T) -> T {
    let Some(now) = CLOCK.get() else {
        return f();
    };
    let start = now();
    let out = f();
    *ms += now() - start;
    out
}

/// Where a bake's time went, outside the script itself (see [`time_baking`]).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BakeStats {
    /// Milliseconds in [`Animation::plan`].
    pub plan_ms: f64,
    /// Milliseconds cloning the scene into each clip's keyframe.
    pub snapshot_ms: f64,
    /// Clips recorded, the closing keyframe included.
    pub clips: usize,
    /// Mobjects summed over every keyframe: how much `snapshot_ms` copied.
    pub snapshot_mobjects: usize,
}

/// What a [`SceneEvent`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// A [`Scene::play`].
    Play,
    /// A [`Scene::always`], running to the end of the scene.
    Updater,
    /// A [`Scene::add_sound`].
    Sound,
}

/// Something that happens in a scene, for the preview's timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneEvent {
    /// Scene time it starts, in seconds.
    pub start: f32,
    /// Scene time it ends, in seconds.
    pub end: f32,
    /// The animation's [label](Animation::label), `"Updater"` or `"Sound"`.
    pub label: String,
    /// What it is.
    pub kind: EventKind,
    /// The mobjects it changes, adds or removes, in id order.
    pub targets: Vec<MobjectId>,
    /// Row in the timeline: plays never overlap and take row 0; overlapping updaters and sounds
    /// stack on the rows below.
    pub lane: usize,
}

type UpdaterFn = Arc<dyn Fn(&SceneState, f32) -> VState + Send + Sync>;

/// Sets one mobject from the scene and the time since it was registered (SPEC §4.4).
#[derive(Clone)]
struct Updater {
    start: f32,
    id: MobjectId,
    f: UpdaterFn,
}

/// Records a scene imperatively, manim-style: `add`, `play`, `wait`. Nothing renders here.
#[derive(Default)]
pub struct Scene {
    state: SceneState,
    clips: Vec<Clip>,
    cursor: f32,
    next_id: u32,
    markers: Vec<(String, f32)>,
    updaters: Vec<Updater>,
    sounds: Vec<SoundEvent>,
    events: Vec<SceneEvent>,
    stats: BakeStats,
    background: Option<Color>,
}

impl Scene {
    /// An empty scene at time 0.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a mobject at the current time.
    pub fn add(&mut self, m: VState) -> MobjectId {
        let id = MobjectId(self.next_id);
        self.next_id += 1;
        self.state.insert(id, m);
        id
    }

    /// Removes a mobject at the current time.
    pub fn remove(&mut self, id: MobjectId) {
        self.state.remove(&id);
    }

    /// The scene as of the current time, e.g. for positioning relative to other mobjects.
    pub fn state(&self) -> &SceneState {
        &self.state
    }

    /// Current time in seconds.
    pub fn time(&self) -> f32 {
        self.cursor
    }

    /// Plans `anim` against the current state, records it and advances time past it.
    pub fn play(&mut self, mut anim: impl Animation + 'static) {
        timed(&mut self.stats.plan_ms, || anim.plan(&self.state));
        let rate = anim.rate_fn();
        let before = self.snapshot();
        // The midpoint too, for animations that end where they started (`indicate`).
        let mid = timed(&mut self.stats.snapshot_ms, || {
            let mut mid = before.clone();
            anim.sample(rate.apply(0.5), &mut mid);
            mid
        });
        anim.sample(rate.apply(1.0), &mut self.state);
        let targets = timed(&mut self.stats.snapshot_ms, || {
            changed(&before, [&mid, &self.state])
        });
        let start = self.cursor;
        anim.sounds(start, &mut self.sounds);
        self.cursor += anim.duration().max(0.0);
        self.event(
            start,
            self.cursor,
            anim.label().into(),
            EventKind::Play,
            targets,
        );
        self.clips.push(Clip {
            start,
            end: self.cursor,
            before,
            anim: Some((Box::new(anim), rate)),
        });
    }

    /// Holds still for `secs`.
    pub fn wait(&mut self, secs: f32) {
        if secs <= 0.0 {
            return;
        }
        // Keyframe now: `add`/`remove` since the last clip must show during the wait.
        let before = self.snapshot();
        let start = self.cursor;
        self.cursor += secs;
        self.clips.push(Clip {
            start,
            end: self.cursor,
            before,
            anim: None,
        });
    }

    /// Starts `sound` now without waiting for it: time doesn't advance, so the scene goes on
    /// underneath. [`wait_for_sounds`](Scene::wait_for_sounds) catches up with it.
    pub fn add_sound(&mut self, sound: Sound) {
        let end = self.cursor + sound.duration;
        self.event(
            self.cursor,
            end,
            "Sound".into(),
            EventKind::Sound,
            Vec::new(),
        );
        self.sounds.push(SoundEvent {
            at: self.cursor,
            sound,
        });
    }

    /// Holds still until every sound started so far has finished.
    pub fn wait_for_sounds(&mut self) {
        let end = self.sounds.iter().map(SoundEvent::end).fold(0.0, f32::max);
        self.wait(end - self.cursor);
    }

    /// From now on, mobject `id` is `f(scene, secs_since_now)` at every frame, until removed.
    ///
    /// `f` sees the scene as animated plus the output of updaters registered before it, so
    /// register in dependency order. Animations still plan against the un-updated state.
    pub fn always(
        &mut self,
        id: MobjectId,
        f: impl Fn(&SceneState, f32) -> VState + Send + Sync + 'static,
    ) {
        // Ends with the scene; `bake` fills it in.
        let t = self.cursor;
        self.event(t, f32::NAN, "Updater".into(), EventKind::Updater, vec![id]);
        self.updaters.push(Updater {
            start: self.cursor,
            id,
            f: Arc::new(f),
        });
    }

    /// Sets the color behind every frame, black by default. It is one color for the whole scene:
    /// the last call wins.
    pub fn background(&mut self, color: Color) {
        self.background = Some(color);
    }

    /// Names the current time as a seek point.
    pub fn marker(&mut self, name: &str) {
        self.markers.push((name.to_owned(), self.cursor));
    }

    fn event(
        &mut self,
        start: f32,
        end: f32,
        label: String,
        kind: EventKind,
        targets: Vec<MobjectId>,
    ) {
        self.events.push(SceneEvent {
            start,
            end,
            label,
            kind,
            targets,
            lane: 0,
        });
    }

    /// A copy of the current state for a clip's keyframe, counted in the bake stats.
    fn snapshot(&mut self) -> SceneState {
        self.stats.clips += 1;
        self.stats.snapshot_mobjects += self.state.len();
        timed(&mut self.stats.snapshot_ms, || self.state.clone())
    }

    /// Finishes recording.
    pub fn bake(mut self) -> BakedTimeline {
        // The closing keyframe moves the state rather than cloning it.
        self.stats.clips += 1;
        self.stats.snapshot_mobjects += self.state.len();
        let tail = Clip {
            start: self.cursor,
            end: self.cursor,
            before: self.state,
            anim: None,
        };
        self.clips.push(tail);
        // Events are recorded in start order; pack updaters and sounds greedily below the plays.
        let mut lane_ends: Vec<f32> = Vec::new();
        for e in &mut self.events {
            if e.end.is_nan() {
                e.end = self.cursor;
            }
            if e.kind == EventKind::Play {
                continue;
            }
            e.lane = 1 + match lane_ends.iter().position(|&end| end <= e.start) {
                Some(i) => i,
                None => {
                    lane_ends.push(0.0);
                    lane_ends.len() - 1
                }
            };
            lane_ends[e.lane - 1] = e.end;
        }
        BakedTimeline {
            events: self.events,
            clips: self.clips,
            duration: self.cursor,
            markers: self.markers,
            updaters: self.updaters,
            sounds: self.sounds,
            stats: self.stats,
            background: self.background.unwrap_or(BLACK),
        }
    }
}

/// The recorded scene: every frame is a pure function of time.
pub struct BakedTimeline {
    events: Vec<SceneEvent>,
    clips: Vec<Clip>,
    duration: f32,
    markers: Vec<(String, f32)>,
    updaters: Vec<Updater>,
    sounds: Vec<SoundEvent>,
    stats: BakeStats,
    background: Color,
}

impl BakedTimeline {
    /// Total length in seconds.
    pub fn duration(&self) -> f32 {
        self.duration
    }

    /// The color behind every frame (see [`Scene::background`]).
    pub fn background(&self) -> Color {
        self.background
    }

    /// Where the bake's time went (see [`time_baking`]).
    pub fn stats(&self) -> BakeStats {
        self.stats
    }

    /// All markers as `(name, time)`, in recording order.
    pub fn markers(&self) -> &[(String, f32)] {
        &self.markers
    }

    /// Every sound and when it starts, in recording order. Sounds may run past
    /// [`duration`](BakedTimeline::duration); players cut them off there.
    pub fn sounds(&self) -> &[SoundEvent] {
        &self.sounds
    }

    /// Every play, updater and sound, in start order.
    pub fn events(&self) -> &[SceneEvent] {
        &self.events
    }

    /// The box around `event`'s targets that are visible at `t`, or failing that at the event's
    /// end (e.g. before a `create` has drawn anything).
    pub fn highlight_box(&self, event: &SceneEvent, t: f32) -> Option<Rect> {
        let bbox = |s: &SceneState| {
            (event.targets.iter())
                .filter_map(|id| s.get(id))
                .filter(|m| m.opacity > 0.0 && m.draw_range.end > m.draw_range.start)
                .filter_map(|m| m.path.bbox())
                .reduce(|a, b| a.union(b))
        };
        bbox(&self.eval(t)).or_else(|| bbox(&self.eval(event.end)))
    }

    /// Time of the first marker called `name`.
    pub fn marker(&self, name: &str) -> Option<f32> {
        self.markers
            .iter()
            .find(|(n, _)| n == name)
            .map(|&(_, t)| t)
    }

    /// The scene at time `t`: the keyframe of the clip containing `t`, plus that clip sampled.
    /// Costs one clip regardless of position or evaluation order.
    pub fn eval(&self, t: f32) -> SceneState {
        // Clips are contiguous in start order and `bake` appends a tail, so this is never empty.
        let i = self
            .clips
            .partition_point(|c| c.start <= t)
            .saturating_sub(1);
        let clip = &self.clips[i];
        let mut out = clip.before.clone();
        if let Some((anim, rate)) = &clip.anim {
            let len = clip.end - clip.start;
            let p = if len > 0.0 {
                (t - clip.start) / len
            } else {
                1.0
            };
            anim.sample(rate.apply(p), &mut out);
        }
        for u in self.updaters.iter().filter(|u| u.start <= t) {
            if out.contains_key(&u.id) {
                let m = (u.f)(&out, t - u.start);
                out.insert(u.id, m);
            }
        }
        out
    }
}

/// Ids added, removed or changed between `before` and any of `after`, in id order.
fn changed(before: &SceneState, after: [&SceneState; 2]) -> Vec<MobjectId> {
    let mut ids = BTreeSet::new();
    for s in after {
        ids.extend(
            s.iter()
                .filter(|&(id, m)| before.get(id) != Some(m))
                .map(|(id, _)| *id),
        );
        ids.extend(before.keys().filter(|id| !s.contains_key(id)));
    }
    ids.into_iter().collect()
}
