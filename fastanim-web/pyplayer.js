// Python scenes in the worker: the `fastanim` package built for Pyodide (`just pyodide-wheel`)
// runs the script as `python -m fastanim` would, and `PyPlayer` draws the baked timeline on a
// Canvas 2D. Pyodide is only fetched for the first scene.

const PYODIDE = "https://cdn.jsdelivr.net/pyodide/v314.0.7/full/";
// shortcut: the wheel's name is fixed to the workspace version, so bump it with Cargo.toml; read it from a manifest if that gets missed.
const WHEEL = new URL("wheels/fastanim-0.1.1-cp310-abi3-pyemscripten_2026_0_wasm32.whl", import.meta.url);

let pyodide = null;

async function boot() {
  const { loadPyodide } = await import(PYODIDE + "pyodide.mjs");
  const py = await loadPyodide({ indexURL: PYODIDE });
  await py.loadPackage(WHEEL.href);
  py.FS.mkdirTree("/scene");
  // The scene's frames only, as `python -m fastanim` would print them.
  py.runPython(`
import traceback
from fastanim import SceneFn, scene
from fastanim._load import RUN_NAME, load

def _bake(path):
    """The baked scene and None, or None and the error."""
    try:
        f = load(path)
        return (f if isinstance(f, SceneFn) else scene(f)).bake(), None
    except Exception as e:
        return None, _error(e)

def _error(e):
    tb = e.__traceback__
    while tb is not None and tb.tb_frame.f_globals.get("__name__") != RUN_NAME:
        tb = tb.tb_next
    if isinstance(e, SyntaxError) or tb is None:
        return "".join(traceback.format_exception_only(e)).strip()
    return "".join(traceback.format_exception(type(e), e, tb)).strip()
`);
  return py;
}

/// Runs and bakes the Python scene `src`; throws an Error with the Python traceback.
export async function bakePython(src) {
  pyodide ??= boot().catch((e) => { pyodide = null; throw e; });
  const py = await pyodide;
  // Imports from Pyodide's own packages, such as numpy, are fetched first.
  await py.loadPackagesFromImports(src);
  py.FS.writeFile("/scene/scene.py", src);
  const t0 = performance.now();
  const [tl, error] = py.globals.get("_bake")("/scene/scene.py");
  if (error) throw new Error(error);
  return new PyPlayer(tl, performance.now() - t0);
}

class PyPlayer {
  #tl;
  #bakeMs;
  /** Each image's pixels on a canvas of their own, by key. */
  #images = new Map();

  constructor(tl, bakeMs) {
    this.#tl = tl;
    this.#bakeMs = bakeMs;
  }

  free() { this.#tl.destroy(); }
  bake_ms() { return this.#bakeMs; }
  duration() { return this.#tl.duration; }
  marker_names() { return this.#markers().map(([n]) => n); }
  marker_times() { return this.#markers().map(([, t]) => t); }
  events() { return this.#js(this.#tl.events); }
  svg(t) { return this.#tl.svg(t); }

  #markers() { return this.#js(this.#tl.markers); }

  #js(proxy) {
    const v = proxy.toJs({ dict_converter: Object.fromEntries });
    proxy.destroy();
    return v;
  }

  /// Draws the frame at `t` from `Timeline.frame`, filling the canvas width with the 16:9 frame.
  draw(ctx, t) {
    const { width: w, height: h } = ctx.canvas;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.globalAlpha = 1;
    ctx.fillStyle = "#000";
    ctx.fillRect(0, 0, w, h);
    // Scene units are y-up and centered.
    // FRAME_HEIGHT and FRAME_WIDTH in fastanim-core.
    const FRAME_HEIGHT = 8, FRAME_WIDTH = FRAME_HEIGHT * 16 / 9;
    const k = w / FRAME_WIDTH;
    ctx.setTransform(k, 0, 0, -k, w / 2, h / 2);
    ctx.fillStyle = this.#tl.background_css;
    ctx.fillRect(-FRAME_WIDTH / 2, -FRAME_HEIGHT / 2, FRAME_WIDTH, FRAME_HEIGHT);
    // Vello's and SVG's.
    ctx.miterLimit = 4;

    for (const m of this.#js(this.#tl.frame(t))) {
      if (m.image) this.#drawImage(ctx, m.image);
      if (!m.d) continue;
      const p = new Path2D(m.d);
      ctx.globalAlpha = m.opacity;
      if (m.fill) {
        ctx.fillStyle = m.fill_gradient ? gradient(ctx, m.fill_gradient) : m.fill;
        ctx.fill(p);
      }
      if (m.stroke) {
        ctx.strokeStyle = m.stroke_gradient ? gradient(ctx, m.stroke_gradient) : m.stroke;
        ctx.lineWidth = m.width;
        ctx.lineJoin = m.join;
        ctx.lineCap = m.cap;
        ctx.stroke(p);
      }
    }
  }

  #drawImage(ctx, [key, [a, b, c, d, e, f], smooth, alpha]) {
    if (alpha <= 0) return;
    let src = this.#images.get(key);
    if (!src) {
      const [iw, ih, bytes] = this.#js(this.#tl.image(key));
      src = new OffscreenCanvas(iw, ih);
      src.getContext("2d").putImageData(new ImageData(new Uint8ClampedArray(bytes.buffer, bytes.byteOffset, bytes.length), iw, ih), 0, 0);
      this.#images.set(key, src);
    }
    ctx.save();
    ctx.globalAlpha = alpha;
    ctx.imageSmoothingEnabled = smooth;
    ctx.transform(a, b, c, d, e, f);
    ctx.drawImage(src, 0, 0, 1, 1);
    ctx.restore();
  }
}

function gradient(ctx, [x0, y0, x1, y1, stops]) {
  const g = ctx.createLinearGradient(x0, y0, x1, y1);
  for (const [offset, css] of stops) g.addColorStop(offset, css);
  return g;
}
