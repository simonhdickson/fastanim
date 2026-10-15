//! The Rhai frontend: the core API registered on a Rhai [`Engine`].

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};

use fastanim_core::kurbo::{Affine, Point, Vec2};
use fastanim_core::{
    Audio, Axes, BakedTimeline, Color, DEFAULT_BUFF, DiffStyle, Position, RateFn, VState,
};
use fastanim_text::TextMobject;
use rhai::{
    AST, Array, Dynamic, Engine, EvalAltResult, FnPtr, FuncArgs, Map, NativeCallContext,
    ParseError, Scope,
};

use crate::{Anim, Const, Ctx, Item, Mob, Opt, VGroup};

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

/// Runs a Rhai scene script and bakes what it recorded. Relative file paths, such as
/// `audio`'s, are relative to the current directory.
pub fn bake(src: &str) -> Result<BakedTimeline, Error> {
    bake_in(src, Path::new(""))
}

/// [`bake`], with relative file paths relative to `dir`, usually the script's directory.
pub fn bake_in(src: &str, dir: &Path) -> Result<BakedTimeline, Error> {
    let (engine, slot) = engine(dir.to_path_buf());
    let ast = Arc::new(engine.compile(src)?);
    let engine = Arc::new(engine);
    // Weak, or the engine would keep itself alive through the closures it registers.
    let _ = slot.set((Arc::downgrade(&engine), ast.clone()));
    let handle = SceneHandle(Arc::new(Mutex::new(Ctx::new())));
    let mut scope = Scope::new();
    scope.push("scene", handle.clone());
    engine.run_ast_with_scope(&mut scope, &ast)?;
    // Taken rather than unwrapped: closures in the timeline may still hold `scene`.
    let scene = std::mem::take(&mut handle.lock()?.scene);
    Ok(scene.bake())
}

/// Every function a scene can call, as Rhai signatures, then the constants. Rhai's own
/// standard library (`min`, `sin`, `to_float`, ...) is left out.
#[cfg(feature = "metadata")]
pub fn signatures() -> Vec<String> {
    let (e, _) = engine(PathBuf::new());
    // Rhai leaves fallible return types as raw Rust paths; show the script-facing name.
    let mut out: Vec<String> = (e.gen_fn_signatures(false).into_iter())
        .map(|sig| {
            let Some((head, ret)) = sig.rsplit_once(" -> ") else {
                return sig;
            };
            let ret = (ret.strip_prefix("core::result::Result<"))
                .and_then(|r| r.split_once(','))
                .map_or(ret, |(ok, _)| ok);
            format!("{head} -> {}", e.map_type_name(ret))
        })
        .collect();
    out.sort();
    out.extend(crate::CONSTANTS.iter().map(|c| format!("const {c}")));
    out
}

/// What closures need after the script has finished: the engine and the script.
#[derive(Clone)]
struct Rt {
    engine: Arc<Engine>,
    ast: Arc<AST>,
}

impl Rt {
    /// Calls `f`, which must return a `T` (`what`, for the error).
    fn call<T: Clone + Send + Sync + 'static>(
        &self,
        f: &FnPtr,
        what: &str,
        args: impl FuncArgs,
    ) -> Result<T, String> {
        let f = unshared(f, 4);
        let d = (f.call::<Dynamic>(&self.engine, &self.ast, args)).map_err(|e| e.to_string())?;
        let ty = d.type_name();
        (d.try_cast::<T>()).ok_or_else(|| format!("closure returned {ty}, not {what}"))
    }
}

/// `f` with private copies of its captured variables, `depth` closures deep. Captures are shared
/// cells that Rhai write-locks on every method call, so frames evaluated on several threads would
/// otherwise serialise on them. The cap stops a closure that captures itself recursing forever;
/// anything deeper stays shared, which is slower but still correct.
fn unshared(f: &FnPtr, depth: u32) -> FnPtr {
    let mut f = f.clone();
    if depth > 0 {
        let curry: Vec<_> = (f.iter_curry())
            .map(|c| {
                let c = c.flatten_clone();
                match c.clone().try_cast::<FnPtr>() {
                    Some(g) => Dynamic::from(unshared(&g, depth - 1)),
                    None => c,
                }
            })
            .collect();
        f.set_curry(curry);
    }
    f
}

/// Where closures find the engine and script, set once the script is compiled.
type Slot = Arc<OnceLock<(Weak<Engine>, Arc<AST>)>>;

fn rt(slot: &Slot) -> Rt {
    let (engine, ast) = slot.get().expect("set before the script runs");
    Rt {
        engine: engine.upgrade().expect("alive while the script runs"),
        ast: ast.clone(),
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

    fn play(&mut self, a: Anim) -> Res<()> {
        Ok(self.lock()?.play(&a)?)
    }
}

/// The scene as an updater sees it; index it with a shape.
#[derive(Clone)]
struct State(fastanim_core::SceneState);

fn anims(a: Array) -> Res<Vec<Anim>> {
    (a.into_iter())
        .map(|d| {
            let ty = d.type_name();
            d.try_cast::<Anim>()
                .ok_or_else(|| format!("expected an animation, got {ty}").into())
        })
        .collect()
}

fn diff_style(opts: Map) -> Res<DiffStyle> {
    let mut style = DiffStyle::default();
    for (k, v) in opts {
        let v = match (v.as_bool(), v.as_float()) {
            (Ok(b), _) => Opt::Bool(b),
            (_, Ok(n)) => Opt::Num(n),
            _ => return Err(format!("`{k}` can't be {}", v.type_name()).into()),
        };
        crate::diff_option(&mut style, &k, v)?;
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
    Ok(crate::range(&n)?)
}

/// `build` with script closure `f` as the function; the first error `f` raises fails it.
fn graph(
    cx: &NativeCallContext,
    f: &FnPtr,
    build: impl FnOnce(&dyn Fn(f64) -> f64) -> VState,
) -> Res<VState> {
    crate::graph(|x| f.call_within_context(cx, (x,)), build)
}

/// [`Axes::fit_y_range`] with script closure `f`; the first error `f` raises fails it.
fn fit_y(cx: &NativeCallContext, f: &FnPtr, x: [f64; 3]) -> Res<[f64; 3]> {
    crate::graph(
        |x| f.call_within_context(cx, (x,)),
        |f| Axes::fit_y_range(f, x),
    )
}

/// `group([...])`: handles make a [`Mob::Group`], values a [`VGroup`].
fn group(a: Array) -> Res<Dynamic> {
    if a.iter().all(|d| d.is::<Mob>()) && !a.is_empty() {
        let ms = a.into_iter().map(|d| d.cast::<Mob>()).collect();
        return Ok(Dynamic::from(crate::group(ms)));
    }
    let items = (a.into_iter())
        .map(|d| {
            let ty = d.type_name();
            Ok(if d.is::<VState>() {
                Item::Shape(d.cast())
            } else if d.is::<TextMobject>() {
                Item::Text(d.cast())
            } else if d.is::<Axes>() {
                Item::Shape(d.cast::<Axes>().shape())
            } else if d.is::<VGroup>() {
                Item::Group(d.cast())
            } else if d.is::<Mob>() {
                return Err("a group is all mobjects (added) or all shapes, texts and groups (not added yet)".into());
            } else {
                return Err(format!("expected a shape, text, axes or group, got {ty}").into());
            })
        })
        .collect::<Res<_>>()?;
    Ok(Dynamic::from(VGroup(items)))
}

fn constant(name: &str) -> Option<Dynamic> {
    Some(match crate::constant(name)? {
        Const::Point(v) => Dynamic::from(v),
        Const::Color(c) => Dynamic::from(c),
        Const::Rate(r) => Dynamic::from(r),
        Const::Num(n) => n.into(),
    })
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
            .register_fn("next_to", |m: $t, o: VGroup, d: Vec2| {
                m.next_to(&o, d, DEFAULT_BUFF)
            })
            .register_fn("next_to", |m: $t, o: VGroup, d: Vec2, b: f64| {
                m.next_to(&o, d, b)
            })
            .register_fn("align_to", |m: $t, o: VGroup, d: Vec2| m.align_to(&o, d))
            .register_fn("center", |m: $t| {
                m.bbox().map_or(Point::ORIGIN, |b| b.center()).to_vec2()
            })
    };
}

/// The engine with the whole scene API registered, and limits so a runaway script fails
/// instead of hanging (SPEC §14.2).
fn engine(dir: PathBuf) -> (Engine, Slot) {
    let mut e = Engine::new();
    let slot = Slot::default();
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
        .register_fn("latex", |s: &str| {
            TextMobject::latex(s).map_err(Into::into) as Res<_>
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

    // Groups.
    e.register_type_with_name::<VGroup>("Group")
        .register_fn("group", group)
        .register_fn("rotate", VGroup::rotate)
        .register_fn("arrange", |g: VGroup, d: Vec2| g.arrange(d, DEFAULT_BUFF))
        .register_fn("arrange", VGroup::arrange)
        .register_fn("len", |g: &mut VGroup| g.0.len() as i64);
    positional!(e, VGroup);

    // Axes.
    e.register_type_with_name::<Axes>("Axes")
        .register_fn("axes", |x: Array, y: Array| {
            Ok(Axes::new(range(x)?, range(y)?)) as Res<_>
        })
        .register_fn("axes", |x: Array, y: Array, w: f64, h: f64| {
            Ok(Axes::sized(range(x)?, range(y)?, w, h)) as Res<_>
        })
        .register_fn(
            "fit_axes",
            |cx: NativeCallContext, f: FnPtr, x: Array| -> Res<_> {
                let x = range(x)?;
                let y = fit_y(&cx, &f, x)?;
                Ok(Axes::new(x, y))
            },
        )
        .register_fn(
            "fit_axes",
            |cx: NativeCallContext, f: FnPtr, x: Array, w: f64, h: f64| -> Res<_> {
                let x = range(x)?;
                let y = fit_y(&cx, &f, x)?;
                Ok(Axes::sized(x, y, w, h))
            },
        )
        .register_fn("shape", |a: &mut Axes| a.shape())
        .register_fn("c2p", |a: &mut Axes, x: f64, y: f64| a.c2p(x, y).to_vec2())
        .register_fn("p2c", |a: &mut Axes, p: Vec2| a.p2c(p.to_point()).to_vec2())
        .register_fn("plot", |cx: NativeCallContext, a: &mut Axes, f: FnPtr| {
            let a = *a;
            let [x0, x1, _] = a.x_range;
            graph(&cx, &f, |f| a.plot(f, x0..x1))
        })
        .register_fn(
            "plot",
            |cx: NativeCallContext, a: &mut Axes, f: FnPtr, x0: f64, x1: f64| {
                let a = *a;
                graph(&cx, &f, |f| a.plot(f, x0..x1))
            },
        )
        .register_fn(
            "plot",
            |cx: NativeCallContext,
             a: &mut Axes,
             f: FnPtr,
             x0: f64,
             x1: f64,
             segments: i64|
             -> Res<_> {
                if segments < 1 {
                    return Err("segments must be at least 1".into());
                }
                let a = *a;
                graph(&cx, &f, |f| a.plot_segments(f, x0..x1, segments as usize))
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
    e.register_type_with_name::<Mob>("Mobject")
        .register_indexer_get(|m: &mut Mob, i: i64| Ok(m.member(i)?) as Res<_>)
        .register_fn("len", |m: &mut Mob| m.len() as i64);
    e.register_type_with_name::<State>("State")
        .register_indexer_get(|s: &mut State, m: Mob| Ok(crate::get(&s.0, &m)?) as Res<_>);
    e.register_type_with_name::<SceneHandle>("Scene")
        .register_fn("add", |s: &mut SceneHandle, m: VState| {
            Ok(s.lock()?.add_shape(m)) as Res<_>
        })
        .register_fn("add", |s: &mut SceneHandle, a: Axes| {
            Ok(s.lock()?.add_shape(a.shape())) as Res<_>
        })
        .register_fn("add", |s: &mut SceneHandle, t: TextMobject| {
            Ok(s.lock()?.add_text(t)) as Res<_>
        })
        .register_fn("add", |s: &mut SceneHandle, g: VGroup| {
            Ok(s.lock()?.add_group(g)) as Res<_>
        })
        .register_fn("remove", |s: &mut SceneHandle, m: Mob| {
            Ok(s.lock()?.remove(&m)?) as Res<()>
        })
        .register_fn("get", |s: &mut SceneHandle, m: Mob| {
            Ok(crate::get(s.lock()?.scene.state(), &m)?) as Res<_>
        })
        .register_get("time", |s: &mut SceneHandle| {
            Ok(f64::from(s.lock()?.scene.time())) as Res<_>
        })
        .register_fn("play", |s: &mut SceneHandle, a: Anim| s.play(a))
        .register_fn("play", |s: &mut SceneHandle, a: Array| {
            s.play(crate::parallel(anims(a)?))
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
        .register_fn("add_sound", |s: &mut SceneHandle, a: Audio| {
            Ok(s.lock()?.add_sound(&a, None)?) as Res<()>
        })
        .register_fn(
            "add_sound",
            |s: &mut SceneHandle, a: Audio, start: f64, end: f64| {
                Ok(s.lock()?.add_sound(&a, Some((start as f32, end as f32)))?) as Res<()>
            },
        )
        .register_fn("wait_for_sounds", |s: &mut SceneHandle| {
            s.lock()?.scene.wait_for_sounds();
            Ok(()) as Res<_>
        })
        .register_fn("always", {
            let slot = slot.clone();
            move |s: &mut SceneHandle, m: Mob, f: FnPtr| -> Res<()> {
                let rt = rt(&slot);
                Ok(s.lock()?.always(&m, move |st, t| {
                    // shortcut: clones the scene per updater per frame; pass a shared handle
                    // if scenes with many updaters get slow.
                    rt.call(&f, "a shape", (State(st.clone()), f64::from(t)))
                })?)
            }
        });

    // Sound.
    e.register_type_with_name::<Audio>("Audio")
        .register_fn("audio", move |path: &str| {
            Ok(crate::audio(&dir.join(path))?) as Res<_>
        })
        .register_get("duration", |a: &mut Audio| f64::from(a.duration()))
        .register_fn("sound", |a: Audio| Ok(crate::sound(&a, None)?) as Res<_>)
        .register_fn("sound", |a: Audio, start: f64, end: f64| {
            Ok(crate::sound(&a, Some((start as f32, end as f32)))?) as Res<_>
        });

    // Animations.
    e.register_type_with_name::<Anim>("Animation")
        .register_fn("run_time", |a: Anim, t: f64| Anim {
            run_time: Some(t as f32),
            ..a
        })
        .register_fn("rate", |a: Anim, r: RateFn| Anim { rate: Some(r), ..a })
        .register_fn("named", |a: Anim, name: &str| Anim {
            name: Some(name.into()),
            ..a
        });
    e.register_type_with_name::<RateFn>("Rate")
        .register_fn("ease_in", |n: &str| {
            Ok(RateFn::EaseIn(crate::ease(n)?)) as Res<_>
        })
        .register_fn("ease_out", |n: &str| {
            Ok(RateFn::EaseOut(crate::ease(n)?)) as Res<_>
        })
        .register_fn("ease_in_out", |n: &str| {
            Ok(RateFn::EaseInOut(crate::ease(n)?)) as Res<_>
        })
        .register_fn("spring", |stiffness: f64, damping: f64| RateFn::Spring {
            stiffness: stiffness as f32,
            damping: damping as f32,
        });
    e.register_fn("create", crate::create)
        .register_fn("uncreate", crate::uncreate)
        .register_fn("fade_in", crate::fade_in)
        .register_fn("fade_out", crate::fade_out)
        .register_fn("grow_from_center", crate::grow_from_center)
        .register_fn("shrink_to_center", crate::shrink_to_center)
        .register_fn("spin_in", crate::spin_in)
        .register_fn("draw_border_then_fill", crate::draw_border_then_fill)
        .register_fn("shift", crate::shift)
        .register_fn("rotate", crate::rotate)
        .register_fn("scale", crate::scale)
        .register_fn("move_to", |m: Mob, p: Vec2| crate::move_to(m, p.to_point()))
        .register_fn("write", crate::write)
        .register_fn("unwrite", crate::unwrite)
        .register_fn("indicate", crate::indicate)
        .register_fn("wiggle", crate::wiggle)
        .register_fn("circumscribe", crate::circumscribe)
        .register_fn("flash", |p: Vec2| crate::flash(p.to_point()))
        .register_fn("transform", crate::transform)
        .register_fn("replacement_transform", crate::replacement_transform)
        .register_fn("apply_function", {
            let slot = slot.clone();
            move |m: Mob, f: FnPtr| {
                let rt = rt(&slot);
                crate::apply_function(m, move |p| {
                    (rt.call::<Vec2>(&f, "a point", (p.to_vec2(),))).map(Vec2::to_point)
                })
            }
        })
        .register_fn("update", {
            let slot = slot.clone();
            move |m: Mob, f: FnPtr| {
                let rt = rt(&slot);
                crate::update(m, move |s, a| {
                    rt.call(&f, "a shape", (s.clone(), f64::from(a)))
                })
            }
        })
        .register_fn("transform_diff", |m: Mob, to: TextMobject| {
            crate::transform_diff(m, to, DiffStyle::default(), false)
        })
        .register_fn("transform_diff", |m: Mob, to: TextMobject, opts: Map| {
            Ok(crate::transform_diff(m, to, diff_style(opts)?, false)) as Res<_>
        })
        .register_fn("transform_list", |m: Mob, to: TextMobject| {
            crate::transform_diff(m, to, DiffStyle::default(), true)
        })
        .register_fn("parallel", |a: Array| {
            Ok(crate::parallel(anims(a)?)) as Res<_>
        })
        .register_fn("sequence", |a: Array| {
            Ok(crate::sequence(anims(a)?)) as Res<_>
        })
        .register_fn("lagged_start", |lag_ratio: f64, a: Array| {
            Ok(crate::lagged_start(lag_ratio as f32, anims(a)?)) as Res<_>
        });
    (e, slot)
}
