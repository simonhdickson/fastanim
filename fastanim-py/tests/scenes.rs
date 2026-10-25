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
    assert_eq!(a.background(), b.background());
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

#[test]
fn moving_group_matches_rhai() {
    assert_same(&rhai("moving_group"), &script("moving_group"));
}

#[test]
fn gradient_image_matches_rhai() {
    assert_same(&rhai("gradient_image"), &script("gradient_image"));
}

#[test]
fn square_to_circle_matches_rhai() {
    assert_same(&rhai("square_to_circle"), &script("square_to_circle"));
}

#[test]
fn boolean_operations_matches_rhai() {
    assert_same(&rhai("boolean_operations"), &script("boolean_operations"));
}

#[test]
fn styles_matches_rhai() {
    assert_same(&rhai("styles"), &script("styles"));
}

#[test]
fn images_and_fonts() {
    run(r##"
from fastanim import *

def raises(kind, text, f, *args, **kwargs):
    try:
        f(*args, **kwargs)
    except kind as e:
        assert text in str(e), e
    else:
        raise AssertionError(f"expected {kind.__name__}")

class Array:
    """Stands in for a NumPy array, which becomes nested lists."""
    def tolist(self):
        return [[0, 255], [(255, 0, 0), (0, 0, 255, 128)]]

a = image(Array(), height=2, pixelated=True)
assert abs(a.center()) < 1e-9
b = image([[1, 2, 3]]).scale_to_fit_width(3)
assert b.next_to(a, RIGHT).center().x > 1

@scene
def s(s):
    m = s.add(a)
    s.play(rotate(m, 1))
assert s.bake().svg(0).count("<image") == 1

raises(TypeError, "expected a path or rows of pixels", image, 42)
raises(ValueError, "row 1 has 1 pixels", image, [[1, 2], [3]])
raises(ValueError, "not both", image, [[1]], height=1, width=1)
raises(SceneError, "missing.png", image, "missing.png")

text("mono", font="DejaVu Sans Mono")
code("let x = 1;", "rust", font="DejaVu Sans Mono")
raises(TypesetError, "load_font", text, "x", font="No Such Font")
raises(SceneError, "no/such/font.ttf", load_font, "no/such/font.ttf")
"##);
}

#[test]
fn videos() {
    let dir = std::env::temp_dir().join(format!("fastanim-py-video-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("clip.mp4");
    let made = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=d=1:s=64x48:r=10",
        ])
        .args(["-f", "lavfi", "-i", "sine=d=1", "-pix_fmt", "yuv420p"])
        .arg(&path)
        .status();
    if !made.is_ok_and(|s| s.success()) {
        eprintln!("skipped: no ffmpeg");
        return;
    }
    run(&r##"
from fastanim import *

def raises(kind, text, f, *args, **kwargs):
    try:
        f(*args, **kwargs)
    except kind as e:
        assert text in str(e), e
    else:
        raise AssertionError(f"expected {kind.__name__}")

v = video(PATH, height=3, resolution=24)
assert abs(v.height - 3) < 1e-9

@scene
def s(s):
    m = s.add(v.rotate(0.5))
    s.play(rotate(m, 1), play_video(m, loops=2))
    s.play(play_video(m, start=0.5, end=0))
assert s.bake().duration == 2.5

raises(SceneError, "missing.mp4", video, "missing.mp4")
raises(ValueError, "not both", video, PATH, height=1, width=1)
"##
    .replace("PATH", &format!("{path:?}")));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn lotties() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fastanim-script/scenes/bounce.json"
    );
    run(&r##"
from fastanim import *

def raises(kind, text, f, *args, **kwargs):
    try:
        f(*args, **kwargs)
    except kind as e:
        assert text in str(e), e
    else:
        raise AssertionError(f"expected {kind.__name__}")

l = lottie(PATH, height=4)
assert abs(l.center()) < 1e-9
assert l.duration == 2
assert l.next_to(circle(1), RIGHT).center().x > 1

@scene
def s(s):
    m = s.add(l.at_frame(30).shift(RIGHT))
    s.play(play_lottie(m, loops=2))
    s.play(fade_out(m), play_lottie(m, start=30, end=0))
assert s.bake().duration == 5

raises(SceneError, "missing.json", lottie, "missing.json")
raises(ValueError, "not both", lottie, PATH, height=1, width=1)
"##
    .replace("PATH", &format!("{path:?}")));
    let e = err("c = s.add(circle(1))\ns.play(play_lottie(c))");
    assert!(e.contains("expected a Lottie"), "{e}");
    let e = err(&format!(
        "l = s.add(lottie({path:?}))\ns.play(play_lottie(l, loops=0))"
    ));
    assert!(e.contains("ValueError") && e.contains("loops"), "{e}");
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

    let e = err("g = s.add(group(circle(1), text('a')))\ns.play(transform(g, circle(2)))");
    assert!(e.contains("got a group"), "{e}");
    let e = err("g = s.add(group(circle(1)))\ng[1]");
    assert!(
        e.starts_with("SceneError") && e.contains("out of range"),
        "{e}"
    );
    let e = err("group(s.add(circle(1)), circle(1))");
    assert!(
        e.starts_with("TypeError") && e.contains("all mobjects"),
        "{e}"
    );

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

/// Runs `src` with the package imported.
fn run(src: &str) {
    attach(|py| {
        init(py)?;
        let globals = PyDict::new(py);
        py.run(&std::ffi::CString::new(src).unwrap(), Some(&globals), None)
    })
    .unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn stubs_match_the_module() {
    // `_native.pyi` names every public function, class, method and constant, and nothing else.
    run(&format!(
        r##"
import ast, inspect
from fastanim import _native

stub = ast.parse(open("{}/python/fastanim/_native.pyi").read())
public = lambda names: {{n for n in names if not n.startswith("_")}}

def names(body):
    out = set()
    for node in body:
        if isinstance(node, (ast.FunctionDef, ast.ClassDef)):
            out.add(node.name)
        elif isinstance(node, ast.AnnAssign):
            out.add(node.target.id)
    return out

classes = {{c.name: c for c in stub.body if isinstance(c, ast.ClassDef)}}
assert public(names(stub.body)) == public(dir(_native)), \
    public(names(stub.body)) ^ public(dir(_native))
base = names(classes["_Positioned"].body)
for name, c in classes.items():
    if name.startswith("_") or issubclass(getattr(_native, name), BaseException):
        continue
    stubbed = public(names(c.body) | (base if c.bases else set()))
    actual = public(vars(getattr(_native, name)))
    assert stubbed == actual, (name, stubbed ^ actual)
"##,
        env!("CARGO_MANIFEST_DIR")
    ));
}

#[test]
fn exports_svg() {
    // Without the `preview` feature, SVG is the one format; with it, Bevy writes the same files.
    run(r##"
import os, tempfile
from fastanim import *

@scene
def s(s):
    c = s.add(circle(1))
    s.play(create(c), run_time=1)
    s.marker("fade")
    s.play(fade_out(c), run_time=0.6)

out = tempfile.mkdtemp()
s.still(os.path.join(out, "a.svg"), at=0.4)
s.still(os.path.join(out, "b.svg"), frame=6, quality="480p15")
a, b = (open(os.path.join(out, f)).read() for f in ("a.svg", "b.svg"))
assert a == b == s.bake().svg(0.4), "a still is the frame at `at`, or frame / fps"

# 1 s to 1.6 s at 15 fps: frames 15 to 24.
s.render(os.path.join(out, "frames.svg"), quality="480p15", section="fade")
frames = sorted(os.listdir(os.path.join(out, "frames.svg")))
assert frames == [f"{i:04}.svg" for i in range(10)], frames

def raises(kind, text, f, *args, **kwargs):
    try:
        f(*args, **kwargs)
    except kind as e:
        assert text in str(e), e
    else:
        raise AssertionError(f"expected {kind.__name__}")

raises(ValueError, "unknown quality", s.render, os.path.join(out, "x.svg"), quality="2k")
raises(SceneError, "no marker `nope`", s.render, os.path.join(out, "x.svg"), section="nope")
raises(ValueError, "unsupported output", s.render, os.path.join(out, "x.avi"))
raises(ValueError, "unsupported still", s.still, os.path.join(out, "x.mp4"))
"##);
}

#[test]
fn frame_draw_list() {
    // What the browser's Python player draws (fastanim-web/pyplayer.js).
    run(r##"
from fastanim import *

@scene
def s(s):
    s.background(hex(0x102030))
    s.add(square(2).fill(RED, 0.5).z_index(1))
    b = s.add(circle(1))
    gone = s.add(circle(2))
    s.add(image([[0, 255], [255, 0]]))
    s.play(create(b), fade_out(gone), run_time=1)

tl = s.bake()
assert tl.background_css == "rgba(16,32,48,1)", tl.background_css
assert [(e["kind"], e["label"]) for e in tl.events] == [("play", "Create + FadeOut")], tl.events
f = tl.frame(1)
assert len(f) == 3, "the faded circle is left out"
assert f[-1]["fill"].endswith(",0.5)"), ("z order, back to front", f[-1])
img = next(m["image"] for m in f if m["image"])
w, h, rgba = tl.image(img[0])
assert (w, h, len(rgba)) == (2, 2, 16), (w, h, len(rgba))
assert tl.image(1) is None
"##);
}

#[cfg(feature = "preview")]
#[test]
fn jupyter_shows_an_inline_video() {
    run(r##"
from fastanim import *

@scene
def s(s):
    s.play(create(s.add(circle(1))), run_time=0.5)

for html in (s._repr_html_(), s.bake()._repr_html_()):
    assert html.startswith('<video controls') and "data:video/mp4;base64," in html, html[:200]
"##);
}

#[cfg(not(feature = "preview"))]
#[test]
fn rendering_needs_the_preview_feature() {
    run(r##"
from fastanim import *

@scene
def s(s):
    s.add(circle(1))

for f in (lambda: s.render("out.mp4"), lambda: s.still("frame.png"), s.preview):
    try:
        f()
    except RuntimeError as e:
        assert "only svg output works" in str(e), e
    else:
        raise AssertionError("expected RuntimeError")
"##);
}

#[test]
fn command_line() {
    run(&format!(
        r##"
import contextlib, io, os, tempfile
from fastanim.__main__ import main

scenes = "{dir}/scenes"
out = os.path.join(tempfile.mkdtemp(), "f.svg")
assert main(["still", f"{{scenes}}/parity.py", "--at", "2s", "-o", out]) == 0
assert os.path.isfile(out)
assert main(["still", f"{{scenes}}/diff.py:diff", "--frame", "3", "-o", out]) == 0

err = io.StringIO()
with contextlib.redirect_stderr(err):
    assert main(["still", f"{{scenes}}/parity.py:nope", "-o", out]) == 1
assert "no scene `nope` (scenes: parity)" in err.getvalue(), err.getvalue()
"##,
        dir = env!("CARGO_MANIFEST_DIR")
    ));
}

#[test]
fn reloading_reimports_the_scene_files_neighbours() {
    run(r##"
import os, tempfile
from fastanim._load import load, watched

d = tempfile.mkdtemp()
scene_py, helper_py = os.path.join(d, "s.py"), os.path.join(d, "helper_for_reload.py")
with open(scene_py, "w") as f:
    f.write("from fastanim import *\nfrom helper_for_reload import R\n"
            "@scene\ndef s(s):\n    s.add(circle(R))\n    s.wait(R)\n")
# Different sizes: a .pyc is trusted when its source has the same size and mtime in seconds.
for r in (1, 20):
    with open(helper_py, "w") as f:
        f.write(f"R = {r}\n")
    assert load(scene_py).bake().duration == r, "an edited neighbour is imported again"
assert watched(scene_py) == [scene_py, helper_py], watched(scene_py)
assert not any("site-packages" in p for p in watched(scene_py))
"##);
}

#[test]
fn sounds_match_rhai() {
    let wav = std::env::temp_dir().join("fastanim-py-tone.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 8000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&wav, spec).unwrap();
    (0..16000).for_each(|_| w.write_sample(0i16).unwrap());
    w.finalize().unwrap();
    let wav = wav.display();

    let py = format!(
        r#"
from fastanim import *

@scene
def s(s):
    a = audio("{wav}")
    assert a.duration == 2
    c = s.add(circle(1))
    s.play(sound(a))
    s.play(create(c), sound(a, 0.5, 1))
    s.add_sound(a, start=1)
    s.wait_for_sounds()
    s.play(sound(a, end=0.5))
"#
    );
    let rhai = format!(
        r#"
        let a = audio("{wav}");
        let c = scene.add(circle(1.0));
        scene.play(sound(a));
        scene.play([create(c), sound(a, 0.5, 1.0)]);
        scene.add_sound(a, 1.0, 2.0);
        scene.wait_for_sounds();
        scene.play(sound(a, 0.0, 0.5));
    "#
    );
    let a = attach(|py_| bake_src(py_, &py, "s")).unwrap_or_else(|e| panic!("{e}"));
    let b = fastanim_script::bake(&rhai).unwrap();
    assert_eq!(a.duration(), 4.5);
    assert_eq!(a.sounds(), b.sounds());
    assert_same(&a, &b);

    let e = err(&format!("s.play(sound(audio(\"{wav}\"), 1.5, 0.5))"));
    assert!(
        e.starts_with("SceneError") && e.contains("must lie within"),
        "{e}"
    );
}
