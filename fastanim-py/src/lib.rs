//! The `fastanim` Python package (see `docs/SPEC.md` §15.2): the `fastanim-script` core bound
//! with pyo3, as the native module `fastanim._native`. The pure-Python half of the package
//! lives in `python/fastanim`.

mod render;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use fastanim_core::kurbo::{Affine, Point, Rect, Vec2};
use fastanim_core::{
    Audio as CoreAudio, Axes as CoreAxes, BakedTimeline, BinaryOp, Color as CoreColor,
    DEFAULT_BUFF, DiffStyle, LottieMobject, Position, RIGHT, RateFn, SceneState, VState,
};
use fastanim_script::{Anim, Const, Ctx, Glyphs, Item, Mob, Opt, VGroup};
use fastanim_text::{TextMobject, TextStyle};
use pyo3::create_exception;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

create_exception!(
    fastanim,
    SceneError,
    PyValueError,
    "Misuse of the scene API, such as playing a removed mobject."
);
create_exception!(
    fastanim,
    TypesetError,
    PyValueError,
    "Typst failed to typeset a snippet."
);

fn scene_err(e: String) -> PyErr {
    SceneError::new_err(e)
}

fn typeset_err(e: String) -> PyErr {
    TypesetError::new_err(e)
}

/// `Python::attach` for calling a scene's Python closures from Rust threads (export's frame
/// workers, the preview). A thread Python didn't start gets a fresh Python thread state on
/// every attach, and drops it after; that cost dominates small updaters, most of all on
/// free-threaded Python, and loses their `threading.local`. This makes one per thread and
/// keeps it until the thread ends.
fn attach<R>(f: impl for<'py> FnOnce(Python<'py>) -> R) -> R {
    use pyo3::ffi;

    struct Kept(Option<(ffi::PyGILState_STATE, *mut ffi::PyThreadState)>);
    impl Drop for Kept {
        fn drop(&mut self) {
            // SAFETY: undoes `new` below on the same thread, while the interpreter is alive.
            if let Some((gil, ts)) = self.0.take()
                && unsafe { ffi::Py_IsInitialized() } != 0
            {
                unsafe {
                    ffi::PyEval_RestoreThread(ts);
                    ffi::PyGILState_Release(gil);
                }
            }
        }
    }
    thread_local! {
        static KEPT: Kept = Kept(
            // SAFETY: this thread has no Python thread state (checked), so isn't attached;
            // `Ensure` makes one and attaches, `SaveThread` detaches and keeps it.
            (unsafe { ffi::PyGILState_GetThisThreadState() }.is_null()).then(|| unsafe {
                let gil = ffi::PyGILState_Ensure();
                (gil, ffi::PyEval_SaveThread())
            }),
        );
    }
    KEPT.with(|_| {});
    Python::attach(f)
}

/// A Python exception as a core error, keeping its traceback.
fn closure_err(py: Python<'_>, e: PyErr) -> String {
    let tb = (e.traceback(py)).and_then(|tb| tb.format().ok());
    format!("{}{e}", tb.unwrap_or_default())
}

fn type_name(o: &Bound<'_, PyAny>) -> String {
    (o.get_type().name()).map_or_else(|_| "?".into(), |n| n.to_string())
}

/// `o` as a `T`, or "closure returned X, not `what`".
fn returned<'py, T: FromPyObjectOwned<'py>>(
    o: &Bound<'py, PyAny>,
    what: &str,
) -> Result<T, String> {
    o.extract()
        .map_err(|_| format!("closure returned {}, not {what}", type_name(o)))
}

// Values.

/// A point or vector.
#[pyclass(frozen, from_py_object, name = "Point", module = "fastanim")]
#[derive(Clone, Copy)]
struct PyPoint(Vec2);

/// Anything a point is taken as: a `Point`, an `(x, y)` tuple or a length-2 sequence such as a
/// numpy array.
struct P(Vec2);

impl<'py> FromPyObject<'_, 'py> for P {
    type Error = PyErr;

    fn extract(o: Borrowed<'_, 'py, PyAny>) -> PyResult<Self> {
        if let Ok(p) = o.cast::<PyPoint>() {
            return Ok(P(p.get().0));
        }
        match o.extract::<Vec<f64>>() {
            Ok(v) if v.len() == 2 => Ok(P(Vec2::new(v[0], v[1]))),
            _ => Err(PyTypeError::new_err(format!(
                "expected a point, got {}",
                type_name(&o)
            ))),
        }
    }
}

impl P {
    fn pt(&self) -> Point {
        self.0.to_point()
    }
}

#[pymethods]
impl PyPoint {
    #[new]
    fn new(x: f64, y: f64) -> Self {
        Self(Vec2::new(x, y))
    }

    #[getter]
    fn x(&self) -> f64 {
        self.0.x
    }

    #[getter]
    fn y(&self) -> f64 {
        self.0.y
    }

    fn __add__(&self, o: P) -> Self {
        Self(self.0 + o.0)
    }

    fn __radd__(&self, o: P) -> Self {
        Self(o.0 + self.0)
    }

    fn __sub__(&self, o: P) -> Self {
        Self(self.0 - o.0)
    }

    fn __rsub__(&self, o: P) -> Self {
        Self(o.0 - self.0)
    }

    fn __mul__(&self, k: f64) -> Self {
        Self(self.0 * k)
    }

    fn __rmul__(&self, k: f64) -> Self {
        Self(self.0 * k)
    }

    fn __truediv__(&self, k: f64) -> Self {
        Self(self.0 / k)
    }

    fn __neg__(&self) -> Self {
        Self(-self.0)
    }

    fn __abs__(&self) -> f64 {
        self.0.hypot()
    }

    fn __eq__(&self, o: &Bound<'_, PyAny>) -> bool {
        o.extract::<P>().is_ok_and(|o| o.0 == self.0)
    }

    fn __len__(&self) -> usize {
        2
    }

    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(PyTuple::new(py, [self.0.x, self.0.y])?
            .try_iter()?
            .into_any())
    }

    fn __repr__(&self) -> String {
        format!("point({}, {})", self.0.x, self.0.y)
    }
}

/// A color.
#[pyclass(frozen, from_py_object, name = "Color", module = "fastanim")]
#[derive(Clone, Copy)]
struct PyColor(CoreColor);

/// Anything a color is taken as: a `Color`, `"#rrggbb"` (or `"#rrggbbaa"`), or an
/// `(r, g, b[, a])` tuple of channels in `0..=1`.
struct C(CoreColor);

impl<'py> FromPyObject<'_, 'py> for C {
    type Error = PyErr;

    fn extract(o: Borrowed<'_, 'py, PyAny>) -> PyResult<Self> {
        if let Ok(c) = o.cast::<PyColor>() {
            return Ok(C(c.get().0));
        }
        if let Ok(s) = o.extract::<String>() {
            return parse_hex(&s).map(C).ok_or_else(|| {
                PyValueError::new_err(format!("bad color {s:?}: expected #rrggbb or #rrggbbaa"))
            });
        }
        match o.extract::<Vec<f32>>().as_deref() {
            Ok(&[r, g, b]) => Ok(C(CoreColor::rgb(r, g, b))),
            Ok(&[r, g, b, a]) => Ok(C(CoreColor::rgba(r, g, b, a))),
            _ => Err(PyTypeError::new_err(format!(
                "expected a color, \"#rrggbb\" or (r, g, b[, a]), got {}",
                type_name(&o)
            ))),
        }
    }
}

fn parse_hex(s: &str) -> Option<CoreColor> {
    let h = s.strip_prefix('#')?;
    let n = u32::from_str_radix(h, 16).ok()?;
    match h.len() {
        6 => Some(CoreColor::hex(n)),
        8 => Some(CoreColor::hex(n >> 8).with_alpha((n & 0xff) as f32 / 255.0)),
        _ => None,
    }
}

#[pymethods]
impl PyColor {
    fn with_alpha(&self, a: f32) -> Self {
        Self(self.0.with_alpha(a))
    }

    fn __repr__(&self) -> String {
        format!("Color({:?})", self.0.to_hex())
    }
}

/// A rate function: how an animation's progress follows time.
#[pyclass(frozen, from_py_object, name = "Rate", module = "fastanim")]
#[derive(Clone, Copy)]
struct PyRate(RateFn);

// Shapes, texts and axes.

/// A shape, before or after it is added to a scene.
#[pyclass(frozen, from_py_object, name = "Shape", module = "fastanim")]
#[derive(Clone)]
struct Shape(VState);

/// Typeset text, before it is added to a scene.
#[pyclass(frozen, from_py_object, name = "Text", module = "fastanim")]
#[derive(Clone)]
struct Text(TextMobject);

/// Coordinate axes.
#[pyclass(frozen, from_py_object, name = "Axes", module = "fastanim")]
#[derive(Clone, Copy)]
struct PyAxes(CoreAxes);

/// A Lottie animation, before it is added to a scene.
#[pyclass(frozen, from_py_object, name = "Lottie", module = "fastanim")]
#[derive(Clone)]
struct PyLottie(LottieMobject);

/// A loaded WAV file.
#[pyclass(frozen, from_py_object, name = "Audio", module = "fastanim")]
#[derive(Clone)]
struct PyAudio(CoreAudio);

#[pymethods]
impl PyAudio {
    /// Length in seconds.
    #[getter]
    fn duration(&self) -> f32 {
        self.0.duration()
    }
}

impl PyAudio {
    /// Seconds `start..end`, either end defaulting to the file's; `None` for the whole file.
    fn range(&self, start: Option<f32>, end: Option<f32>) -> Option<(f32, f32)> {
        (start.is_some() || end.is_some())
            .then(|| (start.unwrap_or(0.0), end.unwrap_or(self.0.duration())))
    }
}

/// Shapes, texts and groups positioned as one, before they are added to a scene.
#[pyclass(frozen, from_py_object, name = "Group", module = "fastanim")]
#[derive(Clone)]
struct PyGroup(VGroup);

/// What a `Group` holds.
#[derive(FromPyObject)]
enum GroupItem {
    Shape(Shape),
    Text(Text),
    Axes(PyAxes),
    Group(PyGroup),
}

impl GroupItem {
    fn item(self) -> Item {
        match self {
            GroupItem::Shape(s) => Item::Shape(s.0),
            GroupItem::Text(t) => Item::Text(t.0),
            GroupItem::Axes(a) => Item::Shape(a.0.shape()),
            GroupItem::Group(g) => Item::Group(g.0),
        }
    }
}

/// `group(*items)`: mobjects make a group mobject; shapes, texts and groups a `Group`.
#[pyfunction]
#[pyo3(signature = (*items))]
fn group(py: Python<'_>, items: Vec<Bound<'_, PyAny>>) -> PyResult<Py<PyAny>> {
    if !items.is_empty() && items.iter().all(|o| o.is_instance_of::<PyMob>()) {
        let ms = (items.iter())
            .map(|o| Ok(o.extract::<PyMob>()?.0))
            .collect::<PyResult<_>>()?;
        return Ok(Py::new(py, PyMob(fastanim_script::group(ms)))?.into_any());
    }
    let items = (items.iter())
        .map(|o| match o.extract::<GroupItem>() {
            Ok(m) => Ok(m.item()),
            Err(_) if o.is_instance_of::<PyMob>() => Err(PyTypeError::new_err(
                "a group is all mobjects (added) or all shapes, texts and groups (not added yet)",
            )),
            Err(_) => Err(PyTypeError::new_err(format!(
                "expected a shape, text, axes or group, got {}",
                type_name(o)
            ))),
        })
        .collect::<PyResult<_>>()?;
    Ok(Py::new(py, PyGroup(VGroup(items)))?.into_any())
}

/// What `next_to` and `align_to` place against.
#[derive(FromPyObject)]
enum Other {
    Shape(Shape),
    Text(Text),
    Axes(PyAxes),
    Group(PyGroup),
    Point(P),
}

impl Other {
    fn bbox(&self) -> Option<Rect> {
        match self {
            Other::Shape(s) => s.0.bbox(),
            Other::Text(t) => t.0.bbox(),
            Other::Axes(a) => a.0.bbox(),
            Other::Group(g) => g.0.bbox(),
            Other::Point(p) => p.pt().bbox(),
        }
    }
}

fn next_to<T: Position>(m: T, o: &Other, d: Vec2, buff: f64) -> T {
    match o.bbox() {
        Some(r) => m.next_to(&r, d, buff),
        None => m,
    }
}

fn align_to<T: Position>(m: T, o: &Other, d: Vec2) -> T {
    match o.bbox() {
        Some(r) => m.align_to(&r, d),
        None => m,
    }
}

/// The methods every positioned value has, plus `$extra`.
macro_rules! positioned {
    ($t:ty { $($extra:tt)* }) => {
        #[pymethods]
        impl $t {
            fn shift(&self, v: P) -> Self {
                Self(self.0.clone().shift(v.0))
            }

            fn move_to(&self, p: P) -> Self {
                Self(self.0.clone().move_to(p.pt()))
            }

            fn scale(&self, k: f64) -> Self {
                Self(self.0.clone().scale(k))
            }

            #[allow(clippy::wrong_self_convention)]
            fn to_edge(&self, d: P) -> Self {
                Self(self.0.clone().to_edge(d.0))
            }

            #[pyo3(signature = (other, direction, buff = DEFAULT_BUFF))]
            fn next_to(&self, other: Other, direction: P, buff: f64) -> Self {
                Self(next_to(self.0.clone(), &other, direction.0, buff))
            }

            fn align_to(&self, other: Other, direction: P) -> Self {
                Self(align_to(self.0.clone(), &other, direction.0))
            }

            /// Scales about the center to be `height` tall.
            fn scale_to_fit_height(&self, height: f64) -> Self {
                Self(self.0.clone().scale_to_fit_height(height))
            }

            /// Scales about the center to be `width` wide.
            fn scale_to_fit_width(&self, width: f64) -> Self {
                Self(self.0.clone().scale_to_fit_width(width))
            }

            /// The bounding-box center.
            fn center(&self) -> PyPoint {
                PyPoint(self.0.bbox().map_or(Point::ORIGIN, |b| b.center()).to_vec2())
            }

            /// The bounding-box width.
            #[getter]
            fn width(&self) -> f64 {
                self.0.bbox().map_or(0.0, |b| b.width())
            }

            /// The bounding-box height.
            #[getter]
            fn height(&self) -> f64 {
                self.0.bbox().map_or(0.0, |b| b.height())
            }

            $($extra)*
        }
    };
}

/// Shape style keywords, applied to `m`.
fn styled(
    m: VState,
    stroke: Option<C>,
    stroke_width: Option<f64>,
    fill: Option<C>,
    fill_opacity: Option<f32>,
) -> VState {
    let w = stroke_width.unwrap_or(m.stroke.width);
    let m = match stroke {
        Some(c) => m.stroke(c.0, w),
        None => VState {
            stroke: fastanim_core::Stroke {
                width: w,
                ..m.stroke
            },
            ..m
        },
    };
    let m = match fill {
        Some(f) => m.fill(f.0),
        None => m,
    };
    match fill_opacity {
        Some(o) => m.fill_opacity(o),
        None => m,
    }
}

/// A fill color, its alpha replaced by `opacity` if given.
fn with_opacity(color: C, opacity: Option<f32>) -> CoreColor {
    opacity.map_or(color.0, |o| color.0.with_alpha(o))
}

fn colors(stops: Vec<C>) -> Vec<CoreColor> {
    stops.into_iter().map(|c| c.0).collect()
}

positioned!(Shape {
    /// Sets the fill color, and its opacity if given.
    #[pyo3(signature = (color, opacity = None))]
    fn fill(&self, color: C, opacity: Option<f32>) -> Self {
        Self(self.0.clone().fill(with_opacity(color, opacity)))
    }

    /// Sets the fill's opacity, keeping its color or gradient.
    fn fill_opacity(&self, opacity: f32) -> Self {
        Self(self.0.clone().fill_opacity(opacity))
    }

    #[pyo3(signature = (color, width = None))]
    fn stroke(&self, color: C, width: Option<f64>) -> Self {
        Self(styled(self.0.clone(), Some(color), width, None, None))
    }

    /// Sets the outline's opacity, keeping its color or gradient.
    fn stroke_opacity(&self, opacity: f32) -> Self {
        Self(self.0.clone().stroke_opacity(opacity))
    }

    /// Fills with a linear gradient of `colors` across the shape along `direction`.
    #[pyo3(signature = (colors, direction = None))]
    fn fill_gradient(&self, colors: Vec<C>, direction: Option<P>) -> Self {
        let d = direction.map_or(RIGHT, |p| p.0);
        Self(self.0.clone().fill_gradient(&self::colors(colors), d))
    }

    /// Paints the fill with a WGSL shader defining `fn shade(uv: vec2<f32>) -> vec4<f32>`,
    /// drawn on the GPU by preview and export; SVG and the web player draw the solid fill.
    fn shader(&self, source: &str) -> Self {
        Self(self.0.clone().shader(source))
    }

    /// Strokes the outline with a linear gradient of `colors` along `direction`.
    #[pyo3(signature = (colors, direction = None))]
    fn stroke_gradient(&self, colors: Vec<C>, direction: Option<P>) -> Self {
        let d = direction.map_or(RIGHT, |p| p.0);
        Self(self.0.clone().stroke_gradient(&self::colors(colors), d))
    }

    /// Sets how open outline ends are drawn: `"butt"`, `"round"` or `"square"`.
    fn line_cap(&self, cap: &str) -> PyResult<Self> {
        let cap = fastanim_script::line_cap(cap).map_err(PyValueError::new_err)?;
        Ok(Self(self.0.clone().line_cap(cap)))
    }

    /// Sets how outline corners are drawn: `"miter"`, `"round"` or `"bevel"`.
    fn line_join(&self, join: &str) -> PyResult<Self> {
        let join = fastanim_script::line_join(join).map_err(PyValueError::new_err)?;
        Ok(Self(self.0.clone().line_join(join)))
    }

    /// The outline cut into `dashes` dashes, each `ratio` of its period long.
    #[pyo3(signature = (dashes = 15, ratio = 0.5))]
    fn dashed(&self, dashes: usize, ratio: f64) -> Self {
        Self(self.0.clone().dashed(dashes, ratio))
    }

    fn z_index(&self, z: i32) -> Self {
        Self(self.0.clone().z_index(z))
    }

    /// Draws an image with sharp pixels when scaled up.
    fn pixelated(&self) -> Self {
        Self(self.0.clone().pixelated())
    }

    fn rotate(&self, angle: f64) -> Self {
        Self(self.0.clone().rotate(angle))
    }

    fn __repr__(&self) -> String {
        "Shape(...)".into()
    }
});

positioned!(Text {
    /// Sets every glyph's fill color, and its opacity if given.
    #[pyo3(signature = (color, opacity = None))]
    fn fill(&self, color: C, opacity: Option<f32>) -> Self {
        Self(self.0.clone().fill(with_opacity(color, opacity)))
    }

    /// Fills the whole text with one linear gradient of `colors` along `direction`.
    #[pyo3(signature = (colors, direction = None))]
    fn fill_gradient(&self, colors: Vec<C>, direction: Option<P>) -> Self {
        let d = direction.map_or(RIGHT, |p| p.0);
        Self(self.0.clone().fill_gradient(&self::colors(colors), d))
    }

    /// Colors glyph by glyph along `colors`, first glyph to last.
    fn set_color_by_gradient(&self, colors: Vec<C>) -> Self {
        Self(self.0.clone().set_color_by_gradient(&self::colors(colors)))
    }

    fn rotate(&self, angle: f64) -> Self {
        let c = self.0.bbox().map_or(Point::ORIGIN, |b| b.center());
        Self(self.0.clone().transform(Affine::rotate_about(angle, c)))
    }

    /// One shape per glyph.
    #[getter]
    fn glyphs(&self) -> Vec<Shape> {
        self.0.glyphs.iter().cloned().map(Shape).collect()
    }

    fn __repr__(&self) -> String {
        format!("Text({:?})", self.0.source)
    }
});

positioned!(PyLottie {
    fn rotate(&self, angle: f64) -> Self {
        Self(self.0.clone().rotate(angle))
    }

    /// Shows frame `frame` until it plays.
    fn at_frame(&self, frame: f64) -> Self {
        Self(self.0.clone().at_frame(frame))
    }

    /// Length in seconds at its own frame rate.
    #[getter]
    fn duration(&self) -> f32 {
        self.0.lottie.duration()
    }

    fn __repr__(&self) -> String {
        let l = &self.0.lottie;
        format!("Lottie({}x{}, {}s)", l.width(), l.height(), l.duration())
    }
});

positioned!(PyAxes {
    /// The axes as one shape.
    fn shape(&self) -> Shape {
        Shape(self.0.shape())
    }

    /// Coordinates to a point in the scene.
    fn c2p(&self, x: f64, y: f64) -> PyPoint {
        PyPoint(self.0.c2p(x, y).to_vec2())
    }

    /// A point in the scene to coordinates.
    fn p2c(&self, p: P) -> PyPoint {
        PyPoint(self.0.p2c(p.pt()).to_vec2())
    }

    /// The graph of `f` from `x0` to `x1` (default: the whole x range), in these coordinates,
    /// with `segments` cubic pieces.
    #[pyo3(signature = (f, x0 = None, x1 = None, segments = 64))]
    fn plot(
        &self,
        py: Python<'_>,
        f: Py<PyAny>,
        x0: Option<f64>,
        x1: Option<f64>,
        segments: usize,
    ) -> PyResult<Shape> {
        if segments < 1 {
            return Err(PyValueError::new_err("segments must be at least 1"));
        }
        let a = self.0;
        let [lo, hi, _] = a.x_range;
        let range = x0.unwrap_or(lo)..x1.unwrap_or(hi);
        graph(py, &f, |g| a.plot_segments(g, range, segments))
    }

    fn vertical_line(&self, p: P) -> Shape {
        Shape(self.0.vertical_line(p.pt()))
    }

    /// Tick numbers along both axes.
    fn numbers(&self) -> Text {
        Text(fastanim_text::axis_numbers(&self.0))
    }

    /// Math labels for the x and y axes.
    fn labels(&self, x: &str, y: &str) -> PyResult<Text> {
        let x = TextMobject::math(x).map_err(typeset_err)?;
        let y = TextMobject::math(y).map_err(typeset_err)?;
        Ok(Text(fastanim_text::axis_labels(&self.0, x, y)))
    }
});

positioned!(PyGroup {
    fn rotate(&self, angle: f64) -> Self {
        Self(self.0.clone().rotate(angle))
    }

    /// Lays the items out in a row in `direction`, `buff` apart.
    #[pyo3(signature = (direction, buff = DEFAULT_BUFF))]
    fn arrange(&self, direction: P, buff: f64) -> Self {
        Self(self.0.clone().arrange(direction.0, buff))
    }

    /// Colors every shape and glyph along `colors`, first to last, keeping their opacities.
    fn set_color_by_gradient(&self, colors: Vec<C>) -> Self {
        Self(self.0.clone().set_color_by_gradient(&self::colors(colors)))
    }

    fn __len__(&self) -> usize {
        self.0.0.len()
    }

    fn __repr__(&self) -> String {
        format!("Group(<{} items>)", self.0.0.len())
    }
});

/// `build` with Python function `f`; the first exception `f` raises fails it.
fn graph(
    py: Python<'_>,
    f: &Py<PyAny>,
    build: impl FnOnce(&dyn Fn(f64) -> f64) -> VState,
) -> PyResult<Shape> {
    let m = fastanim_script::graph(|x| f.call1(py, (x,))?.extract::<f64>(py), build)?;
    Ok(Shape(m))
}

/// Axes over `x_range` with the y range fitted to `f`, optionally sized.
#[pyfunction]
#[pyo3(signature = (f, x_range, width = None, height = None))]
fn fit_axes(
    py: Python<'_>,
    f: Py<PyAny>,
    x_range: Vec<f64>,
    width: Option<f64>,
    height: Option<f64>,
) -> PyResult<PyAxes> {
    let x = fastanim_script::range(&x_range).map_err(PyValueError::new_err)?;
    let y = fastanim_script::graph(
        |x| f.call1(py, (x,))?.extract::<f64>(py),
        |g| CoreAxes::fit_y_range(g, x),
    )?;
    axes(x.to_vec(), y.to_vec(), width, height)
}

// The scene.

/// A mobject in the scene: what `Scene.add` returns.
#[pyclass(frozen, from_py_object, name = "Mobject", module = "fastanim")]
#[derive(Clone)]
struct PyMob(Mob);

#[pymethods]
impl PyMob {
    /// Member `i` of a group; negative counts from the end.
    fn __getitem__(&self, i: i64) -> PyResult<Self> {
        self.0.member(i).map(Self).map_err(scene_err)
    }

    /// How many members a group has; 1 for anything else.
    fn __len__(&self) -> usize {
        self.0.len()
    }

    /// Chain methods on this to get an animation to their result, e.g.
    /// `scene.play(eq.animate.shift(UP).scale(0.8))`.
    #[getter]
    fn animate(&self) -> Animate {
        Animate {
            m: self.0.clone(),
            ops: Vec::new(),
        }
    }
}

type Op = Arc<dyn Fn(Glyphs) -> Glyphs + Send + Sync>;

/// A mobject's `.animate`: the methods chained on it, applied when played.
#[pyclass(frozen, from_py_object, name = "Animate", module = "fastanim")]
#[derive(Clone)]
struct Animate {
    m: Mob,
    ops: Vec<Op>,
}

impl Animate {
    fn then(&self, op: impl Fn(Glyphs) -> Glyphs + Send + Sync + 'static) -> Self {
        let mut ops = self.ops.clone();
        ops.push(Arc::new(op));
        Self {
            m: self.m.clone(),
            ops,
        }
    }

    fn anim(&self) -> Anim {
        let ops = self.ops.clone();
        fastanim_script::animate(self.m.clone(), move |g| {
            Ok(ops.iter().fold(g, |g, op| op(g)))
        })
    }
}

fn center(g: &Glyphs) -> Point {
    g.bbox().map_or(Point::ORIGIN, |b| b.center())
}

fn each_glyph(g: Glyphs, f: impl Fn(VState) -> VState) -> Glyphs {
    Glyphs(g.0.into_iter().map(f).collect())
}

#[pymethods]
impl Animate {
    fn shift(&self, v: P) -> Self {
        let v = v.0;
        self.then(move |g| g.transform(Affine::translate(v)))
    }

    fn move_to(&self, p: P) -> Self {
        let p = p.pt();
        self.then(move |g| {
            let c = center(&g);
            g.transform(Affine::translate(p - c))
        })
    }

    fn scale(&self, k: f64) -> Self {
        self.then(move |g| {
            let c = center(&g);
            g.transform(Affine::scale_about(k, c))
        })
    }

    fn rotate(&self, angle: f64) -> Self {
        self.then(move |g| {
            let c = center(&g);
            g.transform(Affine::rotate_about(angle, c))
        })
    }

    fn to_edge(&self, d: P) -> Self {
        let d = d.0;
        self.then(move |g| g.to_edge(d))
    }

    #[pyo3(signature = (other, direction, buff = DEFAULT_BUFF))]
    fn next_to(&self, other: Other, direction: P, buff: f64) -> Self {
        let (r, d) = (other.bbox(), direction.0);
        self.then(move |g| match r {
            Some(r) => g.next_to(&r, d, buff),
            None => g,
        })
    }

    fn align_to(&self, other: Other, direction: P) -> Self {
        let (r, d) = (other.bbox(), direction.0);
        self.then(move |g| match r {
            Some(r) => g.align_to(&r, d),
            None => g,
        })
    }

    #[pyo3(signature = (color, opacity = None))]
    fn fill(&self, color: C, opacity: Option<f32>) -> Self {
        let c = with_opacity(color, opacity);
        self.then(move |g| each_glyph(g, |m| m.fill(c)))
    }

    #[pyo3(signature = (color, width = None))]
    fn stroke(&self, color: C, width: Option<f64>) -> Self {
        let c = color.0;
        self.then(move |g| {
            each_glyph(g, |m| {
                let w = width.unwrap_or(m.stroke.width);
                m.stroke(c, w)
            })
        })
    }
}

/// An animation, played with `Scene.play`.
#[pyclass(frozen, from_py_object, name = "Animation", module = "fastanim")]
#[derive(Clone)]
struct PyAnim(Anim);

#[pymethods]
impl PyAnim {
    fn run_time(&self, t: f32) -> Self {
        let mut a = self.0.clone();
        a.run_time = Some(t);
        Self(a)
    }

    fn rate(&self, r: PyRate) -> Self {
        let mut a = self.0.clone();
        a.rate = Some(r.0);
        Self(a)
    }
}

/// An animation, or a mobject's `.animate`.
#[derive(FromPyObject)]
enum A {
    Anim(PyAnim),
    Animate(Animate),
}

impl A {
    fn anim(self) -> Anim {
        match self {
            A::Anim(a) => a.0,
            A::Animate(a) => a.anim(),
        }
    }
}

fn anims(a: Vec<A>) -> Vec<Anim> {
    a.into_iter().map(A::anim).collect()
}

/// What an updater sees: index it with a shape's mobject to get its shape now.
#[pyclass(frozen, name = "State", module = "fastanim")]
struct State(Arc<SceneState>);

#[pymethods]
impl State {
    fn __getitem__(&self, m: PyMob) -> PyResult<Shape> {
        fastanim_script::get(&self.0, &m.0)
            .map(Shape)
            .map_err(scene_err)
    }
}

/// A scene being recorded; the argument of an `@scene` function.
#[pyclass(frozen, name = "Scene", module = "fastanim")]
struct PyScene(Arc<Mutex<Ctx>>);

/// What `Scene.add` takes.
#[derive(FromPyObject)]
enum Addable {
    Shape(Shape),
    Text(Text),
    Axes(PyAxes),
    Lottie(PyLottie),
    Group(PyGroup),
}

impl PyScene {
    fn lock(&self) -> PyResult<MutexGuard<'_, Ctx>> {
        // Fails rather than deadlocks when a closure run by `play` uses the scene.
        (self.0.try_lock())
            .map_err(|_| scene_err("the scene can't be used inside a closure".into()))
    }
}

/// Keeps a shape at an updater's result; see `Scene.always`.
#[pyclass(frozen, name = "_Always", module = "fastanim")]
struct Always {
    scene: Py<PyScene>,
    m: PyMob,
}

#[pymethods]
impl Always {
    fn __call__(&self, py: Python<'_>, f: Py<PyAny>) -> PyResult<Py<PyAny>> {
        always(self.scene.get(), self.m.clone(), f.clone_ref(py))?;
        Ok(f)
    }
}

fn always(s: &PyScene, m: PyMob, f: Py<PyAny>) -> PyResult<()> {
    s.lock()?
        .always(&m.0, move |st, t| {
            attach(|py| {
                let r = (f.call1(py, (State(st.clone()), t))).map_err(|e| closure_err(py, e))?;
                returned::<Shape>(r.bind(py), "a shape").map(|s| s.0)
            })
        })
        .map_err(scene_err)
}

#[pymethods]
impl PyScene {
    /// Adds a shape, text, axes or group, returning its handle.
    fn add(&self, m: Addable) -> PyResult<PyMob> {
        let mut cx = self.lock()?;
        Ok(PyMob(match m {
            Addable::Shape(s) => cx.add_shape(s.0),
            Addable::Text(t) => cx.add_text(t.0),
            Addable::Axes(a) => cx.add_shape(a.0.shape()),
            Addable::Group(g) => cx.add_group(g.0),
            Addable::Lottie(l) => cx.add_lottie(l.0),
        }))
    }

    fn remove(&self, m: PyMob) -> PyResult<()> {
        self.lock()?.remove(&m.0).map_err(scene_err)
    }

    /// A shape's state now.
    fn get(&self, m: PyMob) -> PyResult<Shape> {
        let cx = self.lock()?;
        (fastanim_script::get(cx.scene.state(), &m.0))
            .map(Shape)
            .map_err(scene_err)
    }

    /// Where the scene is up to, in seconds.
    #[getter]
    fn time(&self) -> PyResult<f32> {
        Ok(self.lock()?.scene.time())
    }

    /// Plays `anims` together.
    #[pyo3(signature = (*anims, run_time = None, rate = None))]
    fn play(&self, anims: Vec<A>, run_time: Option<f32>, rate: Option<PyRate>) -> PyResult<()> {
        let mut a = match <[A; 1]>::try_from(anims) {
            Ok([a]) => a.anim(),
            Err(v) if v.is_empty() => return Err(scene_err("nothing to play".into())),
            Err(v) => fastanim_script::parallel(self::anims(v)),
        };
        a.run_time = run_time.or(a.run_time);
        a.rate = rate.map(|r| r.0).or(a.rate);
        self.lock()?.play(&a).map_err(scene_err)
    }

    #[pyo3(signature = (seconds = 1.0))]
    fn wait(&self, seconds: f32) -> PyResult<()> {
        self.lock()?.scene.wait(seconds);
        Ok(())
    }

    /// Sets the color behind every frame, black by default; one color for the whole scene.
    fn background(&self, color: C) -> PyResult<()> {
        self.lock()?.scene.background(color.0);
        Ok(())
    }

    /// Names this point in time.
    fn marker(&self, name: &str) -> PyResult<()> {
        self.lock()?.scene.marker(name);
        Ok(())
    }

    /// Starts `audio`, or seconds `start..end` of it, without waiting for it.
    #[pyo3(signature = (audio, start = None, end = None))]
    fn add_sound(&self, audio: PyAudio, start: Option<f32>, end: Option<f32>) -> PyResult<()> {
        let range = audio.range(start, end);
        self.lock()?.add_sound(&audio.0, range).map_err(scene_err)
    }

    /// Waits until every sound started so far has finished.
    fn wait_for_sounds(&self) -> PyResult<()> {
        self.lock()?.scene.wait_for_sounds();
        Ok(())
    }

    /// Keeps shape `m` at `f(state, t)` from now on. Without `f`, a decorator.
    #[pyo3(signature = (m, f = None))]
    fn always(slf: &Bound<'_, Self>, m: PyMob, f: Option<Py<PyAny>>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        match f {
            Some(f) => {
                always(slf.get(), m, f)?;
                Ok(py.None())
            }
            None => Ok(Py::new(
                py,
                Always {
                    scene: slf.clone().unbind(),
                    m,
                },
            )?
            .into_any()),
        }
    }
}

/// A baked scene: what playback and export read.
#[pyclass(frozen, name = "Timeline", module = "fastanim")]
pub struct Timeline(pub BakedTimeline);

#[pymethods]
impl Timeline {
    #[getter]
    fn duration(&self) -> f32 {
        self.0.duration()
    }

    /// When marker `name` is, if there is one.
    fn marker(&self, name: &str) -> Option<f32> {
        self.0.marker(name)
    }

    #[getter]
    fn markers(&self) -> Vec<(String, f32)> {
        self.0.markers().to_vec()
    }

    /// The frame at `t` as SVG, over the scene's background unless `background` is given.
    #[pyo3(signature = (t, background = None))]
    fn svg(&self, t: f32, background: Option<C>) -> String {
        let bg = background.map_or(self.0.background(), |c| c.0);
        fastanim_core::to_svg(&self.0.eval(t), bg)
    }

    /// Exports the scene, or from marker `section` to the next one, to `path`; its extension
    /// picks the format: `mp4`, `webm`, `gif`, `png` or `svg` (one file per frame, in the
    /// directory `path`). Only `svg` works without the `preview` extra.
    #[pyo3(signature = (path = PathBuf::from("out.mp4"), *, quality = "1080p60", section = None))]
    fn render(
        &self,
        py: Python<'_>,
        path: PathBuf,
        quality: &str,
        section: Option<&str>,
    ) -> PyResult<()> {
        let q = render::Quality::parse(quality)?;
        let range = render::section(&self.0, section)?;
        render::export(py, &self.0, path, q, range)
    }

    /// Writes the frame at `at` seconds, or frame number `frame` at `quality`, to `path` as
    /// `png` or `svg`. Only `svg` works without the `preview` extra.
    #[pyo3(signature = (path = PathBuf::from("frame.png"), *, at = 0.0, frame = None, quality = "1080p60"))]
    fn still(
        &self,
        py: Python<'_>,
        path: PathBuf,
        at: f32,
        frame: Option<u32>,
        quality: &str,
    ) -> PyResult<()> {
        let ext = (path.extension().and_then(|e| e.to_str())).unwrap_or_default();
        if !["png", "svg"].contains(&ext) {
            return Err(PyValueError::new_err(format!(
                "unsupported still `{}`: use png or svg",
                path.display()
            )));
        }
        let q = render::Quality::parse(quality)?;
        let t = frame.map_or(at, |n| q.frame_time(n));
        render::export(py, &self.0, path, q, (t, t))
    }
}

/// A scene function, made with `@scene`.
#[pyclass(frozen, name = "SceneFn", module = "fastanim")]
struct SceneFn {
    f: Py<PyAny>,
}

#[pymethods]
impl SceneFn {
    /// Runs the scene and bakes what it recorded.
    fn bake(&self, py: Python<'_>) -> PyResult<Timeline> {
        bake(py, &self.f)
    }

    /// Bakes the scene and exports it; see `Timeline.render`.
    #[pyo3(signature = (path = PathBuf::from("out.mp4"), *, quality = "1080p60", section = None))]
    fn render(
        &self,
        py: Python<'_>,
        path: PathBuf,
        quality: &str,
        section: Option<&str>,
    ) -> PyResult<()> {
        bake(py, &self.f)?.render(py, path, quality, section)
    }

    /// Bakes the scene and writes one frame; see `Timeline.still`.
    #[pyo3(signature = (path = PathBuf::from("frame.png"), *, at = 0.0, frame = None, quality = "1080p60"))]
    fn still(
        &self,
        py: Python<'_>,
        path: PathBuf,
        at: f32,
        frame: Option<u32>,
        quality: &str,
    ) -> PyResult<()> {
        bake(py, &self.f)?.still(py, path, at, frame, quality)
    }

    /// Opens a window that plays the scene with a scrubber, and blocks until it closes. With
    /// `watch`, saving the scene's file runs it again. Needs the `preview` extra.
    #[pyo3(signature = (*, watch = true))]
    fn preview(&self, py: Python<'_>, watch: bool) -> PyResult<()> {
        let f = self.f.bind(py);
        let tl = bake_scene(f)?;
        render::preview(py, tl, watch.then(|| render::source(f)).flatten())
    }

    #[getter]
    fn __name__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.f.getattr(py, "__name__")
    }

    #[getter]
    fn __wrapped__(&self, py: Python<'_>) -> Py<PyAny> {
        self.f.clone_ref(py)
    }
}

/// Runs scene function `f` with a fresh scene and bakes it.
fn bake(py: Python<'_>, f: &Py<PyAny>) -> PyResult<Timeline> {
    bake_scene(f.bind(py)).map(Timeline)
}

/// Runs a scene, an `@scene` function or a plain `def construct(scene)`, and bakes what it
/// recorded.
pub fn bake_scene(f: &Bound<'_, PyAny>) -> PyResult<BakedTimeline> {
    if let Ok(s) = f.cast::<SceneFn>() {
        return bake_scene(s.get().f.bind(f.py()));
    }
    let ctx = Arc::new(Mutex::new(Ctx::new()));
    f.call1((PyScene(ctx.clone()),))?;
    // Taken rather than unwrapped: closures in the timeline may still hold the scene.
    let scene = std::mem::take(&mut ctx.lock().unwrap().scene);
    Ok(scene.bake())
}

/// Marks `f(scene)` as a scene: `f.bake()`, `f.still(path, at=t)`, `f.render(path)`,
/// `f.preview()`.
#[pyfunction]
fn scene(f: Py<PyAny>) -> SceneFn {
    SceneFn { f }
}

// Free functions, named as in Rust and Rhai.

#[pyfunction]
fn point(x: f64, y: f64) -> PyPoint {
    PyPoint(Vec2::new(x, y))
}

#[pyfunction]
#[pyo3(signature = (r, g, b, a = 1.0))]
fn rgb(r: f32, g: f32, b: f32, a: f32) -> PyColor {
    PyColor(CoreColor::rgba(r, g, b, a))
}

/// A color from `0xRRGGBB`.
#[pyfunction]
fn hex(rgb: u32) -> PyColor {
    PyColor(CoreColor::hex(rgb))
}

macro_rules! shapes {
    ($($(#[$doc:meta])* fn $name:ident($($arg:ident: $ty:ty),*) => $body:expr;)*) => {$(
        $(#[$doc])*
        #[pyfunction]
        #[pyo3(signature = (
            $($arg,)* *, stroke = None, stroke_width = None, fill = None, fill_opacity = None
        ))]
        fn $name(
            $($arg: $ty,)*
            stroke: Option<C>,
            stroke_width: Option<f64>,
            fill: Option<C>,
            fill_opacity: Option<f32>,
        ) -> PyResult<Shape> {
            Ok(Shape(styled($body, stroke, stroke_width, fill, fill_opacity)))
        }
    )*};
}

shapes! {
    fn circle(radius: f64) => VState::circle(radius);
    fn square(side: f64) => VState::square(side);
    fn rectangle(width: f64, height: f64) => VState::rectangle(width, height);
    fn ellipse(width: f64, height: f64) => VState::ellipse(width, height);
    /// The area covered by either shape.
    fn union(a: Shape, b: Shape) => a.0.boolean(&b.0, BinaryOp::Union);
    /// The area covered by both shapes.
    fn intersection(a: Shape, b: Shape) => a.0.boolean(&b.0, BinaryOp::Intersection);
    /// The area covered by `a` but not `b`.
    fn difference(a: Shape, b: Shape) => a.0.boolean(&b.0, BinaryOp::Difference);
    /// The area covered by exactly one of the shapes.
    fn exclusion(a: Shape, b: Shape) => a.0.boolean(&b.0, BinaryOp::Xor);
    /// An arc of a circle, from angle `start` sweeping `sweep` radians.
    fn arc(radius: f64, start: f64, sweep: f64) => VState::arc(radius, start, sweep);
    fn line(a: P, b: P) => VState::line(a.pt(), b.pt());
    /// A small filled dot at `p`.
    fn dot(p: P) => VState::dot(p.pt());
    fn polygon(points: Vec<P>) => VState::polygon(&points.iter().map(P::pt).collect::<Vec<_>>());
    /// Straight segments through `points`, left open.
    fn polyline(points: Vec<P>) => VState::polyline(&points.iter().map(P::pt).collect::<Vec<_>>());
}

/// A dashed line from `a` to `b`, with dashes about `dash_length` long and `ratio` of each
/// period.
#[pyfunction]
#[pyo3(signature = (
    a, b, dash_length = 0.05, ratio = 0.5, *, stroke = None, stroke_width = None
))]
fn dashed_line(
    a: P,
    b: P,
    dash_length: f64,
    ratio: f64,
    stroke: Option<C>,
    stroke_width: Option<f64>,
) -> Shape {
    let m = VState::dashed_line(a.pt(), b.pt(), dash_length, ratio);
    Shape(styled(m, stroke, stroke_width, None, None))
}

/// The graph of `f` from `x0` to `x1`, with `segments` cubic pieces.
#[pyfunction]
fn function_graph(
    py: Python<'_>,
    f: Py<PyAny>,
    x0: f64,
    x1: f64,
    segments: usize,
) -> PyResult<Shape> {
    if segments < 1 {
        return Err(PyValueError::new_err("segments must be at least 1"));
    }
    graph(py, &f, |g| VState::function_graph(g, x0..x1, segments))
}

/// An image: a PNG or JPEG file's path, or rows of pixels (a NumPy array or nested lists), each
/// a grey value or `(r, g, b[, a])` from 0 to 255. As tall as manim makes it unless `height` or
/// `width` is given; `pixelated` keeps pixels sharp when scaled up.
#[pyfunction]
#[pyo3(signature = (src, *, height = None, width = None, pixelated = false))]
fn image(
    src: &Bound<'_, PyAny>,
    height: Option<f64>,
    width: Option<f64>,
    pixelated: bool,
) -> PyResult<Shape> {
    let m = if let Ok(path) = src.extract::<PathBuf>() {
        fastanim_script::image(&path).map_err(scene_err)?
    } else {
        let rows = if src.hasattr("tolist")? {
            src.call_method0("tolist")?
        } else {
            src.clone()
        };
        let bad = || {
            PyTypeError::new_err(format!(
                "expected a path or rows of pixels, got {}",
                type_name(src)
            ))
        };
        let rows: Vec<Vec<Bound<'_, PyAny>>> = rows.extract().map_err(|_| bad())?;
        let rows = (rows.iter())
            .map(|row| {
                (row.iter())
                    .map(|p| match p.extract::<f64>() {
                        Ok(v) => Ok(vec![v]),
                        Err(_) => p.extract::<Vec<f64>>().map_err(|_| bad()),
                    })
                    .collect::<PyResult<Vec<_>>>()
            })
            .collect::<PyResult<Vec<_>>>()?;
        fastanim_script::image_from_rows(&rows).map_err(PyValueError::new_err)?
    };
    let m = match (height, width) {
        (Some(h), None) => m.scale_to_fit_height(h),
        (None, Some(w)) => m.scale_to_fit_width(w),
        (None, None) => m,
        (Some(_), Some(_)) => {
            return Err(PyValueError::new_err("give height or width, not both"));
        }
    };
    Ok(Shape(if pixelated { m.pixelated() } else { m }))
}

/// A Lottie (Bodymovin JSON) file's animation, showing its first frame until played with
/// `play_lottie`. As tall as an image of its size unless `height` or `width` is given.
#[pyfunction]
#[pyo3(signature = (path, *, height = None, width = None))]
fn lottie(path: PathBuf, height: Option<f64>, width: Option<f64>) -> PyResult<PyLottie> {
    let l = fastanim_script::lottie(&path).map_err(scene_err)?;
    Ok(PyLottie(match (height, width) {
        (Some(h), None) => l.scale_to_fit_height(h),
        (None, Some(w)) => l.scale_to_fit_width(w),
        (None, None) => l,
        (Some(_), Some(_)) => {
            return Err(PyValueError::new_err("give height or width, not both"));
        }
    }))
}

/// Loads the fonts in the TTF, OTF, TTC or OTC file at `path`, relative to the current
/// directory, for `text(..., font=...)`; returns their family names.
#[pyfunction]
fn load_font(path: PathBuf) -> PyResult<Vec<String>> {
    fastanim_script::load_font(&path).map_err(scene_err)
}

/// Typeset Typst text, optionally in font family `font`.
#[pyfunction]
#[pyo3(signature = (s, *, font = None))]
fn text(s: &str, font: Option<String>) -> PyResult<Text> {
    (TextMobject::text_styled(s, &TextStyle { font }))
        .map(Text)
        .map_err(typeset_err)
}

/// Typeset Typst math, e.g. `"a^2 + b^2 = c^2"`.
#[pyfunction]
#[pyo3(signature = (s, *, font = None))]
fn math_tex(s: &str, font: Option<String>) -> PyResult<Text> {
    (TextMobject::math_styled(s, &TextStyle { font }))
        .map(Text)
        .map_err(typeset_err)
}

/// Typeset LaTeX math.
#[pyfunction]
#[pyo3(signature = (s, *, font = None))]
fn latex(s: &str, font: Option<String>) -> PyResult<Text> {
    (TextMobject::latex_styled(s, &TextStyle { font }))
        .map(Text)
        .map_err(typeset_err)
}

/// Highlighted source code.
#[pyfunction]
#[pyo3(signature = (s, lang, *, font = None))]
fn code(s: &str, lang: &str, font: Option<String>) -> PyResult<Text> {
    (TextMobject::code_styled(s, lang, &TextStyle { font }))
        .map(Text)
        .map_err(typeset_err)
}

/// A row of values, one cell each, for `transform_list`.
#[pyfunction]
fn list(values: Vec<Bound<'_, PyAny>>) -> PyResult<Text> {
    let v = (values.iter())
        .map(|v| Ok(v.str()?.to_string()))
        .collect::<PyResult<Vec<_>>>()?;
    Ok(Text(fastanim_text::list(&v)))
}

/// Axes over `x_range` and `y_range` (`(min, max[, step])`), optionally sized.
#[pyfunction]
#[pyo3(signature = (x_range, y_range, width = None, height = None))]
fn axes(
    x_range: Vec<f64>,
    y_range: Vec<f64>,
    width: Option<f64>,
    height: Option<f64>,
) -> PyResult<PyAxes> {
    let (x, y) = (
        fastanim_script::range(&x_range).map_err(PyValueError::new_err)?,
        fastanim_script::range(&y_range).map_err(PyValueError::new_err)?,
    );
    Ok(PyAxes(match (width, height) {
        (None, None) => CoreAxes::new(x, y),
        (Some(w), Some(h)) => CoreAxes::sized(x, y, w, h),
        _ => {
            return Err(PyValueError::new_err(
                "give both width and height, or neither",
            ));
        }
    }))
}

fn ease(name: &str) -> PyResult<fastanim_core::Ease> {
    fastanim_script::ease(name).map_err(PyValueError::new_err)
}

#[pyfunction]
fn ease_in(name: &str) -> PyResult<PyRate> {
    Ok(PyRate(RateFn::EaseIn(ease(name)?)))
}

#[pyfunction]
fn ease_out(name: &str) -> PyResult<PyRate> {
    Ok(PyRate(RateFn::EaseOut(ease(name)?)))
}

#[pyfunction]
fn ease_in_out(name: &str) -> PyResult<PyRate> {
    Ok(PyRate(RateFn::EaseInOut(ease(name)?)))
}

#[pyfunction]
fn spring(stiffness: f32, damping: f32) -> PyRate {
    PyRate(RateFn::Spring { stiffness, damping })
}

macro_rules! simple {
    ($($name:ident),*) => {$(
        #[pyfunction]
        fn $name(m: PyMob) -> PyAnim {
            PyAnim(fastanim_script::$name(m.0))
        }
    )*};
}

simple!(
    create,
    uncreate,
    grow_from_center,
    shrink_to_center,
    spin_in,
    draw_border_then_fill,
    write,
    unwrite,
    indicate,
    wiggle,
    circumscribe
);

#[pyfunction]
#[pyo3(signature = (m, shift = None, scale = None))]
fn fade_in(m: PyMob, shift: Option<P>, scale: Option<f64>) -> PyAnim {
    PyAnim(match (shift, scale) {
        (None, None) => fastanim_script::fade_in(m.0),
        (v, k) => {
            fastanim_script::fade_in_with(m.0, v.map_or(Vec2::ZERO, |v| v.0), k.unwrap_or(1.0))
        }
    })
}

#[pyfunction]
#[pyo3(signature = (m, shift = None, scale = None))]
fn fade_out(m: PyMob, shift: Option<P>, scale: Option<f64>) -> PyAnim {
    PyAnim(match (shift, scale) {
        (None, None) => fastanim_script::fade_out(m.0),
        (v, k) => {
            fastanim_script::fade_out_with(m.0, v.map_or(Vec2::ZERO, |v| v.0), k.unwrap_or(1.0))
        }
    })
}

#[pyfunction(name = "shift")]
fn shift_anim(m: PyMob, v: P) -> PyAnim {
    PyAnim(fastanim_script::shift(m.0, v.0))
}

#[pyfunction(name = "rotate")]
fn rotate_anim(m: PyMob, angle: f64) -> PyAnim {
    PyAnim(fastanim_script::rotate(m.0, angle))
}

#[pyfunction(name = "scale")]
fn scale_anim(m: PyMob, k: f64) -> PyAnim {
    PyAnim(fastanim_script::scale(m.0, k))
}

#[pyfunction(name = "move_to")]
fn move_to_anim(m: PyMob, p: P) -> PyAnim {
    PyAnim(fastanim_script::move_to(m.0, p.pt()))
}

#[pyfunction]
fn flash(p: P) -> PyAnim {
    PyAnim(fastanim_script::flash(p.pt()))
}

/// Morphs shape `m` into `target`.
#[pyfunction]
fn transform(m: PyMob, target: Shape) -> PyAnim {
    PyAnim(fastanim_script::transform(m.0, target.0))
}

#[pyfunction]
fn replacement_transform(m: PyMob, target: PyMob) -> PyAnim {
    PyAnim(fastanim_script::replacement_transform(m.0, target.0))
}

/// Moves every point of shape `m` through `f(point) -> point`.
#[pyfunction]
fn apply_function(m: PyMob, f: Py<PyAny>) -> PyAnim {
    PyAnim(fastanim_script::apply_function(m.0, move |p| {
        attach(|py| {
            let r = (f.call1(py, (PyPoint(p.to_vec2()),))).map_err(|e| closure_err(py, e))?;
            returned::<P>(r.bind(py), "a point").map(|p| p.pt())
        })
    }))
}

/// Sets shape `m` to `f(start, alpha)` while playing.
#[pyfunction]
fn update(m: PyMob, f: Py<PyAny>) -> PyAnim {
    PyAnim(fastanim_script::update(m.0, move |s, a| {
        attach(|py| {
            let r = (f.call1(py, (Shape(s.clone()), a))).map_err(|e| closure_err(py, e))?;
            returned::<Shape>(r.bind(py), "a shape").map(|s| s.0)
        })
    }))
}

fn diff_style(opts: Option<&Bound<'_, PyDict>>) -> PyResult<DiffStyle> {
    let mut style = DiffStyle::default();
    for (k, v) in opts.into_iter().flatten() {
        let k: String = k.extract()?;
        // Checked first: Python bools are ints, so they would pass as numbers.
        let v = match (v.extract::<bool>(), v.extract::<f64>()) {
            (Ok(b), _) => Opt::Bool(b),
            (_, Ok(n)) => Opt::Num(n),
            _ => {
                let msg = format!("`{k}` can't be {}", type_name(&v));
                return Err(PyTypeError::new_err(msg));
            }
        };
        fastanim_script::diff_option(&mut style, &k, v).map_err(PyValueError::new_err)?;
    }
    Ok(style)
}

/// Morphs text `m` into `to`, moving unchanged tokens. Options: `highlight_changes`, `debug`,
/// `lag_ratio`, `move_arc`.
#[pyfunction]
#[pyo3(signature = (m, to, **opts))]
fn transform_diff(m: PyMob, to: Text, opts: Option<&Bound<'_, PyDict>>) -> PyResult<PyAnim> {
    Ok(PyAnim(fastanim_script::transform_diff(
        m.0,
        to.0,
        diff_style(opts)?,
        false,
    )))
}

/// Plays Lottie `m` at its own frame rate: frames `start` to `end` (the whole file by default;
/// backwards if `end < start`), `loops` times over.
#[pyfunction]
#[pyo3(signature = (m, *, start = None, end = None, loops = None))]
fn play_lottie(
    m: PyMob,
    start: Option<f64>,
    end: Option<f64>,
    loops: Option<u32>,
) -> PyResult<PyAnim> {
    if loops == Some(0) {
        return Err(PyValueError::new_err("`loops` must be at least 1"));
    }
    let how = fastanim_script::LottiePlay { start, end, loops };
    Ok(PyAnim(fastanim_script::play_lottie(m.0, how)))
}

/// Morphs list `m` into list `to`, moving cells by value.
#[pyfunction]
fn transform_list(m: PyMob, to: Text) -> PyAnim {
    PyAnim(fastanim_script::transform_diff(
        m.0,
        to.0,
        DiffStyle::default(),
        true,
    ))
}

#[pyfunction]
#[pyo3(signature = (*anims))]
fn parallel(anims: Vec<A>) -> PyAnim {
    PyAnim(fastanim_script::parallel(self::anims(anims)))
}

#[pyfunction]
#[pyo3(signature = (*anims))]
fn sequence(anims: Vec<A>) -> PyAnim {
    PyAnim(fastanim_script::sequence(self::anims(anims)))
}

/// Starts each of `anims` `lag_ratio` of the way through the one before.
#[pyfunction]
#[pyo3(signature = (*anims, lag_ratio))]
fn lagged_start(anims: Vec<A>, lag_ratio: f32) -> PyAnim {
    PyAnim(fastanim_script::lagged_start(lag_ratio, self::anims(anims)))
}

/// Loads the WAV at `path`, relative to the current directory.
#[pyfunction]
fn audio(path: std::path::PathBuf) -> PyResult<PyAudio> {
    (fastanim_script::audio(&path))
        .map(PyAudio)
        .map_err(scene_err)
}

/// Plays `audio`, or seconds `start..end` of it, lasting until it finishes.
#[pyfunction]
#[pyo3(signature = (audio, start = None, end = None))]
fn sound(audio: PyAudio, start: Option<f32>, end: Option<f32>) -> PyResult<PyAnim> {
    (fastanim_script::sound(&audio.0, audio.range(start, end)))
        .map(PyAnim)
        .map_err(scene_err)
}

fn constant(py: Python<'_>, c: Const) -> PyResult<Py<PyAny>> {
    Ok(match c {
        Const::Point(v) => Py::new(py, PyPoint(v))?.into_any(),
        Const::Color(c) => Py::new(py, PyColor(c))?.into_any(),
        Const::Rate(r) => Py::new(py, PyRate(r))?.into_any(),
        Const::Num(n) => n.into_pyobject(py)?.into_any().unbind(),
    })
}

/// The native module.
#[pymodule]
pub fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    m.add("SceneError", py.get_type::<SceneError>())?;
    m.add("TypesetError", py.get_type::<TypesetError>())?;
    m.add_class::<PyPoint>()?;
    m.add_class::<PyColor>()?;
    m.add_class::<PyRate>()?;
    m.add_class::<Shape>()?;
    m.add_class::<Text>()?;
    m.add_class::<PyAxes>()?;
    m.add_class::<PyAudio>()?;
    m.add_class::<PyLottie>()?;
    m.add_class::<PyGroup>()?;
    m.add_class::<PyMob>()?;
    m.add_class::<Animate>()?;
    m.add_class::<PyAnim>()?;
    m.add_class::<State>()?;
    m.add_class::<PyScene>()?;
    m.add_class::<Timeline>()?;
    m.add_class::<SceneFn>()?;
    for name in fastanim_script::CONSTANTS {
        let c = fastanim_script::constant(name).expect("every listed constant exists");
        m.add(*name, constant(py, c)?)?;
    }

    macro_rules! add {
        ($($f:ident),* $(,)?) => {$(m.add_function(wrap_pyfunction!($f, m)?)?;)*};
    }
    add!(
        scene,
        point,
        rgb,
        hex,
        circle,
        ellipse,
        union,
        intersection,
        difference,
        exclusion,
        square,
        rectangle,
        arc,
        line,
        dot,
        polygon,
        polyline,
        dashed_line,
        function_graph,
        image,
        lottie,
        load_font,
        text,
        math_tex,
        latex,
        code,
        list,
        group,
        axes,
        fit_axes,
        ease_in,
        ease_out,
        ease_in_out,
        spring,
        create,
        uncreate,
        fade_in,
        fade_out,
        grow_from_center,
        shrink_to_center,
        spin_in,
        draw_border_then_fill,
        write,
        unwrite,
        indicate,
        wiggle,
        circumscribe,
        shift_anim,
        rotate_anim,
        scale_anim,
        move_to_anim,
        flash,
        transform,
        replacement_transform,
        apply_function,
        update,
        transform_diff,
        transform_list,
        play_lottie,
        parallel,
        sequence,
        lagged_start,
        audio,
        sound,
    );
    Ok(())
}
