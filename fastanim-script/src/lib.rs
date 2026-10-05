//! Rhai scenes for fastanim (see `docs/SPEC.md` §14): the scene API as a script. A script runs
//! once, top to bottom, recording into a [`Scene`]; [`bake`] returns the same
//! [`BakedTimeline`] a Rust scene would, so playback and export never run the script again.
//!
//! ```
//! let tl = fastanim_script::bake(r#"
//!     let c = scene.add(circle(1.0).stroke(BLUE, 0.06));
//!     scene.play(create(c).run_time(2.0));
//!     scene.marker("drawn");
//!     scene.always(c, |state, t| state[c].move_to(point(cos(t), sin(t))));
//!     scene.wait(1.0);
//! "#).unwrap();
//! assert_eq!(tl.duration(), 3.0);
//! assert_eq!(tl.marker("drawn"), Some(2.0));
//! ```
//!
//! Scripts see a `scene` variable with `add`, `remove`, `play`, `wait`, `marker`, `always`
//! and `get`; shape, text and animation functions named as in Rust (`circle`, `math_tex`,
//! `transform_diff`, ...); `point(x, y)` for points and vectors; and the constants `UP`,
//! `DOWN`, `LEFT`, `RIGHT`, `ORIGIN`, the colors, `LINEAR`, `SMOOTH`, `THERE_AND_BACK`,
//! `DEFAULT_BUFF`, `FRAME_WIDTH`, `FRAME_HEIGHT`, `PI` and `TAU`. Numbers passed as floats
//! must be written as floats (`1.0`, not `1`).

use std::cell::RefCell;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use fastanim_core::color::{BLACK, BLUE, GREEN, GREY, ORANGE, RED, WHITE, YELLOW};
use fastanim_core::kurbo::{Affine, Point, Vec2};
use fastanim_core::{
    Animation, AnimationExt, Axes, BakedTimeline, Color, DEFAULT_BUFF, DOWN, DiffStyle, Ease,
    FRAME_HEIGHT, FRAME_WIDTH, Group, LEFT, MobjectId, Parallel, Position, RIGHT, RateFn, Scene,
    SceneState, Sequence, UP, Update, UpdateGroup, VState,
};
use fastanim_text::{TextMobject, TokenKey};
use rhai::{
    AST, Array, Dynamic, Engine, EvalAltResult, FnPtr, FuncArgs, Map, NativeCallContext,
    ParseError, Scope,
};

type Res<T> = Result<T, Box<EvalAltResult>>;

/// A script error, at a 1-based line and column of the script (0 when unknown).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// 1-based line, or 0.
    pub line: usize,
    /// 1-based column, or 0.
    pub col: usize,
    /// What went wrong.
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 {
            write!(f, "{}:{}: ", self.line, self.col)?;
        }
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<ParseError> for Error {
    fn from(e: ParseError) -> Self {
        Self {
            line: e.1.line().unwrap_or(0),
            col: e.1.position().unwrap_or(0),
            message: e.0.to_string(),
        }
    }
}

impl From<Box<EvalAltResult>> for Error {
    fn from(mut e: Box<EvalAltResult>) -> Self {
        let pos = e.take_position();
        Self {
            line: pos.line().unwrap_or(0),
            col: pos.position().unwrap_or(0),
            message: e.to_string(),
        }
    }
}

/// Runs a scene script and bakes what it recorded.
pub fn bake(src: &str) -> Result<BakedTimeline, Error> {
    let engine = engine();
    let ast = Arc::new(engine.compile(src)?);
    let engine = Arc::new(engine);
    let rt = Rt {
        engine: engine.clone(),
        ast: ast.clone(),
        err: Arc::default(),
    };
    let handle = SceneHandle(Arc::new(Mutex::new(Ctx {
        scene: Scene::new(),
        texts: Vec::new(),
        rt,
    })));
    let mut scope = Scope::new();
    scope.push("scene", handle.clone());
    engine.run_ast_with_scope(&mut scope, &ast)?;
    // Taken rather than unwrapped: closures in the timeline may still hold `scene`.
    let scene = std::mem::take(&mut handle.lock()?.scene);
    Ok(scene.bake())
}

/// What closures need after the script has finished: the engine, the script, and a slot for
/// the first error, since animations can't fail.
#[derive(Clone)]
struct Rt {
    engine: Arc<Engine>,
    ast: Arc<AST>,
    err: Arc<Mutex<Option<String>>>,
}

impl Rt {
    /// Calls `f`, or records the error and returns `None`.
    fn call<T: Clone + Send + Sync + 'static>(&self, f: &FnPtr, args: impl FuncArgs) -> Option<T> {
        let out = f
            .call::<Dynamic>(&self.engine, &self.ast, args)
            .and_then(|d| {
                let ty = d.type_name();
                (d.try_cast::<T>()).ok_or_else(|| format!("closure returned {ty}").into())
            });
        out.map_err(|e| {
            self.err
                .lock()
                .unwrap()
                .get_or_insert(e.to_string())
                .clone()
        })
        .ok()
    }
}

/// The scene being recorded, plus the text groups that scripts hold by index.
struct Ctx {
    scene: Scene,
    texts: Vec<Group<TokenKey>>,
    rt: Rt,
}

/// A mobject added to the scene: a shape, or a text's glyphs.
#[derive(Debug, Clone, Copy)]
enum Mob {
    Shape(MobjectId),
    Text(usize),
}

impl Ctx {
    /// The ids of `m`, all of which must still be in the scene.
    fn ids(&self, m: Mob) -> Result<Vec<MobjectId>, String> {
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

    fn shape(&self, m: Mob) -> Result<MobjectId, String> {
        match m {
            Mob::Shape(_) => Ok(self.ids(m)?[0]),
            Mob::Text(_) => Err("expected a shape, got a text".into()),
        }
    }
}

/// The `scene` variable.
#[derive(Clone)]
struct SceneHandle(Arc<Mutex<Ctx>>);

impl SceneHandle {
    fn lock(&self) -> Res<MutexGuard<'_, Ctx>> {
        // Fails rather than deadlocks when a closure run by `play` uses `scene`.
        (self.0.try_lock()).map_err(|_| "`scene` can't be used inside a closure".into())
    }
}

/// The scene as an updater sees it; index it with a shape.
#[derive(Clone)]
struct State(SceneState);

fn get(state: &SceneState, m: Mob) -> Res<VState> {
    match m {
        Mob::Shape(id) => (state.get(&id).cloned()).ok_or_else(|| "not in the scene".into()),
        Mob::Text(_) => Err("only shapes can be read back; texts are many glyphs".into()),
    }
}

type Build = dyn Fn(&mut Ctx) -> Result<Box<dyn Animation>, String> + Send + Sync;

/// An animation as scripts see it: built against the scene when played, since some (like
/// `circumscribe` and `transform_diff`) add to the scene.
#[derive(Clone)]
struct Anim {
    build: Arc<Build>,
    run_time: Option<f32>,
    rate: Option<RateFn>,
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

fn anims(a: Array) -> Res<Vec<Anim>> {
    (a.into_iter())
        .map(|d| {
            let ty = d.type_name();
            d.try_cast::<Anim>()
                .ok_or_else(|| format!("expected an animation, got {ty}").into())
        })
        .collect()
}

fn build_all(anims: &[Anim], cx: &mut Ctx) -> Result<Vec<Box<dyn Animation>>, String> {
    anims.iter().map(|a| a.build(cx)).collect()
}

fn ease(name: &str) -> Res<Ease> {
    Ok(match name {
        "quad" => Ease::Quad,
        "cubic" => Ease::Cubic,
        "expo" => Ease::Expo,
        "back" => Ease::Back,
        _ => return Err(format!("unknown ease `{name}`: quad, cubic, expo or back").into()),
    })
}

fn diff_style(opts: Map) -> Res<DiffStyle> {
    let mut style = DiffStyle::default();
    for (k, v) in opts {
        let ty = v.type_name();
        let bad = || format!("`{k}` can't be {ty}");
        match k.as_str() {
            "highlight_changes" => style.highlight_changes = v.as_bool().map_err(|_| bad())?,
            "debug" => style.debug = v.as_bool().map_err(|_| bad())?,
            "lag_ratio" => style.lag_ratio = v.as_float().map_err(|_| bad())? as f32,
            "move_arc" => style.move_arc = v.as_float().map_err(|_| bad())?,
            _ => {
                return Err(format!(
                    "unknown option `{k}`: highlight_changes, debug, lag_ratio or move_arc"
                )
                .into());
            }
        }
    }
    Ok(style)
}

fn points(a: Array) -> Res<Vec<Point>> {
    (a.into_iter())
        .map(|d| {
            let ty = d.type_name();
            (d.try_cast::<Vec2>().map(Vec2::to_point))
                .ok_or_else(|| format!("expected a point, got {ty}").into())
        })
        .collect()
}

/// `[min, max, step]`, or `[min, max]` with step 1; ints allowed.
fn range(a: Array) -> Res<[f64; 3]> {
    let n = (a.iter())
        .map(|d| d.as_float().or_else(|_| d.as_int().map(|i| i as f64)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|ty| format!("range values must be numbers, got {ty}"))?;
    match n[..] {
        [lo, hi, step] if lo < hi && step > 0.0 => Ok([lo, hi, step]),
        [lo, hi] if lo < hi => Ok([lo, hi, 1.0]),
        _ => Err("expected [min, max] or [min, max, step], min < max, step > 0".into()),
    }
}

/// `build` with script closure `f` as the function; the first error `f` raises fails it.
fn graph(
    cx: &NativeCallContext,
    f: &FnPtr,
    build: impl FnOnce(&dyn Fn(f64) -> f64) -> VState,
) -> Res<VState> {
    let err = RefCell::new(None);
    let m = build(&|x| {
        f.call_within_context(cx, (x,)).unwrap_or_else(|e| {
            err.borrow_mut().get_or_insert(e);
            0.0
        })
    });
    err.into_inner().map_or(Ok(m), Err)
}

fn constant(name: &str) -> Option<Dynamic> {
    let v = |v: Vec2| Some(Dynamic::from(v));
    let c = |c: Color| Some(Dynamic::from(c));
    let r = |r: RateFn| Some(Dynamic::from(r));
    match name {
        "UP" => v(UP),
        "DOWN" => v(DOWN),
        "LEFT" => v(LEFT),
        "RIGHT" => v(RIGHT),
        "ORIGIN" => v(Vec2::ZERO),
        "WHITE" => c(WHITE),
        "BLACK" => c(BLACK),
        "BLUE" => c(BLUE),
        "RED" => c(RED),
        "GREEN" => c(GREEN),
        "YELLOW" => c(YELLOW),
        "GREY" => c(GREY),
        "ORANGE" => c(ORANGE),
        "TRANSPARENT" => c(Color::TRANSPARENT),
        "LINEAR" => r(RateFn::Linear),
        "SMOOTH" => r(RateFn::Smooth),
        "THERE_AND_BACK" => r(RateFn::ThereAndBack),
        "DEFAULT_BUFF" => Some(DEFAULT_BUFF.into()),
        "FRAME_WIDTH" => Some(FRAME_WIDTH.into()),
        "FRAME_HEIGHT" => Some(FRAME_HEIGHT.into()),
        "PI" => Some(std::f64::consts::PI.into()),
        "TAU" => Some(std::f64::consts::TAU.into()),
        _ => None,
    }
}

/// Methods shared by shapes and texts.
macro_rules! positional {
    ($e:expr, $t:ty) => {
        $e.register_fn("shift", |m: $t, v: Vec2| m.shift(v))
            .register_fn("move_to", |m: $t, p: Vec2| m.move_to(p.to_point()))
            .register_fn("scale", |m: $t, k: f64| m.scale(k))
            .register_fn("to_edge", |m: $t, d: Vec2| m.to_edge(d))
            .register_fn("next_to", |m: $t, o: VState, d: Vec2| {
                m.next_to(&o, d, DEFAULT_BUFF)
            })
            .register_fn("next_to", |m: $t, o: TextMobject, d: Vec2| {
                m.next_to(&o, d, DEFAULT_BUFF)
            })
            .register_fn("next_to", |m: $t, p: Vec2, d: Vec2| {
                m.next_to(&p.to_point(), d, DEFAULT_BUFF)
            })
            .register_fn("next_to", |m: $t, o: VState, d: Vec2, b: f64| {
                m.next_to(&o, d, b)
            })
            .register_fn("next_to", |m: $t, o: TextMobject, d: Vec2, b: f64| {
                m.next_to(&o, d, b)
            })
            .register_fn("align_to", |m: $t, o: VState, d: Vec2| m.align_to(&o, d))
            .register_fn("align_to", |m: $t, o: TextMobject, d: Vec2| {
                m.align_to(&o, d)
            })
    };
}

/// The engine with the whole scene API registered, and limits so a runaway script fails
/// instead of hanging (SPEC §14.2).
fn engine() -> Engine {
    let mut e = Engine::new();
    // `on_var` is marked deprecated only as "volatile"; constants this way are visible inside
    // functions and closures too, unlike scope constants.
    #[allow(deprecated)]
    e.set_max_operations(10_000_000)
        .set_max_call_levels(64)
        .set_max_string_size(1 << 20)
        .set_max_array_size(1 << 20)
        .set_max_map_size(1 << 10)
        .on_var(|name, _, _| Ok(constant(name)));

    // Points and colors.
    e.register_type_with_name::<Vec2>("Point")
        .register_fn("point", Vec2::new)
        .register_get("x", |v: &mut Vec2| v.x)
        .register_get("y", |v: &mut Vec2| v.y)
        .register_fn("+", |a: Vec2, b: Vec2| a + b)
        .register_fn("-", |a: Vec2, b: Vec2| a - b)
        .register_fn("-", |a: Vec2| -a)
        .register_fn("*", |a: Vec2, k: f64| a * k)
        .register_fn("*", |k: f64, a: Vec2| a * k)
        .register_fn("/", |a: Vec2, k: f64| a / k)
        .register_fn("to_string", |v: &mut Vec2| {
            format!("point({}, {})", v.x, v.y)
        })
        .register_fn("to_debug", |v: &mut Vec2| {
            format!("point({}, {})", v.x, v.y)
        });
    e.register_type_with_name::<Color>("Color")
        .register_fn("rgb", |r: f64, g: f64, b: f64| {
            Color::rgb(r as f32, g as f32, b as f32)
        })
        .register_fn("hex", |rgb: i64| Color::hex(rgb as u32))
        .register_fn("with_alpha", |c: Color, a: f64| c.with_alpha(a as f32));

    // Shapes.
    e.register_type_with_name::<VState>("Shape")
        .register_fn("circle", VState::circle)
        .register_fn("square", VState::square)
        .register_fn("rectangle", VState::rectangle)
        .register_fn("arc", VState::arc)
        .register_fn("line", |a: Vec2, b: Vec2| {
            VState::line(a.to_point(), b.to_point())
        })
        .register_fn("dot", |p: Vec2| VState::dot(p.to_point()))
        .register_fn("polygon", |a: Array| {
            Ok(VState::polygon(&points(a)?)) as Res<_>
        })
        .register_fn(
            "function_graph",
            |cx: NativeCallContext, f: FnPtr, x0: f64, x1: f64, segments: i64| -> Res<_> {
                if segments < 1 {
                    return Err("segments must be at least 1".into());
                }
                graph(&cx, &f, |f| {
                    VState::function_graph(f, x0..x1, segments as usize)
                })
            },
        )
        .register_fn("fill", |m: VState, c: Color| m.fill(c))
        .register_fn("stroke", |m: VState, c: Color, w: f64| m.stroke(c, w))
        .register_fn("z_index", |m: VState, z: i64| m.z_index(z as i32))
        .register_fn("rotate", |m: VState, a: f64| m.rotate(a));
    positional!(e, VState);

    // Text.
    e.register_type_with_name::<TextMobject>("Text")
        .register_fn("text", |s: &str| {
            TextMobject::text(s).map_err(Into::into) as Res<_>
        })
        .register_fn("math_tex", |s: &str| {
            TextMobject::math(s).map_err(Into::into) as Res<_>
        })
        .register_fn("code", |s: &str, lang: &str| {
            TextMobject::code(s, lang).map_err(Into::into) as Res<_>
        })
        .register_fn("list", |a: Array| fastanim_text::list(&a))
        .register_fn("fill", |m: TextMobject, c: Color| m.fill(c))
        .register_fn("rotate", |m: TextMobject, a: f64| {
            let c = m.bbox().map_or(Point::ORIGIN, |b| b.center());
            m.transform(Affine::rotate_about(a, c))
        })
        .register_get("glyphs", |m: &mut TextMobject| {
            m.glyphs
                .iter()
                .cloned()
                .map(Dynamic::from)
                .collect::<Array>()
        });
    positional!(e, TextMobject);

    // Axes.
    e.register_type_with_name::<Axes>("Axes")
        .register_fn("axes", |x: Array, y: Array| {
            Ok(Axes::new(range(x)?, range(y)?)) as Res<_>
        })
        .register_fn("axes", |x: Array, y: Array, w: f64, h: f64| {
            Ok(Axes::sized(range(x)?, range(y)?, w, h)) as Res<_>
        })
        .register_fn("shape", |a: &mut Axes| a.shape())
        .register_fn("c2p", |a: &mut Axes, x: f64, y: f64| a.c2p(x, y).to_vec2())
        .register_fn("p2c", |a: &mut Axes, p: Vec2| a.p2c(p.to_point()).to_vec2())
        .register_fn(
            "plot",
            |cx: NativeCallContext, a: &mut Axes, f: FnPtr, x0: f64, x1: f64| {
                let a = *a;
                graph(&cx, &f, |f| a.plot(f, x0..x1))
            },
        )
        .register_fn("vertical_line", |a: &mut Axes, p: Vec2| {
            a.vertical_line(p.to_point())
        })
        .register_fn("numbers", |a: &mut Axes| fastanim_text::axis_numbers(a))
        .register_fn("labels", |a: &mut Axes, x: &str, y: &str| -> Res<_> {
            let (x, y) = (TextMobject::math(x)?, TextMobject::math(y)?);
            Ok(fastanim_text::axis_labels(a, x, y))
        });
    positional!(e, Axes);

    // The scene.
    e.register_type_with_name::<Mob>("Mobject");
    e.register_type_with_name::<State>("State")
        .register_indexer_get(|s: &mut State, m: Mob| get(&s.0, m));
    e.register_type_with_name::<SceneHandle>("Scene")
        .register_fn("add", |s: &mut SceneHandle, m: VState| {
            Ok(Mob::Shape(s.lock()?.scene.add(m))) as Res<_>
        })
        .register_fn("add", |s: &mut SceneHandle, a: Axes| {
            Ok(Mob::Shape(s.lock()?.scene.add(a.shape()))) as Res<_>
        })
        .register_fn("add", |s: &mut SceneHandle, t: TextMobject| -> Res<_> {
            let cx = &mut *s.lock()?;
            cx.texts.push(t.add_to(&mut cx.scene));
            Ok(Mob::Text(cx.texts.len() - 1))
        })
        .register_fn("remove", |s: &mut SceneHandle, m: Mob| -> Res<()> {
            let mut cx = s.lock()?;
            for id in cx.ids(m)? {
                cx.scene.remove(id);
            }
            Ok(())
        })
        .register_fn("get", |s: &mut SceneHandle, m: Mob| {
            get(s.lock()?.scene.state(), m)
        })
        .register_get("time", |s: &mut SceneHandle| {
            Ok(f64::from(s.lock()?.scene.time())) as Res<_>
        })
        .register_fn("play", |s: &mut SceneHandle, a: Anim| play(s, a))
        .register_fn("play", |s: &mut SceneHandle, a: Array| {
            play(s, parallel(a)?)
        })
        .register_fn("wait", |s: &mut SceneHandle| {
            s.lock()?.scene.wait(1.0);
            Ok(()) as Res<_>
        })
        .register_fn("wait", |s: &mut SceneHandle, secs: f64| {
            s.lock()?.scene.wait(secs as f32);
            Ok(()) as Res<_>
        })
        .register_fn("marker", |s: &mut SceneHandle, name: &str| {
            s.lock()?.scene.marker(name);
            Ok(()) as Res<_>
        })
        .register_fn(
            "always",
            |s: &mut SceneHandle, m: Mob, f: FnPtr| -> Res<()> {
                let cx = &mut *s.lock()?;
                let id = cx.shape(m)?;
                let rt = cx.rt.clone();
                // Fails now rather than silently on every frame.
                let probe =
                    f.call::<Dynamic>(&rt.engine, &rt.ast, (State(cx.scene.state().clone()), 0.0))?;
                if !probe.is::<VState>() {
                    return Err(
                        format!("closure returned {}, not a shape", probe.type_name()).into(),
                    );
                }
                cx.scene.always(id, move |st, t| {
                    // ponytail: clones the scene per updater per frame; pass a shared handle if
                    // scenes with many updaters get slow.
                    rt.call(&f, (State(st.clone()), f64::from(t)))
                        .unwrap_or_else(|| st[&id].clone())
                });
                Ok(())
            },
        );

    // Animations.
    e.register_type_with_name::<Anim>("Animation")
        .register_fn("run_time", |a: Anim, t: f64| Anim {
            run_time: Some(t as f32),
            ..a
        })
        .register_fn("rate", |a: Anim, r: RateFn| Anim { rate: Some(r), ..a });
    e.register_type_with_name::<RateFn>("Rate")
        .register_fn("ease_in", |n: &str| Ok(RateFn::EaseIn(ease(n)?)) as Res<_>)
        .register_fn("ease_out", |n: &str| {
            Ok(RateFn::EaseOut(ease(n)?)) as Res<_>
        })
        .register_fn("ease_in_out", |n: &str| {
            Ok(RateFn::EaseInOut(ease(n)?)) as Res<_>
        })
        .register_fn("spring", |stiffness: f64, damping: f64| RateFn::Spring {
            stiffness: stiffness as f32,
            damping: damping as f32,
        });
    e.register_fn("create", |m: Mob| each(m, fastanim_core::create))
        .register_fn("uncreate", |m: Mob| each(m, fastanim_core::uncreate))
        .register_fn("fade_in", |m: Mob| each(m, fastanim_core::fade_in))
        .register_fn("fade_out", |m: Mob| each(m, fastanim_core::fade_out))
        .register_fn("grow_from_center", |m: Mob| {
            each(m, fastanim_core::grow_from_center)
        })
        .register_fn("shrink_to_center", |m: Mob| {
            each(m, fastanim_core::shrink_to_center)
        })
        .register_fn("spin_in", |m: Mob| each(m, fastanim_core::spin_in))
        .register_fn("draw_border_then_fill", |m: Mob| {
            each(m, fastanim_core::draw_border_then_fill)
        })
        .register_fn("shift", |m: Mob, v: Vec2| {
            each(m, move |id| fastanim_core::shift(id, v))
        })
        .register_fn("rotate", |m: Mob, angle: f64| {
            whole(
                m,
                move |id| fastanim_core::rotate(id, angle),
                move |s, c, a| (s.clone()).transform(Affine::rotate_about(angle * f64::from(a), c)),
            )
        })
        .register_fn("scale", |m: Mob, k: f64| {
            whole(
                m,
                move |id| fastanim_core::scale(id, k),
                move |s, c, a| {
                    let k = 1.0 + (k - 1.0) * f64::from(a);
                    s.clone().transform(Affine::scale_about(k, c))
                },
            )
        })
        .register_fn("move_to", |m: Mob, p: Vec2| {
            let p = p.to_point();
            whole(
                m,
                move |id| fastanim_core::move_to(id, p),
                move |s, c, a| s.clone().shift((p - c) * f64::from(a)),
            )
        })
        .register_fn("write", |m: Mob| by_ids(m, fastanim_core::write))
        .register_fn("unwrite", |m: Mob| by_ids(m, fastanim_core::unwrite))
        .register_fn("indicate", |m: Mob| by_ids(m, fastanim_core::indicate))
        .register_fn("wiggle", |m: Mob| by_ids(m, fastanim_core::wiggle))
        .register_fn("circumscribe", |m: Mob| {
            Anim::new(move |cx| {
                let ids = cx.ids(m)?;
                Ok(Box::new(fastanim_core::circumscribe(&mut cx.scene, &ids)))
            })
        })
        .register_fn("flash", |p: Vec2| {
            Anim::new(move |cx| Ok(Box::new(fastanim_core::flash(&mut cx.scene, p.to_point()))))
        })
        .register_fn("transform", |m: Mob, target: VState| {
            Anim::new(move |cx| {
                Ok(Box::new(fastanim_core::transform(
                    cx.shape(m)?,
                    target.clone(),
                )))
            })
        })
        .register_fn("replacement_transform", |m: Mob, target: Mob| {
            Anim::new(move |cx| {
                let (a, b) = (cx.shape(m)?, cx.shape(target)?);
                Ok(Box::new(fastanim_core::replacement_transform(a, b)))
            })
        })
        .register_fn("apply_function", |m: Mob, f: FnPtr| {
            Anim::new(move |cx| {
                let (rt, f) = (cx.rt.clone(), f.clone());
                Ok(Box::new(fastanim_core::apply_function(
                    cx.shape(m)?,
                    move |p| rt.call(&f, (p.to_vec2(),)).map_or(p, Vec2::to_point),
                )))
            })
        })
        .register_fn("update", |m: Mob, f: FnPtr| {
            Anim::new(move |cx| {
                let (rt, f) = (cx.rt.clone(), f.clone());
                Ok(Box::new(Update::new(cx.shape(m)?, move |s, a| {
                    rt.call(&f, (s.clone(), f64::from(a)))
                        .unwrap_or_else(|| s.clone())
                })))
            })
        })
        .register_fn("transform_diff", |m: Mob, to: TextMobject| {
            transform_diff(m, to, DiffStyle::default(), false)
        })
        .register_fn("transform_diff", |m: Mob, to: TextMobject, opts: Map| {
            Ok(transform_diff(m, to, diff_style(opts)?, false)) as Res<_>
        })
        .register_fn("transform_list", |m: Mob, to: TextMobject| {
            transform_diff(m, to, DiffStyle::default(), true)
        })
        .register_fn("parallel", parallel)
        .register_fn("sequence", |a: Array| -> Res<_> {
            let a = anims(a)?;
            Ok(Anim::new(move |cx| {
                Ok(Box::new(Sequence(build_all(&a, cx)?)))
            }))
        })
        .register_fn("lagged_start", |lag_ratio: f64, a: Array| -> Res<_> {
            let a = anims(a)?;
            Ok(Anim::new(move |cx| {
                let a = build_all(&a, cx)?;
                Ok(Box::new(fastanim_core::lagged_start(lag_ratio as f32, a)))
            }))
        });
    e
}

fn play(s: &mut SceneHandle, a: Anim) -> Res<()> {
    let cx = &mut *s.lock()?;
    let anim = a.build(cx)?;
    cx.scene.play(anim);
    // `play` samples the end of the clip, which runs any closures once.
    match cx.rt.err.lock().unwrap().take() {
        Some(e) => Err(e.into()),
        None => Ok(()),
    }
}

fn parallel(a: Array) -> Res<Anim> {
    let a = anims(a)?;
    Ok(Anim::new(move |cx| {
        Ok(Box::new(Parallel(build_all(&a, cx)?)))
    }))
}

fn transform_diff(m: Mob, to: TextMobject, style: DiffStyle, list: bool) -> Anim {
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
