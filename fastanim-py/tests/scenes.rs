//! The example scenes bake to the same timelines as their Rust or Rhai versions (SPEC §15.7), and bad
//! scenes fail with Python exceptions. Runs the package in an embedded interpreter, so it needs
//! no wheel.

use fastanim_core::color::BLACK;
use fastanim_core::{BakedTimeline, Scene, to_svg};
use pyo3::prelude::*;
use pyo3::types::PyDict;

#[allow(dead_code)]
#[path = "../../fastanim-bevy/examples/diff.rs"]
mod diff;
#[allow(dead_code)]
#[path = "../../fastanim-bevy/examples/parity.rs"]
mod parity;

fn rust(construct: fn(&mut Scene)) -> BakedTimeline {
    let mut s = Scene::new();
    construct(&mut s);
    s.bake()
}

fn attach<R>(f: impl for<'py> FnOnce(Python<'py>) -> R) -> R {
    Python::initialize();
    Python::attach(f)
}

/// Imports the `fastanim` package from `python/`, with the native module from this crate.
fn init(py: Python<'_>) -> PyResult<()> {
    let modules = py.import("sys")?.getattr("modules")?;
    if modules.contains("fastanim")? {
        return Ok(());
    }
    let native = pyo3::wrap_pymodule!(fastanim_py::_native)(py);
    modules.set_item("fastanim._native", native)?;
    let path = py.import("sys")?.getattr("path")?;
    path.call_method1(
        "insert",
        (0, concat!(env!("CARGO_MANIFEST_DIR"), "/python")),
    )?;
    py.import("fastanim")?;
    Ok(())
}

/// Runs `src` and bakes its scene called `name`.
fn bake_src(py: Python<'_>, src: &str, name: &str) -> PyResult<BakedTimeline> {
    init(py)?;
    let globals = PyDict::new(py);
    py.run(&std::ffi::CString::new(src).unwrap(), Some(&globals), None)?;
    fastanim_py::bake_scene(&globals.get_item(name)?.unwrap())
}

fn script(name: &str) -> BakedTimeline {
    let path = format!("{}/scenes/{name}.py", env!("CARGO_MANIFEST_DIR"));
    let src = std::fs::read_to_string(&path).unwrap();
    attach(|py| bake_src(py, &src, name).unwrap_or_else(|e| panic!("{path}: {e}")))
}

fn assert_same(a: &BakedTimeline, b: &BakedTimeline) {
    assert_eq!(a.duration(), b.duration());
    assert_eq!(a.markers(), b.markers());
    let n = (a.duration() * 10.0) as usize;
    for t in (0..=n).map(|i| i as f32 / 10.0) {
        assert!(
            to_svg(&a.eval(t), BLACK) == to_svg(&b.eval(t), BLACK),
            "differs at {t}s"
        );
    }
}

#[test]
fn parity_matches_rust() {
    assert_same(&rust(parity::construct), &script("parity"));
}

#[test]
fn diff_matches_rust() {
    assert_same(&rust(diff::construct), &script("diff"));
}

/// The Rhai version of an example with no Rust one.
fn rhai(name: &str) -> BakedTimeline {
    let path = format!(
        "{}/../fastanim-script/scenes/{name}.rhai",
        env!("CARGO_MANIFEST_DIR")
    );
    fastanim_script::bake(&std::fs::read_to_string(&path).unwrap())
        .unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn sin_and_cos_matches_rhai() {
    assert_same(&rhai("sin_and_cos"), &script("sin_and_cos"));
}

#[test]
fn sine_curve_matches_rhai() {
    assert_same(&rhai("sine_curve"), &script("sine_curve"));
}

/// The exception `src`'s scene `s` raises, as `Type: message`.
fn err(src: &str) -> String {
    let src = format!("from fastanim import *\n@scene\ndef s(s):\n{}", indent(src));
    attach(|py| match bake_src(py, &src, "s") {
        Ok(_) => panic!("expected an error from {src:?}"),
        Err(e) => format!("{}: {e}", e.get_type(py).name().unwrap()),
    })
}

fn indent(s: &str) -> String {
    s.lines().map(|l| format!("    {l}\n")).collect()
}

#[test]
fn errors_are_python_exceptions() {
    let e = err("c = s.add(circle(1))\ns.remove(c)\ns.play(create(c))");
    assert!(
        e.starts_with("SceneError") && e.contains("not in the scene"),
        "{e}"
    );

    let e = err("s.add(math_tex('a^'))");
    assert!(e.starts_with("TypesetError"), "{e}");

    let e = err("t = s.add(text('x'))\ns.always(t, lambda st, t: st[t])");
    assert!(e.contains("expected a shape"), "{e}");

    let e = err("c = s.add(circle(1))\ns.always(c, lambda st, t: 1)");
    assert!(e.contains("not a shape"), "{e}");

    // The closure's own exception and line, not a hang or a deadlock.
    let e = err("c = s.add(circle(1))\ns.play(apply_function(c, lambda p: p.z))");
    assert!(e.contains("AttributeError") && e.contains("line 5"), "{e}");
    let e = err("c = s.add(circle(1))\ns.play(update(c, lambda m, a: s.get(c)))");
    assert!(e.contains("inside a closure"), "{e}");

    let e = err("s.play(transform_diff(s.add(text('a')), text('b'), nope=1))");
    assert!(e.contains("unknown option `nope`"), "{e}");
}

#[test]
fn pythonic_api() {
    let src = r##"
from fastanim import *

@scene
def s(s):
    assert point(1, 2) + (1, 1) == point(2, 3)
    x, y = point(3, 4)
    assert (x, y) == (3, 4) and abs(point(3, 4)) == 5
    eq = s.add(math_tex("a^2 + b^2 = c^2").scale(1.5))
    s.play(eq.animate.shift(UP).scale(0.8), rate=SMOOTH)
    c = s.add(circle(1, stroke="#ffcc00", fill=(1, 0, 0, 0.5)))
    s.play(c.animate.move_to((2, 0)), run_time=2)
    assert s.get(c).move_to(ORIGIN) is not None
    s.play(lagged_start(fade_out(c), fade_out(eq), lag_ratio=0.2))
    s.marker("end")
"##;
    let tl = attach(|py| bake_src(py, src, "s")).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(tl.marker("end"), Some(tl.duration()));
    assert!(tl.duration() > 3.0);
}

#[test]
fn fit_axes_and_default_plot_range() {
    // x² over [-3, 3] fits y to [0, 10], so y = 10 is the top of 6-tall axes.
    let src = r##"
from fastanim import *

@scene
def s(s):
    ax = fit_axes(lambda x: x * x, (-3, 3))
    assert ax.c2p(0, 10) == point(0, 3)
    s.add(ax.plot(lambda x: x * x))
    s.add(ax.plot(lambda x: x, 0, 1, segments=8))
"##;
    attach(|py| bake_src(py, src, "s")).unwrap_or_else(|e| panic!("{e}"));
}
