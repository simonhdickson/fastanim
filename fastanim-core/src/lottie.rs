//! Lottie animations: a Bodymovin JSON file parsed once into layers and shapes, then evaluated
//! at any frame into plain [`VState`]s, so it plays in every renderer and moves, scales, fades
//! and groups like any other mobject.
//!
//! Every fill and stroke in the file becomes one mobject, whose path and style change from
//! frame to frame; there is always the same number of them, so a frame is a pure function of
//! time. Supported: shape, solid, null and precomposition layers with parenting, time
//! stretch and time remapping; groups, paths, rectangles, ellipses, stars and polygons;
//! fills, strokes and trim paths; animated properties with eased, held and spatial keyframes.
//! Gradients are drawn in the average of their stops. Masks, mattes, repeaters, dashes,
//! images and text layers are not drawn.
// ponytail: no masks, mattes, repeaters or dashes; add them when a file needs them.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use kurbo::{Affine, CubicBez, ParamCurve, ParamCurveArclen, Point, Vec2};
use serde_json::{Map, Value};

use crate::anim::{Animation, RateFn, get};
use crate::color::Color;
use crate::geom::{SubPath, VPath, line_segment};
use crate::mobject::{MobjectId, SceneState, Stroke, VState};
use crate::position::Position as _;
use crate::{FRAME_HEIGHT, Scene};

/// How deep precompositions may nest, so a file that includes itself fails instead of looping.
const MAX_NESTING: usize = 32;

/// A parsed Lottie file.
#[derive(Debug)]
pub struct Lottie {
    width: f64,
    height: f64,
    fps: f64,
    in_point: f64,
    out_point: f64,
    layers: Vec<Layer>,
    shape_count: usize,
}

impl Lottie {
    /// Reads and parses the Lottie JSON file at `path`.
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&bytes).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Parses Lottie JSON.
    pub fn parse(json: &[u8]) -> Result<Self, String> {
        let root: Value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
        let root = root.as_object().ok_or("a Lottie file is a JSON object")?;
        let num = |k: &str| {
            (root.get(k).and_then(Value::as_f64))
                .ok_or_else(|| format!("not a Lottie file: no `{k}`"))
        };
        let (width, height, fps) = (num("w")?, num("h")?, num("fr")?);
        let (in_point, out_point) = (num("ip")?, num("op")?);
        if width <= 0.0 || height <= 0.0 || fps <= 0.0 || out_point <= in_point {
            return Err(format!(
                "a {width}x{height} Lottie at {fps} fps from frame {in_point} to {out_point} \
                 has nothing to show"
            ));
        }
        let assets = (root.get("assets").and_then(Value::as_array))
            .into_iter()
            .flatten()
            .filter_map(|a| Some((a.get("id")?.as_str()?, a.get("layers")?.as_array()?)))
            .collect();
        let layers = parse_layers(array(root.get("layers")), &assets, 0)?;
        let mut l = Self {
            width,
            height,
            fps,
            in_point,
            out_point,
            layers,
            shape_count: 0,
        };
        l.shape_count = l.shapes(in_point, Affine::IDENTITY).len();
        Ok(l)
    }

    /// Width in pixels.
    pub fn width(&self) -> f64 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> f64 {
        self.height
    }

    /// Frames per second.
    pub fn fps(&self) -> f64 {
        self.fps
    }

    /// The first frame.
    pub fn in_point(&self) -> f64 {
        self.in_point
    }

    /// The frame it ends at; the last frame shown is just before it.
    pub fn out_point(&self) -> f64 {
        self.out_point
    }

    /// Length in seconds at its own frame rate.
    pub fn duration(&self) -> f32 {
        ((self.out_point - self.in_point) / self.fps) as f32
    }

    /// Sizes and centers the animation like an image: as many pixels tall as a 1080p frame
    /// fills its height, centered on the origin, with y flipped up.
    pub fn default_placement(&self) -> Affine {
        let k = FRAME_HEIGHT / 1080.0;
        Affine::new([
            k,
            0.0,
            0.0,
            -k,
            -self.width / 2.0 * k,
            self.height / 2.0 * k,
        ])
    }

    /// Every fill and stroke at `frame`, bottom first, placed into the scene by `place` (from
    /// pixels, y down). Always the same number of shapes; those not showing are empty and
    /// transparent.
    pub fn shapes(&self, frame: f64, place: Affine) -> Vec<VState> {
        // Layers are visible on frames `ip..op`, so the end itself would show nothing.
        let frame = frame.min(self.out_point - 1e-3).max(self.in_point);
        let mut out = Vec::new();
        let cx = Eval { fps: self.fps };
        eval_layers(cx, &self.layers, frame, place, 1.0, true, &mut out);
        out.reverse();
        out
    }

    /// How many shapes [`shapes`](Self::shapes) gives.
    pub fn shape_count(&self) -> usize {
        self.shape_count
    }

    /// The composition's outline in pixels, corners clockwise from the top-left.
    fn corners(&self) -> [Point; 4] {
        let (w, h) = (self.width, self.height);
        [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].map(Point::from)
    }
}

/// A Lottie not yet in a scene: the animation, where it's placed and the frame it shows.
#[derive(Debug, Clone)]
pub struct LottieMobject {
    /// The animation.
    pub lottie: Arc<Lottie>,
    /// Pixels (y down) to scene units.
    pub place: Affine,
    /// The frame shown until it plays.
    pub frame: f64,
}

impl LottieMobject {
    /// `lottie` at its default size and placement, showing its first frame.
    pub fn new(lottie: Lottie) -> Self {
        Self {
            place: lottie.default_placement(),
            frame: lottie.in_point,
            lottie: Arc::new(lottie),
        }
    }

    /// The mobjects it adds: an invisible outline of the composition, which carries the
    /// placement through any later move, then every fill and stroke, bottom first.
    pub fn states(&self) -> Vec<VState> {
        let outline = self.lottie.corners().map(|p| self.place * p);
        let outline = VState::new(VPath::polyline(&outline, true)).stroke(Color::TRANSPARENT, 0.0);
        let mut out = vec![outline];
        out.extend(self.lottie.shapes(self.frame, self.place));
        out
    }

    /// Adds it to `s`; the ids, outline first, are what [`play_lottie`] takes.
    pub fn add_to(&self, s: &mut Scene) -> Vec<MobjectId> {
        self.states().into_iter().map(|m| s.add(m)).collect()
    }

    fn center(&self) -> Point {
        self.bbox().map_or(Point::ORIGIN, |b| b.center())
    }

    /// Translates by `v`.
    pub fn shift(self, v: Vec2) -> Self {
        self.transform(Affine::translate(v))
    }

    /// Moves so the composition's center is at `p`.
    pub fn move_to(self, p: Point) -> Self {
        let c = self.center();
        self.shift(p - c)
    }

    /// Scales about the composition's center.
    pub fn scale(self, k: f64) -> Self {
        let c = self.center();
        self.transform(Affine::scale_about(k, c))
    }

    /// Rotates counter-clockwise by `angle` radians about the composition's center.
    pub fn rotate(self, angle: f64) -> Self {
        let c = self.center();
        self.transform(Affine::rotate_about(angle, c))
    }

    /// Shows frame `frame` until it plays.
    pub fn at_frame(self, frame: f64) -> Self {
        Self { frame, ..self }
    }
}

impl crate::position::Position for LottieMobject {
    fn bbox(&self) -> Option<kurbo::Rect> {
        let c = self.lottie.corners().map(|p| self.place * p);
        let r = kurbo::Rect::from_points(c[0], c[1]);
        Some(c[2..].iter().fold(r, |r, &p| r.union_pt(p)))
    }

    fn transform(self, a: Affine) -> Self {
        Self {
            place: a * self.place,
            ..self
        }
    }
}

/// Plays a Lottie added with [`LottieMobject::add_to`], wherever it has been moved to. It
/// follows its outline (the first id) frame by frame, so animations of the whole group that
/// come before it in a [`Parallel`](crate::Parallel) move, fade or reorder it as it plays.
pub struct PlayLottie {
    ids: Vec<MobjectId>,
    lottie: Arc<Lottie>,
    from: f64,
    to: f64,
    loops: u32,
}

/// Plays the Lottie at `ids` (as [`LottieMobject::add_to`] returned them) once through, at
/// its own frame rate.
pub fn play_lottie(ids: &[MobjectId], lottie: Arc<Lottie>) -> PlayLottie {
    PlayLottie {
        ids: ids.to_vec(),
        from: lottie.in_point,
        to: lottie.out_point,
        lottie,
        loops: 1,
    }
}

impl PlayLottie {
    /// Plays frames `from` to `to` instead; backwards if `to < from`.
    pub fn frames(self, from: f64, to: f64) -> Self {
        Self { from, to, ..self }
    }

    /// Plays it `n` times over, `n` times as long.
    pub fn loops(self, n: u32) -> Self {
        Self {
            loops: n.max(1),
            ..self
        }
    }
}

impl Animation for PlayLottie {
    fn plan(&mut self, state: &SceneState) {
        assert_eq!(
            self.ids.len(),
            self.lottie.shape_count() + 1,
            "play_lottie needs the ids `LottieMobject::add_to` gave"
        );
        get(state, self.ids[0]);
    }

    fn sample(&self, alpha: f32, state: &mut SceneState) {
        let n = f64::from(self.loops);
        let a = f64::from(alpha) * n;
        // Lands on the last frame at the end rather than wrapping back to the first.
        let u = if a >= n { 1.0 } else { a.fract() };
        let frame = self.from + (self.to - self.from) * u;
        // Moves, scales and rotations of the group carry the outline along: the placement is
        // where three of its corners are now.
        let outline = get(state, self.ids[0]);
        let seg = |i: usize| outline.path.subpaths[0].segments[i];
        let (o, x, y) = (seg(0).p0, seg(0).p3, seg(2).p3);
        let (w, h) = (self.lottie.width, self.lottie.height);
        let (dx, dy) = ((x - o) / w, (y - o) / h);
        let place = Affine::new([dx.x, dx.y, dy.x, dy.y, o.x, o.y]);
        let (opacity, z_index) = (outline.opacity, outline.z_index);
        let shapes = self.lottie.shapes(frame, place);
        for (&id, m) in self.ids[1..].iter().zip(shapes) {
            let m = VState {
                opacity: m.opacity * opacity,
                z_index,
                ..m
            };
            state.insert(id, m);
        }
    }

    fn duration(&self) -> f32 {
        (self.to - self.from).abs() as f32 / self.lottie.fps as f32 * self.loops as f32
    }

    fn rate_fn(&self) -> RateFn {
        RateFn::Linear
    }

    fn label(&self) -> std::borrow::Cow<'static, str> {
        "PlayLottie".into()
    }
}

// The model.

#[derive(Debug)]
struct Layer {
    index: Option<i64>,
    parent: Option<i64>,
    in_point: f64,
    out_point: f64,
    start: f64,
    stretch: f64,
    transform: Transform,
    /// Only used as another layer's matte, so not drawn itself.
    matte: bool,
    content: Content,
}

#[derive(Debug)]
enum Content {
    /// A null: only parents others.
    None,
    Shapes(Vec<Shape>),
    Solid {
        width: f64,
        height: f64,
        color: Color,
    },
    Precomp {
        layers: Vec<Layer>,
        /// Seconds into the precomposition, against the layer's time.
        remap: Option<Prop<Vec<f64>>>,
    },
}

#[derive(Debug)]
enum Shape {
    Group(Vec<Shape>, Option<Transform>),
    Path(Prop<Bez>),
    Rect {
        position: Prop<Vec<f64>>,
        size: Prop<Vec<f64>>,
        roundness: Prop<Vec<f64>>,
        reversed: bool,
    },
    Ellipse {
        position: Prop<Vec<f64>>,
        size: Prop<Vec<f64>>,
        reversed: bool,
    },
    Star {
        star: bool,
        points: Prop<Vec<f64>>,
        position: Prop<Vec<f64>>,
        rotation: Prop<Vec<f64>>,
        outer: Prop<Vec<f64>>,
        inner: Prop<Vec<f64>>,
        reversed: bool,
    },
    Fill {
        color: Paint,
        opacity: Prop<Vec<f64>>,
    },
    Stroke {
        color: Paint,
        opacity: Prop<Vec<f64>>,
        width: Prop<Vec<f64>>,
    },
    Trim {
        start: Prop<Vec<f64>>,
        end: Prop<Vec<f64>>,
        offset: Prop<Vec<f64>>,
        individually: bool,
    },
}

/// A solid color, or a gradient with `stops` color stops drawn in their average.
#[derive(Debug)]
enum Paint {
    Solid(Prop<Vec<f64>>),
    Gradient {
        stops: usize,
        colors: Prop<Vec<f64>>,
    },
}

#[derive(Debug, Default)]
struct Transform {
    anchor: Prop<Vec<f64>>,
    position: Position,
    scale: Option<Prop<Vec<f64>>>,
    rotation: Prop<Vec<f64>>,
    opacity: Option<Prop<Vec<f64>>>,
    skew: Prop<Vec<f64>>,
    skew_axis: Prop<Vec<f64>>,
}

#[derive(Debug)]
enum Position {
    Joint(Prop<Vec<f64>>),
    Split(Prop<Vec<f64>>, Prop<Vec<f64>>),
}

impl Default for Position {
    fn default() -> Self {
        Position::Joint(Prop::default())
    }
}

/// A Lottie path: vertices with in and out tangents relative to them.
#[derive(Debug, Clone, Default)]
struct Bez {
    closed: bool,
    v: Vec<Point>,
    i: Vec<Vec2>,
    o: Vec<Vec2>,
}

/// An animatable property.
#[derive(Debug)]
enum Prop<T> {
    Static(T),
    Animated(Vec<Key<T>>),
}

impl<T: Default> Default for Prop<T> {
    fn default() -> Self {
        Prop::Static(T::default())
    }
}

#[derive(Debug)]
struct Key<T> {
    t: f64,
    start: T,
    /// Where it heads; the next key's start when not given.
    end: Option<T>,
    hold: bool,
    ease_out: Ease,
    ease_in: Ease,
    /// Spatial tangents, for positions that move along a curve.
    tangents: Option<(Vec2, Vec2)>,
}

/// A keyframe's easing handle, per dimension.
#[derive(Debug, Default, Clone)]
struct Ease {
    x: Vec<f64>,
    y: Vec<f64>,
}

// Parsing.

type Assets<'a> = HashMap<&'a str, &'a Vec<Value>>;

fn parse_layers(v: &[Value], assets: &Assets, depth: usize) -> Result<Vec<Layer>, String> {
    if depth > MAX_NESTING {
        return Err("precompositions nest too deeply (does one include itself?)".into());
    }
    let mut out = Vec::new();
    for l in v {
        let Some(l) = l.as_object() else { continue };
        if flag(l, "hd") {
            continue;
        }
        let content = match l.get("ty").and_then(int) {
            Some(0) => {
                let id = l.get("refId").and_then(Value::as_str).unwrap_or_default();
                let layers = assets
                    .get(id)
                    .ok_or_else(|| format!("no precomposition `{id}` in the assets"))?;
                Content::Precomp {
                    layers: parse_layers(layers, assets, depth + 1)?,
                    remap: l.get("tm").map(prop).transpose()?,
                }
            }
            Some(1) => Content::Solid {
                width: num(l, "sw", 0.0),
                height: num(l, "sh", 0.0),
                color: hex_color(l.get("sc").and_then(Value::as_str).unwrap_or("#000000")),
            },
            Some(3) => Content::None,
            Some(4) => Content::Shapes(shapes(l.get("shapes"))?),
            // Images, text and anything newer: still parents others, but draws nothing.
            _ => Content::None,
        };
        out.push(Layer {
            index: l.get("ind").and_then(int),
            parent: l.get("parent").and_then(int),
            in_point: num(l, "ip", f64::NEG_INFINITY),
            out_point: num(l, "op", f64::INFINITY),
            start: num(l, "st", 0.0),
            stretch: Some(num(l, "sr", 1.0))
                .filter(|&sr| sr != 0.0)
                .unwrap_or(1.0),
            transform: transform(l.get("ks"))?,
            matte: l.get("td").and_then(int).unwrap_or(0) != 0,
            content,
        });
    }
    Ok(out)
}

fn shapes(v: Option<&Value>) -> Result<Vec<Shape>, String> {
    let mut out = Vec::new();
    for s in array(v) {
        let Some(s) = s.as_object() else { continue };
        if flag(s, "hd") {
            continue;
        }
        let p = |k: &str| {
            s.get(k)
                .map(prop)
                .transpose()
                .map(Option::unwrap_or_default)
        };
        let reversed = s.get("d").and_then(int) == Some(3);
        let paint = |k: &str| -> Result<Paint, String> {
            Ok(match s.get("g") {
                Some(g) => Paint::Gradient {
                    stops: g.get("p").and_then(int).unwrap_or(0).max(0) as usize,
                    colors: g.get("k").map(prop).transpose()?.unwrap_or_default(),
                },
                None => Paint::Solid(p(k)?),
            })
        };
        out.push(
            match s.get("ty").and_then(Value::as_str).unwrap_or_default() {
                "gr" => {
                    // The group's own transform, conventionally its last item.
                    let tr = (array(s.get("it")).iter())
                        .find(|i| i.get("ty").and_then(Value::as_str) == Some("tr"))
                        .map(|t| transform(Some(t)))
                        .transpose()?;
                    Shape::Group(shapes(s.get("it"))?, tr)
                }
                "sh" => Shape::Path(s.get("ks").map(prop).transpose()?.unwrap_or_default()),
                "rc" => Shape::Rect {
                    position: p("p")?,
                    size: p("s")?,
                    roundness: p("r")?,
                    reversed,
                },
                "el" => Shape::Ellipse {
                    position: p("p")?,
                    size: p("s")?,
                    reversed,
                },
                "sr" => Shape::Star {
                    star: s.get("sy").and_then(int) != Some(2),
                    points: p("pt")?,
                    position: p("p")?,
                    rotation: p("r")?,
                    outer: p("or")?,
                    inner: p("ir")?,
                    reversed,
                },
                "fl" | "gf" => Shape::Fill {
                    color: paint("c")?,
                    opacity: p("o")?,
                },
                "st" | "gs" => Shape::Stroke {
                    color: paint("c")?,
                    opacity: p("o")?,
                    width: p("w")?,
                },
                "tm" => Shape::Trim {
                    start: p("s")?,
                    end: p("e")?,
                    offset: p("o")?,
                    individually: s.get("m").and_then(int) == Some(2),
                },
                // Transforms are read with their group; modifiers we can't draw are skipped.
                _ => continue,
            },
        );
    }
    Ok(out)
}

fn transform(v: Option<&Value>) -> Result<Transform, String> {
    let Some(t) = v.and_then(Value::as_object) else {
        return Ok(Transform::default());
    };
    let p = |k: &str| {
        t.get(k)
            .map(prop)
            .transpose()
            .map(Option::unwrap_or_default)
    };
    let opt = |k: &str| t.get(k).map(prop).transpose();
    let position = match t.get("p") {
        Some(Value::Object(pos)) if flag(pos, "s") => Position::Split(
            pos.get("x").map(prop).transpose()?.unwrap_or_default(),
            pos.get("y").map(prop).transpose()?.unwrap_or_default(),
        ),
        _ => Position::Joint(p("p")?),
    };
    Ok(Transform {
        anchor: p("a")?,
        position,
        scale: opt("s")?,
        // 3D layers keep their 2D rotation in `rz`.
        rotation: if t.contains_key("r") {
            p("r")?
        } else {
            p("rz")?
        },
        opacity: opt("o")?,
        skew: p("sk")?,
        skew_axis: p("sa")?,
    })
}

/// Values that can be read from a property's JSON and blended between keyframes.
trait Value2: Sized + Clone + Default {
    fn from_json(v: &Value) -> Option<Self>;
    /// Blends towards `b` by `t`, one progress per dimension.
    fn blend(&self, b: &Self, t: &[f64]) -> Self;
    /// Moves `t` of the way to `b` along the curve that spatial tangents `tan` make, by arc
    /// length. Only positions have such curves.
    fn along(&self, b: &Self, tan: (Vec2, Vec2), t: f64) -> Self {
        let _ = tan;
        self.blend(b, &[t])
    }
}

impl Value2 for Vec<f64> {
    fn from_json(v: &Value) -> Option<Self> {
        match v {
            Value::Number(n) => Some(vec![n.as_f64()?]),
            Value::Array(a) => a.iter().map(Value::as_f64).collect(),
            _ => None,
        }
    }

    fn blend(&self, b: &Self, t: &[f64]) -> Self {
        (self.iter().zip(b).enumerate())
            .map(|(d, (x, y))| x + (y - x) * t[d.min(t.len() - 1)])
            .collect()
    }

    fn along(&self, b: &Self, (to, ti): (Vec2, Vec2), t: f64) -> Self {
        let mut out = self.blend(b, &[t]);
        if out.len() < 2 || (to == Vec2::ZERO && ti == Vec2::ZERO) {
            return out;
        }
        let (p0, p3) = (Point::new(self[0], self[1]), Point::new(b[0], b[1]));
        let c = CubicBez::new(p0, p0 + to, p3 + ti, p3);
        let p = c.eval(c.inv_arclen(c.arclen(1e-3) * t.clamp(0.0, 1.0), 1e-3));
        (out[0], out[1]) = (p.x, p.y);
        out
    }
}

impl Value2 for Bez {
    fn from_json(v: &Value) -> Option<Self> {
        // Keyframed shapes wrap the path in a one-element array.
        let v = match v {
            Value::Array(a) => a.first()?,
            v => v,
        };
        let pts = |k: &str| -> Option<Vec<[f64; 2]>> {
            (v.get(k)?.as_array()?.iter())
                .map(|p| Some([p.get(0)?.as_f64()?, p.get(1)?.as_f64()?]))
                .collect()
        };
        let (vs, is, os) = (pts("v")?, pts("i")?, pts("o")?);
        if is.len() != vs.len() || os.len() != vs.len() {
            return None;
        }
        Some(Bez {
            closed: v.get("c").and_then(Value::as_bool).unwrap_or(false),
            v: vs.into_iter().map(|[x, y]| Point::new(x, y)).collect(),
            i: is.into_iter().map(|[x, y]| Vec2::new(x, y)).collect(),
            o: os.into_iter().map(|[x, y]| Vec2::new(x, y)).collect(),
        })
    }

    fn blend(&self, b: &Self, t: &[f64]) -> Self {
        let t = t[0];
        if self.v.len() != b.v.len() {
            return if t < 1.0 { self.clone() } else { b.clone() };
        }
        let lerp = |x: &[Vec2], y: &[Vec2]| x.iter().zip(y).map(|(p, q)| p.lerp(*q, t)).collect();
        Bez {
            closed: self.closed,
            v: (self.v.iter().zip(&b.v))
                .map(|(p, q)| p.lerp(*q, t))
                .collect(),
            i: lerp(&self.i, &b.i),
            o: lerp(&self.o, &b.o),
        }
    }
}

fn prop<T: Value2>(v: &Value) -> Result<Prop<T>, String> {
    let bad = || format!("unreadable Lottie property {v}");
    let k = v.get("k").ok_or_else(bad)?;
    let keys = match k {
        Value::Array(ks) if ks.first().is_some_and(|f| f.get("t").is_some()) => ks,
        _ => return T::from_json(k).map(Prop::Static).ok_or_else(bad),
    };
    let mut out: Vec<Key<T>> = Vec::new();
    for key in keys {
        let read = |k: &str| key.get(k).and_then(T::from_json);
        let tangent = |k: &str| {
            let v = key.get(k)?;
            Some(Vec2::new(v.get(0)?.as_f64()?, v.get(1)?.as_f64()?))
        };
        // The old format's last key often only has a time: it holds the previous end.
        let start = match read("s") {
            Some(s) => s,
            None => match out.last() {
                Some(prev) => prev.end.clone().unwrap_or_else(|| prev.start.clone()),
                None => return Err(bad()),
            },
        };
        out.push(Key {
            t: key.get("t").and_then(Value::as_f64).ok_or_else(bad)?,
            start,
            end: read("e"),
            hold: key.get("h").and_then(int).unwrap_or(0) == 1,
            ease_out: ease(key.get("o")),
            ease_in: ease(key.get("i")),
            tangents: tangent("to").zip(tangent("ti")),
        });
    }
    Ok(Prop::Animated(out))
}

fn ease(v: Option<&Value>) -> Ease {
    let dims = |k: &str| match v.and_then(|v| v.get(k)) {
        Some(Value::Number(n)) => n.as_f64().into_iter().collect(),
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_f64).collect(),
        _ => Vec::new(),
    };
    Ease {
        x: dims("x"),
        y: dims("y"),
    }
}

/// An array's items; none if it's missing or not an array.
fn array(v: Option<&Value>) -> &[Value] {
    v.and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// An integer, however it's written (`1` or `1.0`).
fn int(v: &Value) -> Option<i64> {
    v.as_f64().map(|n| n.round() as i64)
}

fn flag(o: &Map<String, Value>, k: &str) -> bool {
    match o.get(k) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        _ => false,
    }
}

fn num(o: &Map<String, Value>, k: &str, default: f64) -> f64 {
    o.get(k).and_then(Value::as_f64).unwrap_or(default)
}

fn hex_color(s: &str) -> Color {
    let rgb = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0);
    Color::hex(rgb & 0xff_ffff)
}

// Evaluation.

impl<T: Value2> Prop<T> {
    fn at(&self, f: f64) -> T {
        let keys = match self {
            Prop::Static(v) => return v.clone(),
            Prop::Animated(keys) => keys,
        };
        let Some(first) = keys.first() else {
            return T::default();
        };
        if f <= first.t {
            return first.start.clone();
        }
        let i = keys.iter().rposition(|k| k.t <= f).unwrap_or(0);
        let k = &keys[i];
        let Some(next) = keys.get(i + 1) else {
            return k.start.clone();
        };
        let end = k.end.as_ref().unwrap_or(&next.start);
        if k.hold || next.t <= k.t {
            return k.start.clone();
        }
        let u = (f - k.t) / (next.t - k.t);
        let dims = k.ease_out.x.len().max(k.ease_in.x.len()).max(1);
        let progress: Vec<f64> = (0..dims)
            .map(|d| {
                let at = |v: &[f64], or: f64| v.get(d).or(v.last()).copied().unwrap_or(or);
                let (x1, y1) = (at(&k.ease_out.x, 0.0), at(&k.ease_out.y, 0.0));
                let (x2, y2) = (at(&k.ease_in.x, 1.0), at(&k.ease_in.y, 1.0));
                cubic_ease(x1, y1, x2, y2, u)
            })
            .collect();
        match &k.tangents {
            Some(tan) => k.start.along(end, *tan, progress[0]),
            None => k.start.blend(end, &progress),
        }
    }
}

/// CSS-style cubic-Bézier easing through `(x1, y1)` and `(x2, y2)`, at `x`.
fn cubic_ease(x1: f64, y1: f64, x2: f64, y2: f64, x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    let bez = |a: f64, b: f64, t: f64| {
        let s = 1.0 - t;
        3.0 * s * s * t * a + 3.0 * s * t * t * b + t * t * t
    };
    // Bisection: the x curve is monotonic for handles in 0..=1, and this never diverges.
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if bez(x1.clamp(0.0, 1.0), x2.clamp(0.0, 1.0), mid) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bez(y1, y2, (lo + hi) / 2.0)
}

fn scalar(p: &Prop<Vec<f64>>, f: f64, default: f64) -> f64 {
    p.at(f).first().copied().unwrap_or(default)
}

fn point(p: &Prop<Vec<f64>>, f: f64) -> Point {
    let v = p.at(f);
    Point::new(
        v.first().copied().unwrap_or(0.0),
        v.get(1).copied().unwrap_or(0.0),
    )
}

impl Transform {
    fn matrix(&self, f: f64) -> Affine {
        let anchor = point(&self.anchor, f);
        let position = match &self.position {
            Position::Joint(p) => point(p, f),
            Position::Split(x, y) => Point::new(scalar(x, f, 0.0), scalar(y, f, 0.0)),
        };
        let scale = match &self.scale {
            Some(s) => {
                let s = s.at(f);
                let x = s.first().copied().unwrap_or(100.0);
                Vec2::new(x, s.get(1).copied().unwrap_or(x)) / 100.0
            }
            None => Vec2::new(1.0, 1.0),
        };
        let skew = scalar(&self.skew, f, 0.0).to_radians();
        let skew = if skew == 0.0 {
            Affine::IDENTITY
        } else {
            let axis = scalar(&self.skew_axis, f, 0.0).to_radians();
            Affine::rotate(axis) * Affine::skew((-skew).tan(), 0.0) * Affine::rotate(-axis)
        };
        Affine::translate(position.to_vec2())
            * Affine::rotate(scalar(&self.rotation, f, 0.0).to_radians())
            * skew
            * Affine::scale_non_uniform(scale.x, scale.y)
            * Affine::translate(-anchor.to_vec2())
    }

    fn opacity(&self, f: f64) -> f64 {
        (self.opacity.as_ref()).map_or(1.0, |o| scalar(o, f, 100.0) / 100.0)
    }
}

/// Appends every layer's draws, top first. `visible: false` appends empty ones, keeping the
/// count fixed.
fn eval_layers(
    cx: Eval,
    layers: &[Layer],
    f: f64,
    place: Affine,
    opacity: f64,
    visible: bool,
    out: &mut Vec<VState>,
) {
    let mut matrices = HashMap::new();
    for layer in layers {
        if layer.matte {
            continue;
        }
        let local = (f - layer.start) / layer.stretch;
        let shown = visible && (layer.in_point..layer.out_point).contains(&f);
        let m = place * layer_matrix(layers, layer, f, &mut matrices, 0);
        let opacity = opacity * layer.transform.opacity(local);
        match &layer.content {
            Content::None => {}
            Content::Shapes(shapes) => eval_shapes(shapes, local, m, opacity, shown, out),
            Content::Solid {
                width,
                height,
                color,
            } => {
                let corners = [(0.0, 0.0), (*width, 0.0), (*width, *height), (0.0, *height)];
                let path = VPath::polyline(&corners.map(|p| m * Point::from(p)), true);
                out.push(draw(path, Paint2::Fill(*color), opacity, shown));
            }
            Content::Precomp { layers, remap } => {
                let f = match remap {
                    Some(r) => scalar(r, local, 0.0) * cx.fps,
                    None => local,
                };
                eval_layers(cx, layers, f, m, opacity, shown, out);
            }
        }
    }
}

/// What every layer's evaluation needs from the file.
#[derive(Clone, Copy)]
struct Eval {
    /// The frame rate time remaps' seconds convert at.
    fps: f64,
}

/// A layer's transform with its parents', from its pixels to the composition's.
fn layer_matrix(
    layers: &[Layer],
    layer: &Layer,
    f: f64,
    memo: &mut HashMap<i64, Affine>,
    depth: usize,
) -> Affine {
    let own = layer.transform.matrix((f - layer.start) / layer.stretch);
    let parent = layer.parent.filter(|_| depth < MAX_NESTING).and_then(|p| {
        if let Some(m) = memo.get(&p) {
            return Some(*m);
        }
        let pl = layers.iter().find(|l| l.index == Some(p))?;
        let m = layer_matrix(layers, pl, f, memo, depth + 1);
        memo.insert(p, m);
        Some(m)
    });
    parent.unwrap_or(Affine::IDENTITY) * own
}

enum Paint2 {
    Fill(Color),
    Stroke(Color, f64),
}

fn draw(path: VPath, paint: Paint2, opacity: f64, shown: bool) -> VState {
    if !shown {
        return VState {
            opacity: 0.0,
            ..VState::new(VPath::default()).stroke(Color::TRANSPARENT, 0.0)
        };
    }
    let m = VState::new(path);
    let m = match paint {
        Paint2::Fill(c) => m.fill(c).stroke(Color::TRANSPARENT, 0.0),
        Paint2::Stroke(c, w) => VState {
            stroke: Stroke::new(c, w),
            ..m
        },
    };
    VState {
        opacity: opacity.clamp(0.0, 1.0) as f32,
        ..m
    }
}

/// A fill or stroke found while walking a layer's shapes, drawn once the layer's trims have
/// all been applied.
struct Pending {
    /// Indices of the paths it draws.
    paths: std::ops::Range<usize>,
    paint: Paint2,
    opacity: f64,
}

fn eval_shapes(
    shapes: &[Shape],
    f: f64,
    m: Affine,
    opacity: f64,
    shown: bool,
    out: &mut Vec<VState>,
) {
    let mut paths = Vec::new();
    let mut pending = Vec::new();
    if shown {
        walk(
            shapes,
            f,
            Affine::IDENTITY,
            opacity,
            &mut paths,
            &mut pending,
        );
    } else {
        count_styles(shapes, &mut pending);
    }
    let scale = m.determinant().abs().sqrt();
    for p in pending {
        let path = VPath {
            subpaths: (paths[p.paths].iter())
                .flat_map(|(path, xf): &(VPath, Affine)| path.transform(m * *xf).subpaths)
                .collect(),
        };
        let paint = match p.paint {
            Paint2::Stroke(c, w) => Paint2::Stroke(c, w * scale),
            fill => fill,
        };
        out.push(draw(path, paint, p.opacity, shown));
    }
}

/// Placeholders for the styles in `shapes`, for a layer that isn't showing.
fn count_styles(shapes: &[Shape], pending: &mut Vec<Pending>) {
    for s in shapes {
        match s {
            Shape::Group(items, _) => count_styles(items, pending),
            Shape::Fill { .. } | Shape::Stroke { .. } => pending.push(Pending {
                paths: 0..0,
                paint: Paint2::Fill(Color::TRANSPARENT),
                opacity: 0.0,
            }),
            _ => {}
        }
    }
}

/// Walks a shape list as Lottie does: paths collect until a fill or stroke draws every one
/// so far in its group (nested groups included), and trims cut every one so far. Paths are
/// kept in their own pixels with their group's transform, `xf`.
fn walk(
    shapes: &[Shape],
    f: f64,
    xf: Affine,
    opacity: f64,
    paths: &mut Vec<(VPath, Affine)>,
    pending: &mut Vec<Pending>,
) {
    let first = paths.len();
    for s in shapes {
        match s {
            Shape::Group(items, tr) => {
                let (xf, opacity) = match tr {
                    Some(t) => (xf * t.matrix(f), opacity * t.opacity(f)),
                    None => (xf, opacity),
                };
                walk(items, f, xf, opacity, paths, pending);
            }
            Shape::Path(p) => paths.push((bez_path(&p.at(f)), xf)),
            Shape::Rect {
                position,
                size,
                roundness,
                reversed,
            } => {
                let path = rect(
                    point(position, f),
                    point(size, f),
                    scalar(roundness, f, 0.0),
                );
                paths.push((reverse_if(path, *reversed), xf));
            }
            Shape::Ellipse {
                position,
                size,
                reversed,
            } => {
                let path = ellipse(point(position, f), point(size, f).to_vec2() / 2.0);
                paths.push((reverse_if(path, *reversed), xf));
            }
            Shape::Star {
                star,
                points,
                position,
                rotation,
                outer,
                inner,
                reversed,
            } => {
                let path = polystar(
                    *star,
                    scalar(points, f, 5.0).round().max(3.0) as usize,
                    point(position, f),
                    scalar(rotation, f, 0.0),
                    scalar(outer, f, 0.0),
                    scalar(inner, f, 0.0),
                );
                paths.push((reverse_if(path, *reversed), xf));
            }
            Shape::Fill { color, opacity: o } => pending.push(Pending {
                paths: first..paths.len(),
                paint: Paint2::Fill(paint_color(color, f)),
                opacity: opacity * scalar(o, f, 100.0) / 100.0,
            }),
            Shape::Stroke {
                color,
                opacity: o,
                width,
            } => pending.push(Pending {
                paths: first..paths.len(),
                paint: Paint2::Stroke(
                    paint_color(color, f),
                    scalar(width, f, 1.0) * xf.determinant().abs().sqrt(),
                ),
                opacity: opacity * scalar(o, f, 100.0) / 100.0,
            }),
            Shape::Trim {
                start,
                end,
                offset,
                individually,
            } => {
                let (s, e) = (scalar(start, f, 0.0) / 100.0, scalar(end, f, 100.0) / 100.0);
                let o = scalar(offset, f, 0.0) / 360.0;
                trim(&mut paths[first..], s, e, o, *individually);
            }
        }
    }
}

fn paint_color(p: &Paint, f: f64) -> Color {
    let c = match p {
        Paint::Solid(c) => c.at(f),
        Paint::Gradient { stops, colors } => {
            // `[offset, r, g, b]` per stop, then any opacity stops.
            let v = colors.at(f);
            let n = (*stops).max(1).min(v.len() / 4).max(1);
            let mut sum = [0.0; 3];
            for stop in v.chunks(4).take(n) {
                for (s, c) in sum.iter_mut().zip(stop.iter().skip(1)) {
                    *s += c / n as f64;
                }
            }
            sum.to_vec()
        }
    };
    // Very old files give channels in 0..=255.
    let k = if c.iter().take(3).any(|&v| v > 1.0) {
        255.0
    } else {
        1.0
    };
    let ch = |i: usize| (c.get(i).copied().unwrap_or(0.0) / k).clamp(0.0, 1.0) as f32;
    Color::rgb(ch(0), ch(1), ch(2))
}

fn bez_path(b: &Bez) -> VPath {
    let n = b.v.len();
    if n == 0 {
        return VPath::default();
    }
    let seg = |a: usize, c: usize| CubicBez::new(b.v[a], b.v[a] + b.o[a], b.v[c] + b.i[c], b.v[c]);
    let mut segments: Vec<_> = (0..n - 1).map(|k| seg(k, k + 1)).collect();
    if b.closed {
        segments.push(seg(n - 1, 0));
    }
    if segments.is_empty() {
        // A lone vertex: a zero-length segment, so it still has a start for trims.
        segments.push(line_segment(b.v[0], b.v[0]));
    }
    VPath {
        subpaths: vec![SubPath {
            segments,
            closed: b.closed,
        }],
    }
}

/// A rectangle centered on `c`, clockwise (on screen) from its top-right corner, as Lottie
/// draws it.
fn rect(c: Point, size: Point, radius: f64) -> VPath {
    let (w, h) = (size.x / 2.0, size.y / 2.0);
    let r = radius.clamp(0.0, w.abs().min(h.abs()));
    let p = |x: f64, y: f64| c + Vec2::new(x, y);
    if r <= 0.0 {
        return VPath::polyline(&[p(w, -h), p(w, h), p(-w, h), p(-w, -h)], true);
    }
    // Each corner is a quarter ellipse; KAPPA puts its handles where a circle's go.
    let k = r * (1.0 - KAPPA);
    let corner = |a: Point, b: Point, ha: Point, hb: Point| CubicBez::new(a, ha, hb, b);
    let segments = vec![
        line_segment(p(w, -h + r), p(w, h - r)),
        corner(p(w, h - r), p(w - r, h), p(w, h - k), p(w - k, h)),
        line_segment(p(w - r, h), p(-w + r, h)),
        corner(p(-w + r, h), p(-w, h - r), p(-w + k, h), p(-w, h - k)),
        line_segment(p(-w, h - r), p(-w, -h + r)),
        corner(p(-w, -h + r), p(-w + r, -h), p(-w, -h + k), p(-w + k, -h)),
        line_segment(p(-w + r, -h), p(w - r, -h)),
        corner(p(w - r, -h), p(w, -h + r), p(w - k, -h), p(w, -h + k)),
    ];
    VPath {
        subpaths: vec![SubPath {
            segments,
            closed: true,
        }],
    }
}

/// How far along the tangent a cubic's handle sits to draw a quarter circle.
const KAPPA: f64 = 0.552_284_749_8;

/// An ellipse centered on `c`, clockwise (on screen) from its top, as Lottie draws it.
fn ellipse(c: Point, r: Vec2) -> VPath {
    let (kx, ky) = (r.x * KAPPA, r.y * KAPPA);
    let p = |x: f64, y: f64| c + Vec2::new(x, y);
    let (top, right, bottom, left) = (p(0.0, -r.y), p(r.x, 0.0), p(0.0, r.y), p(-r.x, 0.0));
    let segments = vec![
        CubicBez::new(top, p(kx, -r.y), p(r.x, -ky), right),
        CubicBez::new(right, p(r.x, ky), p(kx, r.y), bottom),
        CubicBez::new(bottom, p(-kx, r.y), p(-r.x, ky), left),
        CubicBez::new(left, p(-r.x, -ky), p(-kx, -r.y), top),
    ];
    VPath {
        subpaths: vec![SubPath {
            segments,
            closed: true,
        }],
    }
}

/// A star (alternating `outer` and `inner` radii) or regular polygon of `n` points, the first
/// straight up from `c` and turned `rotation` degrees clockwise. Roundness is ignored.
fn polystar(star: bool, n: usize, c: Point, rotation: f64, outer: f64, inner: f64) -> VPath {
    let corners = if star { 2 * n } else { n };
    let step = std::f64::consts::TAU / corners as f64;
    let start = (rotation - 90.0).to_radians();
    let pts: Vec<Point> = (0..corners)
        .map(|i| {
            let r = if star && i % 2 == 1 { inner } else { outer };
            let a = start + step * i as f64;
            c + Vec2::new(a.cos(), a.sin()) * r
        })
        .collect();
    VPath::polyline(&pts, true)
}

fn reverse_if(path: VPath, reversed: bool) -> VPath {
    if !reversed {
        return path;
    }
    let subpaths = (path.subpaths.into_iter())
        .map(|sp| SubPath {
            segments: (sp.segments.iter().rev())
                .map(|s| CubicBez::new(s.p3, s.p2, s.p1, s.p0))
                .collect(),
            closed: sp.closed,
        })
        .collect();
    VPath { subpaths }
}

/// Lottie's trim paths: keeps fractions `start..end` of each path (or of all of them end to
/// end, `individually`), turned by `offset` of a whole turn.
fn trim(paths: &mut [(VPath, Affine)], start: f64, end: f64, offset: f64, individually: bool) {
    let (mut s, mut e) = (start.min(end) + offset, start.max(end) + offset);
    if e - s >= 1.0 {
        return;
    }
    if e <= s {
        paths.iter_mut().for_each(|(p, _)| *p = VPath::default());
        return;
    }
    let shift = s.floor();
    s -= shift;
    e -= shift;
    // The kept spans, as fractions of the whole: one, or two when it wraps past the end.
    let spans: Vec<(f64, f64)> = if e > 1.0 {
        vec![(0.0, e - 1.0), (s, 1.0)]
    } else {
        vec![(s, e)]
    };
    let keep = |path: &VPath, from: f64, len: f64, total: f64| -> VPath {
        let mut subpaths = Vec::new();
        for &(a, b) in &spans {
            let (a, b) = ((a * total - from) / len, (b * total - from) / len);
            if b <= 0.0 || a >= 1.0 || len <= 0.0 {
                continue;
            }
            subpaths.extend(path.trim(a.max(0.0) as f32..b.min(1.0) as f32).subpaths);
        }
        VPath { subpaths }
    };
    if individually {
        let lens: Vec<f64> = paths.iter().map(|(p, xf)| length(p, *xf)).collect();
        let total: f64 = lens.iter().sum();
        let mut from = 0.0;
        for ((p, _), len) in paths.iter_mut().zip(lens) {
            *p = keep(p, from, len, total);
            from += len;
        }
    } else {
        for (p, _) in paths.iter_mut() {
            *p = keep(p, 0.0, 1.0, 1.0);
        }
    }
}

/// Arc length as drawn, under `xf`.
fn length(p: &VPath, xf: Affine) -> f64 {
    (p.subpaths.iter())
        .flat_map(|sp| &sp.segments)
        .map(|s| (xf * *s).arclen(1e-3))
        .sum()
}
