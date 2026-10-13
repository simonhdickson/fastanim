//! Scene builder and baked, seekable timeline (SPEC §4.5).

use std::sync::{Arc, OnceLock};

use crate::anim::{Animation, RateFn};
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
    stats: BakeStats,
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
        anim.sample(rate.apply(1.0), &mut self.state);
        let start = self.cursor;
        self.cursor += anim.duration().max(0.0);
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

    /// From now on, mobject `id` is `f(scene, secs_since_now)` at every frame, until removed.
    ///
    /// `f` sees the scene as animated plus the output of updaters registered before it, so
    /// register in dependency order. Animations still plan against the un-updated state.
    pub fn always(
        &mut self,
        id: MobjectId,
        f: impl Fn(&SceneState, f32) -> VState + Send + Sync + 'static,
    ) {
        self.updaters.push(Updater {
            start: self.cursor,
            id,
            f: Arc::new(f),
        });
    }

    /// Names the current time as a seek point.
    pub fn marker(&mut self, name: &str) {
        self.markers.push((name.to_owned(), self.cursor));
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
        BakedTimeline {
            clips: self.clips,
            duration: self.cursor,
            markers: self.markers,
            updaters: self.updaters,
            stats: self.stats,
        }
    }
}

/// The recorded scene: every frame is a pure function of time.
pub struct BakedTimeline {
    clips: Vec<Clip>,
    duration: f32,
    markers: Vec<(String, f32)>,
    updaters: Vec<Updater>,
    stats: BakeStats,
}

impl BakedTimeline {
    /// Total length in seconds.
    pub fn duration(&self) -> f32 {
        self.duration
    }

    /// Where the bake's time went (see [`time_baking`]).
    pub fn stats(&self) -> BakeStats {
        self.stats
    }

    /// All markers as `(name, time)`, in recording order.
    pub fn markers(&self) -> &[(String, f32)] {
        &self.markers
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
