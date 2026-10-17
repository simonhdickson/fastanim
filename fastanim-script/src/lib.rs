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

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use fastanim_core::color::{
    BLACK, BLUE, GREEN, GREY, ORANGE, PINK, PURPLE, RED, WHITE, YELLOW, gradient_color,
};
use fastanim_core::kurbo::{Affine, Point, Rect, Vec2};
use fastanim_core::{
    Animation, AnimationExt, Audio, DEFAULT_BUFF, DOWN, DiffStyle, Ease, FRAME_HEIGHT, FRAME_WIDTH,
    Group, Image, LEFT, LineCap, LineJoin, Lottie, LottieMobject, MobjectId, Parallel, Position,
    RIGHT, RateFn, Scene, SceneState, Sequence, UP, Update, UpdateGroup, VState, arrange,
};
use fastanim_text::{TextMobject, TextStyle, TokenKey};

#[cfg(feature = "rhai")]
mod rhai;
#[cfg(feature = "rhai")]
pub use rhai::{Error, bake, bake_in};
#[cfg(all(feature = "rhai", feature = "metadata"))]
pub use rhai::{definitions, signatures};

/// The scene being recorded, plus the text groups that scripts hold by index.
pub struct Ctx {
    /// What the script has recorded so far.
    pub scene: Scene,
    texts: Vec<Group<TokenKey>>,
    /// Each added Lottie, by the id of its first mobject.
    lotties: HashMap<MobjectId, Arc<Lottie>>,
    /// The first error a closure raised, since animations can't fail.
    err: Arc<Mutex<Option<String>>>,
}

/// A mobject added to the scene: a shape, a text's glyphs, or a group of mobjects.
#[derive(Debug, Clone)]
pub enum Mob {
    /// One shape.
    Shape(MobjectId),
    /// A text added with [`Ctx::add_text`].
    Text(usize),
    /// Mobjects animated and read as one: an added [`VGroup`], or [`group`] of handles.
    Group(Arc<[Mob]>),
}

impl Mob {
    /// Member `i` of a group; negative counts from the end.
    pub fn member(&self, i: i64) -> Result<Mob, String> {
        let Mob::Group(ms) = self else {
            return Err("only groups can be indexed".into());
        };
        let n = ms.len() as i64;
        let j = if i < 0 { n + i } else { i };
        (0..n)
            .contains(&j)
            .then(|| ms[j as usize].clone())
            .ok_or_else(|| format!("group index {i} out of range for a group of {n}"))
    }

    /// How many members a group has; 1 for anything else.
    #[allow(clippy::len_without_is_empty)] // a mobject handle is never "empty"
    pub fn len(&self) -> usize {
        match self {
            Mob::Group(ms) => ms.len(),
            _ => 1,
        }
    }
}

/// A group of mobjects already added, e.g. to fade several out at once.
pub fn group(ms: Vec<Mob>) -> Mob {
    Mob::Group(ms.into())
}

/// Something that can go in a [`VGroup`].
#[derive(Clone)]
pub enum Item {
    /// A shape.
    Shape(VState),
    /// A text.
    Text(TextMobject),
    /// A nested group.
    Group(VGroup),
}

impl Position for Item {
    fn bbox(&self) -> Option<Rect> {
        match self {
            Item::Shape(m) => m.bbox(),
            Item::Text(m) => m.bbox(),
            Item::Group(m) => m.bbox(),
        }
    }

    fn transform(self, a: Affine) -> Self {
        match self {
            Item::Shape(m) => Item::Shape(m.transform(a)),
            Item::Text(m) => Item::Text(m.transform(a)),
            Item::Group(m) => Item::Group(m.transform(a)),
        }
    }
}

/// Shapes, texts and groups not yet in a scene, positioned as one (manim's `VGroup`). Adding it
/// gives a [`Mob::Group`] with a member per item.
#[derive(Clone, Default)]
pub struct VGroup(pub Vec<Item>);

impl Position for VGroup {
    fn bbox(&self) -> Option<Rect> {
        (self.0.iter().filter_map(Position::bbox)).reduce(|a, b| a.union(b))
    }

    fn transform(self, a: Affine) -> Self {
        Self(self.0.into_iter().map(|m| m.transform(a)).collect())
    }
}

impl VGroup {
    fn center(&self) -> Point {
        self.bbox().map_or(Point::ORIGIN, |b| b.center())
    }

    /// Translates by `v`.
    pub fn shift(self, v: Vec2) -> Self {
        self.transform(Affine::translate(v))
    }

    /// Moves so the bounding-box center is at `p`.
    pub fn move_to(self, p: Point) -> Self {
        let c = self.center();
        self.shift(p - c)
    }

    /// Scales about the bounding-box center.
    pub fn scale(self, k: f64) -> Self {
        let c = self.center();
        self.transform(Affine::scale_about(k, c))
    }

    /// Rotates counter-clockwise by `angle` radians about the bounding-box center.
    pub fn rotate(self, angle: f64) -> Self {
        let c = self.center();
        self.transform(Affine::rotate_about(angle, c))
    }

    /// Lays the items out in a row in direction `dir`, `buff` apart; see [`arrange`].
    pub fn arrange(self, dir: Vec2, buff: f64) -> Self {
        Self(arrange(self.0, dir, buff))
    }

    /// Colors every shape and glyph in it along `stops`, first to last, nested groups
    /// included, keeping their opacities: manim's `set_color_by_gradient`.
    pub fn set_color_by_gradient(self, stops: &[fastanim_core::Color]) -> Self {
        fn leaves(items: &[Item]) -> usize {
            (items.iter())
                .map(|i| match i {
                    Item::Shape(_) => 1,
                    Item::Text(t) => t.glyphs.len(),
                    Item::Group(g) => leaves(&g.0),
                })
                .sum()
        }
        fn paint(items: Vec<Item>, next: &mut dyn FnMut() -> fastanim_core::Color) -> Vec<Item> {
            (items.into_iter())
                .map(|i| match i {
                    Item::Shape(m) => Item::Shape(m.color(next())),
                    Item::Text(mut t) => {
                        t.glyphs = t.glyphs.into_iter().map(|g| g.color(next())).collect();
                        Item::Text(t)
                    }
                    Item::Group(g) => Item::Group(VGroup(paint(g.0, next))),
                })
                .collect()
        }
        let n = leaves(&self.0).saturating_sub(1).max(1) as f32;
        let mut i = 0;
        let mut next = || {
            i += 1;
            gradient_color(stops, (i - 1) as f32 / n)
        };
        Self(paint(self.0, &mut next))
    }
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
            lotties: HashMap::new(),
            err: Arc::default(),
        }
    }

    /// Starts `a`, or seconds `range` of it, now without waiting for it.
    pub fn add_sound(&mut self, a: &Audio, range: Option<(f32, f32)>) -> Result<(), String> {
        self.scene.add_sound(clip(a, range)?);
        Ok(())
    }

    /// The ids of `m`, all of which must still be in the scene.
    pub fn ids(&self, m: &Mob) -> Result<Vec<MobjectId>, String> {
        let mut ids = Vec::new();
        self.collect(m, &mut ids);
        if ids.iter().all(|id| self.scene.state().contains_key(id)) {
            Ok(ids)
        } else {
            Err("not in the scene: `add` it first, or it was removed".into())
        }
    }

    fn collect(&self, m: &Mob, ids: &mut Vec<MobjectId>) {
        match m {
            Mob::Shape(id) => ids.push(*id),
            Mob::Text(i) => ids.extend(&self.texts[*i].ids),
            Mob::Group(ms) => ms.iter().for_each(|m| self.collect(m, ids)),
        }
    }

    /// The id of `m`, which must be a shape in the scene.
    pub fn shape(&self, m: &Mob) -> Result<MobjectId, String> {
        match m {
            Mob::Shape(_) => Ok(self.ids(m)?[0]),
            Mob::Text(_) => Err("expected a shape, got a text".into()),
            Mob::Group(_) => Err("expected a shape, got a group".into()),
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

    /// Adds a Lottie, as a group of its shapes that [`play_lottie`] plays.
    pub fn add_lottie(&mut self, l: LottieMobject) -> Mob {
        let ids = l.add_to(&mut self.scene);
        self.lotties.insert(ids[0], l.lottie);
        group(ids.into_iter().map(Mob::Shape).collect())
    }

    /// Adds a group's items, in order, as one group.
    pub fn add_group(&mut self, g: VGroup) -> Mob {
        let ms: Vec<Mob> = (g.0.into_iter())
            .map(|m| match m {
                Item::Shape(m) => self.add_shape(m),
                Item::Text(m) => self.add_text(m),
                Item::Group(m) => self.add_group(m),
            })
            .collect();
        group(ms)
    }

    /// Removes `m` from the scene.
    pub fn remove(&mut self, m: &Mob) -> Result<(), String> {
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
        m: &Mob,
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
pub fn get(state: &SceneState, m: &Mob) -> Result<VState, String> {
    match m {
        Mob::Shape(id) => (state.get(id).cloned()).ok_or_else(|| "not in the scene".into()),
        Mob::Text(_) => Err("only shapes can be read back; texts are many glyphs".into()),
        Mob::Group(_) => Err("only shapes can be read back; index the group".into()),
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
    /// Overrides the animation's own label in the preview timeline.
    pub name: Option<String>,
}

impl Anim {
    fn new(
        f: impl Fn(&mut Ctx) -> Result<Box<dyn Animation>, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            build: Arc::new(f),
            run_time: None,
            rate: None,
            name: None,
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
        if let Some(n) = &self.name {
            a = Box::new(a.named(n.clone()));
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
        let ids = cx.ids(&m)?;
        Ok(match m {
            Mob::Shape(_) => Box::new(f(ids[0])),
            Mob::Text(_) | Mob::Group(_) => Box::new(Parallel(
                (ids.into_iter())
                    .map(|id| Box::new(f(id)) as Box<dyn Animation>)
                    .collect(),
            )),
        })
    })
}

/// `shape` on a shape; on a text or group, `group(shape, center, alpha)` on every shape in
/// it, so it moves as a whole.
fn whole(
    m: Mob,
    shape: impl Fn(MobjectId) -> Update + Send + Sync + 'static,
    group: impl Fn(&VState, Point, f32) -> VState + Clone + Send + Sync + 'static,
) -> Anim {
    Anim::new(move |cx| {
        let ids = cx.ids(&m)?;
        Ok(match m {
            Mob::Shape(_) => Box::new(shape(ids[0])),
            Mob::Text(_) | Mob::Group(_) => {
                let name = shape(ids[0]).label();
                Box::new(UpdateGroup::new(&ids, group.clone()).named(name))
            }
        })
    })
}

fn by_ids<A: Animation + 'static>(
    m: Mob,
    f: impl Fn(&[MobjectId]) -> A + Send + Sync + 'static,
) -> Anim {
    Anim::new(move |cx| Ok(Box::new(f(&cx.ids(&m)?))))
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

/// Fades `m` in while moving it by `v` and scaling it from `k` times the size into place.
pub fn fade_in_with(m: Mob, v: Vec2, k: f64) -> Anim {
    whole(
        m,
        move |id| fastanim_core::fade_in_with(id, v, k),
        move |s, c, a| fastanim_core::fade_pose(s, c, -v, k, 1.0 - a),
    )
}

/// Fades `m` out while moving it by `v` and scaling it to `k` times the size.
pub fn fade_out_with(m: Mob, v: Vec2, k: f64) -> Anim {
    whole(
        m,
        move |id| fastanim_core::fade_out_with(id, v, k),
        move |s, c, a| fastanim_core::fade_pose(s, c, v, k, a),
    )
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
        let ids = cx.ids(&m)?;
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
            cx.shape(&m)?,
            target.clone(),
        )))
    })
}

/// Morphs shape `m` into shape `target`, which replaces it.
pub fn replacement_transform(m: Mob, target: Mob) -> Anim {
    Anim::new(move |cx| {
        let (a, b) = (cx.shape(&m)?, cx.shape(&target)?);
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
            cx.shape(&m)?,
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
        Ok(Box::new(Update::new(cx.shape(&m)?, move |s, a| {
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
            return Err("expected a text".into());
        };
        cx.ids(&m)?;
        let (s, from) = (&mut cx.scene, &mut cx.texts[i]);
        let d = if list {
            fastanim_text::transform_list(s, from, &to)
        } else {
            fastanim_text::transform_diff(s, from, &to)
        };
        Ok(Box::new(d.style(style)))
    })
}

/// The glyphs of a mobject (one, for a shape), positioned as a whole.
#[derive(Clone)]
pub struct Glyphs(pub Vec<VState>);

impl Position for Glyphs {
    fn bbox(&self) -> Option<Rect> {
        (self.0.iter().filter_map(|g| g.path.bbox())).reduce(|a, b| a.union(b))
    }

    fn transform(self, a: Affine) -> Self {
        Self(self.0.into_iter().map(|g| g.transform(a)).collect())
    }
}

/// Morphs `m` from its state when played to `f` of that state: a `transform` with the target
/// computed at plan time (Python's `.animate`). `f` must keep the number of glyphs.
pub fn animate(
    m: Mob,
    f: impl Fn(Glyphs) -> Result<Glyphs, String> + Send + Sync + 'static,
) -> Anim {
    Anim::new(move |cx| {
        let ids = cx.ids(&m)?;
        let state = cx.scene.state();
        let to = f(Glyphs(ids.iter().map(|id| state[id].clone()).collect()))?.0;
        if to.len() != ids.len() {
            return Err("`.animate` must keep every glyph".into());
        }
        Ok(Box::new(Parallel(
            (ids.into_iter().zip(to))
                .map(|(id, t)| Box::new(fastanim_core::transform(id, t)) as Box<dyn Animation>)
                .collect(),
        )))
    })
}

/// The PNG or JPEG at `path`, as a shape (see [`VState::image`]).
pub fn image(path: &Path) -> Result<VState, String> {
    if cfg!(target_arch = "wasm32") {
        return Err(
            "image files can't be loaded in the browser player yet; build one from pixels".into(),
        );
    }
    Ok(VState::image(Image::load(path)?))
}

/// An image from rows of pixels, top row first, as a shape. Each pixel is 1 (grey), 3 (RGB) or
/// 4 (RGBA) channel values from 0 to 255, like a NumPy `uint8` array.
pub fn image_from_rows(rows: &[Vec<Vec<f64>>]) -> Result<VState, String> {
    let w = rows.first().map_or(0, Vec::len);
    let mut px = Vec::with_capacity(w * rows.len() * 4);
    let c = |v: f64| v.clamp(0.0, 255.0).round() as u8;
    for (y, row) in rows.iter().enumerate() {
        if row.len() != w {
            return Err(format!("row {y} has {} pixels, row 0 has {w}", row.len()));
        }
        for p in row {
            match p[..] {
                [g] => px.extend([c(g), c(g), c(g), 255]),
                [r, g, b] => px.extend([c(r), c(g), c(b), 255]),
                [r, g, b, a] => px.extend([c(r), c(g), c(b), c(a)]),
                _ => return Err(format!("a pixel is 1, 3 or 4 values, got {}", p.len())),
            }
        }
    }
    Ok(VState::image(Image::from_rgba(
        w as u32,
        rows.len() as u32,
        px,
    )?))
}

/// The Lottie (Bodymovin JSON) file at `path`, showing its first frame (see [`LottieMobject`]).
pub fn lottie(path: &Path) -> Result<LottieMobject, String> {
    if cfg!(target_arch = "wasm32") {
        return Err("Lottie files can't be loaded in the browser player yet".into());
    }
    Ok(LottieMobject::new(Lottie::load(path)?))
}

/// How [`play_lottie`] plays: which frames, and how many times.
#[derive(Debug, Clone, Copy, Default)]
pub struct LottiePlay {
    /// The frame to start at; the file's first by default.
    pub start: Option<f64>,
    /// The frame to end at; the file's last by default. Before `start` plays backwards.
    pub end: Option<f64>,
    /// How many times to play it through.
    pub loops: Option<u32>,
}

/// Sets option `k` of a [`LottiePlay`].
pub fn lottie_option(p: &mut LottiePlay, k: &str, v: f64) -> Result<(), String> {
    match k {
        "start" => p.start = Some(v),
        "end" => p.end = Some(v),
        "loops" if v >= 1.0 && v.fract() == 0.0 => p.loops = Some(v as u32),
        "loops" => return Err(format!("`loops` must be a whole number from 1, got {v}")),
        _ => return Err(format!("unknown option `{k}`: start, end or loops")),
    }
    Ok(())
}

/// Plays the Lottie `m` (as added) at its own frame rate, from wherever it has been moved to.
pub fn play_lottie(m: Mob, how: LottiePlay) -> Anim {
    Anim::new(move |cx| {
        let ids = cx.ids(&m)?;
        let l = (ids.first().and_then(|id| cx.lotties.get(id)))
            .filter(|l| ids.len() == l.shape_count() + 1)
            .ok_or("expected a Lottie: `scene.add(lottie(...))`")?;
        let a = fastanim_core::play_lottie(&ids, l.clone()).frames(
            how.start.unwrap_or(l.in_point()),
            how.end.unwrap_or(l.out_point()),
        );
        Ok(Box::new(a.loops(how.loops.unwrap_or(1))))
    })
}

/// Loads the fonts in the font file at `path` for `text`'s `font` option; returns their
/// family names.
pub fn load_font(path: &Path) -> Result<Vec<String>, String> {
    if cfg!(target_arch = "wasm32") {
        return Err("fonts can't be loaded in the browser player yet".into());
    }
    fastanim_text::load_font(path)
}

/// Sets option `k` of a text's style.
pub fn text_option(style: &mut TextStyle, k: &str, v: Option<String>) -> Result<(), String> {
    match k {
        "font" => style.font = Some(v.ok_or("`font` must be a string")?),
        _ => return Err(format!("unknown text option `{k}`: font")),
    }
    Ok(())
}

/// Loads the WAV at `path`.
pub fn audio(path: &Path) -> Result<Audio, String> {
    if cfg!(target_arch = "wasm32") {
        return Err("audio isn't supported in the browser player yet".into());
    }
    Audio::load(path)
}

/// `a`, or seconds `start..end` of it, which must lie within it.
fn clip(a: &Audio, range: Option<(f32, f32)>) -> Result<fastanim_core::Sound, String> {
    let Some((start, end)) = range else {
        return Ok(a.whole());
    };
    let len = a.duration();
    if !(0.0 <= start && start < end && end <= len) {
        return Err(format!(
            "sound range {start}..{end} must lie within 0..{len} (`{}`)",
            a.path().display()
        ));
    }
    Ok(a.clip(start, end))
}

/// Plays `a`, or seconds `range` of it, lasting until it finishes.
pub fn sound(a: &Audio, range: Option<(f32, f32)>) -> Result<Anim, String> {
    let s = clip(a, range)?;
    Ok(Anim::new(move |_| Ok(Box::new(s.clone()))))
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
pub fn graph<T, E>(
    f: impl Fn(f64) -> Result<f64, E>,
    build: impl FnOnce(&dyn Fn(f64) -> f64) -> T,
) -> Result<T, E> {
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

/// A line cap by name: `"butt"`, `"round"` or `"square"`.
pub fn line_cap(name: &str) -> Result<LineCap, String> {
    Ok(match name {
        "butt" => LineCap::Butt,
        "round" => LineCap::Round,
        "square" => LineCap::Square,
        _ => return Err(format!("unknown line cap `{name}` (butt, round, square)")),
    })
}

/// A line join by name: `"miter"`, `"round"` or `"bevel"`.
pub fn line_join(name: &str) -> Result<LineJoin, String> {
    Ok(match name {
        "miter" => LineJoin::Miter,
        "round" => LineJoin::Round,
        "bevel" => LineJoin::Bevel,
        _ => return Err(format!("unknown line join `{name}` (miter, round, bevel)")),
    })
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
    "PINK",
    "PURPLE",
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
        "PINK" => C(PINK),
        "PURPLE" => C(PURPLE),
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
