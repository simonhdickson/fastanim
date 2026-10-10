//! Scripted scenes for fastanim (see `docs/SPEC.md` §14 and §15): the scene API as plain Rust
//! over handles, which each scripting language binds. A script runs once, top to bottom,
//! recording into a [`Ctx`]; baking it gives the same [`BakedTimeline`](fastanim_core::BakedTimeline)
//! a Rust scene would, so playback and export never run the script again.
//!
//! The Rhai frontend (the `rhai` feature, on by default) adds [`bake`]:
//!
//! ```
//! # #[cfg(feature = "rhai")] {
//! let tl = fastanim_script::bake(r#"
//!     let c = scene.add(circle(1.0).stroke(BLUE, 0.06));
//!     scene.play(create(c).run_time(2.0));
//!     scene.marker("drawn");
//!     scene.always(c, |state, t| state[c].move_to(point(cos(t), sin(t))));
//!     scene.wait(1.0);
//! "#).unwrap();
//! assert_eq!(tl.duration(), 3.0);
//! assert_eq!(tl.marker("drawn"), Some(2.0));
//! # }
//! ```
//!
//! Scripts see a `scene` variable with `add`, `remove`, `play`, `wait`, `marker`, `always`
//! and `get`; shape, text and animation functions named as in Rust (`circle`, `math_tex`, `latex`,
//! `transform_diff`, ...); `point(x, y)` for points and vectors; and the [`CONSTANTS`]. In Rhai,
//! numbers passed as floats must be written as floats (`1.0`, not `1`).

use std::sync::{Arc, Mutex};

use fastanim_core::color::{BLACK, BLUE, GREEN, GREY, ORANGE, RED, WHITE, YELLOW};
use fastanim_core::kurbo::{Affine, Point, Vec2};
use fastanim_core::{
    Animation, AnimationExt, DEFAULT_BUFF, DOWN, DiffStyle, Ease, FRAME_HEIGHT, FRAME_WIDTH, Group,
    LEFT, MobjectId, Parallel, RIGHT, RateFn, Scene, SceneState, Sequence, UP, Update, UpdateGroup,
    VState,
};
use fastanim_text::{TextMobject, TokenKey};

#[cfg(feature = "rhai")]
mod rhai;
#[cfg(all(feature = "rhai", feature = "metadata"))]
pub use rhai::signatures;
#[cfg(feature = "rhai")]
pub use rhai::{Error, bake};

/// The scene being recorded, plus the text groups that scripts hold by index.
pub struct Ctx {
    /// What the script has recorded so far.
    pub scene: Scene,
    texts: Vec<Group<TokenKey>>,
    /// The first error a closure raised, since animations can't fail.
    err: Arc<Mutex<Option<String>>>,
}

/// A mobject added to the scene: a shape, or a text's glyphs.
#[derive(Debug, Clone, Copy)]
pub enum Mob {
    /// One shape.
    Shape(MobjectId),
    /// A text added with [`Ctx::add_text`].
    Text(usize),
}

impl Default for Ctx {
    fn default() -> Self {
        Self::new()
    }
}

impl Ctx {
    /// An empty scene.
    pub fn new() -> Self {
        Self {
            scene: Scene::new(),
            texts: Vec::new(),
            err: Arc::default(),
        }
    }

    /// The ids of `m`, all of which must still be in the scene.
    pub fn ids(&self, m: Mob) -> Result<Vec<MobjectId>, String> {
        let ids = match m {
            Mob::Shape(id) => vec![id],
            Mob::Text(i) => self.texts[i].ids.clone(),
        };
        if ids.iter().all(|id| self.scene.state().contains_key(id)) {
            Ok(ids)
        } else {
            Err("not in the scene: `add` it first, or it was removed".into())
        }
    }

    /// The id of `m`, which must be a shape in the scene.
    pub fn shape(&self, m: Mob) -> Result<MobjectId, String> {
        match m {
            Mob::Shape(_) => Ok(self.ids(m)?[0]),
            Mob::Text(_) => Err("expected a shape, got a text".into()),
        }
    }

    /// Adds a shape.
    pub fn add_shape(&mut self, m: VState) -> Mob {
        Mob::Shape(self.scene.add(m))
    }

    /// Adds a text's glyphs.
    pub fn add_text(&mut self, t: TextMobject) -> Mob {
        self.texts.push(t.add_to(&mut self.scene));
        Mob::Text(self.texts.len() - 1)
    }

    /// Removes `m` from the scene.
    pub fn remove(&mut self, m: Mob) -> Result<(), String> {
        for id in self.ids(m)? {
            self.scene.remove(id);
        }
        Ok(())
    }

    /// Builds and plays `a`, failing with the first error a closure raised while doing so.
    pub fn play(&mut self, a: &Anim) -> Result<(), String> {
        let anim = a.build(self)?;
        self.scene.play(anim);
        // `play` samples the end of the clip, which runs any closures once.
        match self.err.lock().unwrap().take() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Keeps shape `m` at `f(state, t)` from now on. `f` is tried once first, so a broken
    /// updater fails here rather than silently on every frame.
    pub fn always(
        &mut self,
        m: Mob,
        f: impl Fn(&SceneState, f32) -> Result<VState, String> + Send + Sync + 'static,
    ) -> Result<(), String> {
        let id = self.shape(m)?;
        f(self.scene.state(), 0.0)?;
        let err = self.err.clone();
        self.scene.always(id, move |st, t| {
            f(st, t).unwrap_or_else(|e| {
                record(&err, e);
                st[&id].clone()
            })
        });
        Ok(())
    }
}

fn record(err: &Mutex<Option<String>>, e: String) {
    err.lock().unwrap().get_or_insert(e);
}

/// Shape `m` in `state`.
pub fn get(state: &SceneState, m: Mob) -> Result<VState, String> {
    match m {
        Mob::Shape(id) => (state.get(&id).cloned()).ok_or_else(|| "not in the scene".into()),
        Mob::Text(_) => Err("only shapes can be read back; texts are many glyphs".into()),
    }
}

type Build = dyn Fn(&mut Ctx) -> Result<Box<dyn Animation>, String> + Send + Sync;

/// An animation as scripts see it: built against the scene when played, since some (like
/// `circumscribe` and `transform_diff`) add to the scene.
#[derive(Clone)]
pub struct Anim {
    build: Arc<Build>,
    /// Overrides the animation's own run time.
    pub run_time: Option<f32>,
    /// Overrides the animation's own rate.
    pub rate: Option<RateFn>,
}

impl Anim {
    fn new(
        f: impl Fn(&mut Ctx) -> Result<Box<dyn Animation>, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            build: Arc::new(f),
            run_time: None,
            rate: None,
        }
    }

    fn build(&self, cx: &mut Ctx) -> Result<Box<dyn Animation>, String> {
        let mut a = (self.build)(cx)?;
        if let Some(t) = self.run_time {
            a = Box::new(a.run_time(t));
        }
        if let Some(r) = self.rate {
            a = Box::new(a.rate(r));
        }
        Ok(a)
    }
}

/// `f` on a shape, or on every glyph of a text together.
fn each<A: Animation + 'static>(
    m: Mob,
    f: impl Fn(MobjectId) -> A + Send + Sync + 'static,
) -> Anim {
    Anim::new(move |cx| {
        let ids = cx.ids(m)?;
        Ok(match m {
            Mob::Shape(_) => Box::new(f(ids[0])),
            Mob::Text(_) => Box::new(Parallel(
                (ids.into_iter())
                    .map(|id| Box::new(f(id)) as Box<dyn Animation>)
                    .collect(),
            )),
        })
    })
}

/// `shape` on a shape; on a text, `group(glyph, text_center, alpha)` on every glyph, so the
/// text moves as a whole.
fn whole(
    m: Mob,
    shape: impl Fn(MobjectId) -> Update + Send + Sync + 'static,
    group: impl Fn(&VState, Point, f32) -> VState + Clone + Send + Sync + 'static,
) -> Anim {
    Anim::new(move |cx| {
        let ids = cx.ids(m)?;
        Ok(match m {
            Mob::Shape(_) => Box::new(shape(ids[0])),
            Mob::Text(_) => Box::new(UpdateGroup::new(&ids, group.clone())),
        })
    })
}

fn by_ids<A: Animation + 'static>(
    m: Mob,
    f: impl Fn(&[MobjectId]) -> A + Send + Sync + 'static,
) -> Anim {
    Anim::new(move |cx| Ok(Box::new(f(&cx.ids(m)?))))
}

fn build_all(anims: &[Anim], cx: &mut Ctx) -> Result<Vec<Box<dyn Animation>>, String> {
    anims.iter().map(|a| a.build(cx)).collect()
}

macro_rules! simple {
    ($($(#[$doc:meta])* $how:ident $name:ident;)*) => {$(
        $(#[$doc])*
        pub fn $name(m: Mob) -> Anim {
            $how(m, fastanim_core::$name)
        }
    )*};
}

simple! {
    /// Draws `m`'s outline in.
    each create;
    /// Undraws `m`.
    each uncreate;
    /// Fades `m` in.
    each fade_in;
    /// Fades `m` out.
    each fade_out;
    /// Grows `m` from its center.
    each grow_from_center;
    /// Shrinks `m` to its center.
    each shrink_to_center;
    /// Spins and grows `m` in.
    each spin_in;
    /// Draws `m`'s outline, then fills it.
    each draw_border_then_fill;
    /// Writes `m` in, stroke by stroke.
    by_ids write;
    /// Unwrites `m`.
    by_ids unwrite;
    /// Briefly scales and tints `m`.
    by_ids indicate;
    /// Wiggles `m`.
    by_ids wiggle;
}

/// Moves `m` by `v`.
pub fn shift(m: Mob, v: Vec2) -> Anim {
    each(m, move |id| fastanim_core::shift(id, v))
}

/// Rotates `m` by `angle` about its center.
pub fn rotate(m: Mob, angle: f64) -> Anim {
    whole(
        m,
        move |id| fastanim_core::rotate(id, angle),
        move |s, c, a| (s.clone()).transform(Affine::rotate_about(angle * f64::from(a), c)),
    )
}

/// Scales `m` by `k` about its center.
pub fn scale(m: Mob, k: f64) -> Anim {
    whole(
        m,
        move |id| fastanim_core::scale(id, k),
        move |s, c, a| {
            let k = 1.0 + (k - 1.0) * f64::from(a);
            s.clone().transform(Affine::scale_about(k, c))
        },
    )
}

/// Moves `m`'s center to `p`.
pub fn move_to(m: Mob, p: Point) -> Anim {
    whole(
        m,
        move |id| fastanim_core::move_to(id, p),
        move |s, c, a| s.clone().shift((p - c) * f64::from(a)),
    )
}

/// Draws a rectangle around `m`, then removes it.
pub fn circumscribe(m: Mob) -> Anim {
    Anim::new(move |cx| {
        let ids = cx.ids(m)?;
        Ok(Box::new(fastanim_core::circumscribe(&mut cx.scene, &ids)))
    })
}

/// A burst of lines from `p`.
pub fn flash(p: Point) -> Anim {
    Anim::new(move |cx| Ok(Box::new(fastanim_core::flash(&mut cx.scene, p))))
}

/// Morphs shape `m` into `target`.
pub fn transform(m: Mob, target: VState) -> Anim {
    Anim::new(move |cx| {
        Ok(Box::new(fastanim_core::transform(
            cx.shape(m)?,
            target.clone(),
        )))
    })
}

/// Morphs shape `m` into shape `target`, which replaces it.
pub fn replacement_transform(m: Mob, target: Mob) -> Anim {
    Anim::new(move |cx| {
        let (a, b) = (cx.shape(m)?, cx.shape(target)?);
        Ok(Box::new(fastanim_core::replacement_transform(a, b)))
    })
}

/// Moves every point of shape `m` through `f`. A failing `f` leaves the point where it was.
pub fn apply_function(
    m: Mob,
    f: impl Fn(Point) -> Result<Point, String> + Send + Sync + 'static,
) -> Anim {
    let f = Arc::new(f);
    Anim::new(move |cx| {
        let (err, f) = (cx.err.clone(), f.clone());
        Ok(Box::new(fastanim_core::apply_function(
            cx.shape(m)?,
            move |p| {
                f(p).unwrap_or_else(|e| {
                    record(&err, e);
                    p
                })
            },
        )))
    })
}

/// Sets shape `m` to `f(start, alpha)` while playing. A failing `f` leaves it at the start.
pub fn update(
    m: Mob,
    f: impl Fn(&VState, f32) -> Result<VState, String> + Send + Sync + 'static,
) -> Anim {
    let f = Arc::new(f);
    Anim::new(move |cx| {
        let (err, f) = (cx.err.clone(), f.clone());
        Ok(Box::new(Update::new(cx.shape(m)?, move |s, a| {
            f(s, a).unwrap_or_else(|e| {
                record(&err, e);
                s.clone()
            })
        })))
    })
}

/// Morphs text `m` into `to`, matching unchanged tokens; `list` matches list items instead.
pub fn transform_diff(m: Mob, to: TextMobject, style: DiffStyle, list: bool) -> Anim {
    Anim::new(move |cx| {
        let Mob::Text(i) = m else {
            return Err("expected a text, got a shape".into());
        };
        cx.ids(m)?;
        let (s, from) = (&mut cx.scene, &mut cx.texts[i]);
        let d = if list {
            fastanim_text::transform_list(s, from, &to)
        } else {
            fastanim_text::transform_diff(s, from, &to)
        };
        Ok(Box::new(d.style(style)))
    })
}

/// Plays `a` together.
pub fn parallel(a: Vec<Anim>) -> Anim {
    Anim::new(move |cx| Ok(Box::new(Parallel(build_all(&a, cx)?))))
}

/// Plays `a` one after another.
pub fn sequence(a: Vec<Anim>) -> Anim {
    Anim::new(move |cx| Ok(Box::new(Sequence(build_all(&a, cx)?))))
}

/// Starts each of `a` `lag_ratio` of the way through the one before.
pub fn lagged_start(lag_ratio: f32, a: Vec<Anim>) -> Anim {
    Anim::new(move |cx| {
        let a = build_all(&a, cx)?;
        Ok(Box::new(fastanim_core::lagged_start(lag_ratio, a)))
    })
}

/// The ease called `name`.
pub fn ease(name: &str) -> Result<Ease, String> {
    Ok(match name {
        "quad" => Ease::Quad,
        "cubic" => Ease::Cubic,
        "expo" => Ease::Expo,
        "back" => Ease::Back,
        _ => return Err(format!("unknown ease `{name}`: quad, cubic, expo or back")),
    })
}

/// An option value as a frontend passes it.
#[derive(Debug, Clone, Copy)]
pub enum Opt {
    /// A flag.
    Bool(bool),
    /// A number.
    Num(f64),
}

/// Sets option `k` of a `transform_diff`.
pub fn diff_option(style: &mut DiffStyle, k: &str, v: Opt) -> Result<(), String> {
    let bad = || {
        let ty = match v {
            Opt::Bool(_) => "bool",
            Opt::Num(_) => "f64",
        };
        format!("`{k}` can't be {ty}")
    };
    let flag = || match v {
        Opt::Bool(b) => Ok(b),
        Opt::Num(_) => Err(bad()),
    };
    let num = || match v {
        Opt::Num(n) => Ok(n),
        Opt::Bool(_) => Err(bad()),
    };
    match k {
        "highlight_changes" => style.highlight_changes = flag()?,
        "debug" => style.debug = flag()?,
        "lag_ratio" => style.lag_ratio = num()? as f32,
        "move_arc" => style.move_arc = num()?,
        _ => {
            return Err(format!(
                "unknown option `{k}`: highlight_changes, debug, lag_ratio or move_arc"
            ));
        }
    }
    Ok(())
}

/// `[min, max, step]`, or `[min, max]` with step 1.
pub fn range(n: &[f64]) -> Result<[f64; 3], String> {
    match *n {
        [lo, hi, step] if lo < hi && step > 0.0 => Ok([lo, hi, step]),
        [lo, hi] if lo < hi => Ok([lo, hi, 1.0]),
        _ => Err("expected [min, max] or [min, max, step], min < max, step > 0".into()),
    }
}

/// `build` with fallible `f` as the function; the first error `f` raises fails it.
pub fn graph<E>(
    f: impl Fn(f64) -> Result<f64, E>,
    build: impl FnOnce(&dyn Fn(f64) -> f64) -> VState,
) -> Result<VState, E> {
    let err = std::cell::RefCell::new(None);
    let m = build(&|x| {
        f(x).unwrap_or_else(|e| {
            err.borrow_mut().get_or_insert(e);
            0.0
        })
    });
    err.into_inner().map_or(Ok(m), Err)
}

/// A constant's value.
#[derive(Debug, Clone, Copy)]
pub enum Const {
    /// A direction or point.
    Point(Vec2),
    /// A color.
    Color(fastanim_core::Color),
    /// A rate function.
    Rate(RateFn),
    /// A number.
    Num(f64),
}

/// The names [`constant`] knows; keep in step with it.
pub const CONSTANTS: &[&str] = &[
    "UP",
    "DOWN",
    "LEFT",
    "RIGHT",
    "ORIGIN",
    "WHITE",
    "BLACK",
    "BLUE",
    "RED",
    "GREEN",
    "YELLOW",
    "GREY",
    "ORANGE",
    "TRANSPARENT",
    "LINEAR",
    "SMOOTH",
    "THERE_AND_BACK",
    "DEFAULT_BUFF",
    "FRAME_WIDTH",
    "FRAME_HEIGHT",
    "PI",
    "TAU",
];

/// The constant called `name`.
pub fn constant(name: &str) -> Option<Const> {
    use Const::{Color as C, Num as N, Point as V, Rate as R};
    Some(match name {
        "UP" => V(UP),
        "DOWN" => V(DOWN),
        "LEFT" => V(LEFT),
        "RIGHT" => V(RIGHT),
        "ORIGIN" => V(Vec2::ZERO),
        "WHITE" => C(WHITE),
        "BLACK" => C(BLACK),
        "BLUE" => C(BLUE),
        "RED" => C(RED),
        "GREEN" => C(GREEN),
        "YELLOW" => C(YELLOW),
        "GREY" => C(GREY),
        "ORANGE" => C(ORANGE),
        "TRANSPARENT" => C(fastanim_core::Color::TRANSPARENT),
        "LINEAR" => R(RateFn::Linear),
        "SMOOTH" => R(RateFn::Smooth),
        "THERE_AND_BACK" => R(RateFn::ThereAndBack),
        "DEFAULT_BUFF" => N(DEFAULT_BUFF),
        "FRAME_WIDTH" => N(FRAME_WIDTH),
        "FRAME_HEIGHT" => N(FRAME_HEIGHT),
        "PI" => N(std::f64::consts::PI),
        "TAU" => N(std::f64::consts::TAU),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_constant_resolves() {
        for c in CONSTANTS {
            assert!(constant(c).is_some(), "{c}");
        }
    }
}
