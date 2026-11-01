// Python scenes in the worker: the `fastanim` package built for Pyodide (`just pyodide-wheel`)
// runs the script as `python -m fastanim` would, and `PyPlayer` draws the baked timeline on a
// Canvas 2D, with shader fills drawn on WebGPU where the browser has it. Pyodide is only fetched
// for the first scene.

const PYODIDE = "https://cdn.jsdelivr.net/pyodide/v314.0.7/full/";
// shortcut: the wheel's name is fixed to the workspace version, so bump it with Cargo.toml; read it from a manifest if that gets missed.
const WHEEL = new URL("wheels/fastanim-0.1.3-cp310-abi3-pyemscripten_2026_0_wasm32.whl", import.meta.url);

let pyodide = null;

async function boot() {
  const { loadPyodide } = await import(PYODIDE + "pyodide.mjs");
  const py = await loadPyodide({ indexURL: PYODIDE });
  await py.loadPackage(WHEEL.href);
  py.FS.mkdirTree("/scene");
  // So relative paths find files beside the scene, as when running it from its own folder.
  py.FS.chdir("/scene");
  // shortcut: files the example scenes read from beside themselves are listed by hand (gists bring their own); fetch them per scene if the list grows.
  for (const name of ["bounce.json"]) {
    const r = await fetch(new URL(`pyscenes/${name}`, import.meta.url));
    py.FS.writeFile(`/scene/${name}`, new Uint8Array(await r.arrayBuffer()));
  }
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

/// Runs and bakes the Python scene `src`, with `files` ({ name: string or Uint8Array }) written
/// beside it; throws an Error with the Python traceback.
export async function bakePython(src, files = {}) {
  pyodide ??= boot().catch((e) => { pyodide = null; throw e; });
  const py = await pyodide;
  // Imports from Pyodide's own packages, such as numpy, are fetched first.
  await py.loadPackagesFromImports(src);
  for (const [name, data] of Object.entries(files)) py.FS.writeFile(`/scene/${name}`, data);
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
  async draw(ctx, t) {
    const { width: w, height: h } = ctx.canvas;
    // FRAME_HEIGHT and FRAME_WIDTH in fastanim-core.
    const FRAME_HEIGHT = 8, FRAME_WIDTH = FRAME_HEIGHT * 16 / 9;
    const k = w / FRAME_WIDTH;
    const frame = this.#js(this.#tl.frame(t));
    // Shaders draw first: the canvas shows whatever is on it whenever this awaits, so the frame
    // itself is drawn without a pause.
    for (const m of frame) if (m.shader && m.fill) m.shaded = await shade(m.shader, t, k);
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.globalAlpha = 1;
    ctx.fillStyle = "#000";
    ctx.fillRect(0, 0, w, h);
    // Scene units are y-up and centered.
    ctx.setTransform(k, 0, 0, -k, w / 2, h / 2);
    ctx.fillStyle = this.#tl.background_css;
    ctx.fillRect(-FRAME_WIDTH / 2, -FRAME_HEIGHT / 2, FRAME_WIDTH, FRAME_HEIGHT);
    // Vello's and SVG's.
    ctx.miterLimit = 4;

    for (const m of frame) {
      if (m.image) this.#drawImage(ctx, m.image);
      if (!m.d) continue;
      const p = new Path2D(m.d);
      ctx.globalAlpha = m.opacity;
      if (m.shaded) {
        const [a, b, c, d, e, f] = m.shader[1];
        ctx.save();
        ctx.clip(p);
        ctx.globalAlpha = m.shader[2][3] * m.opacity;
        ctx.transform(a, b, c, d, e, f);
        ctx.drawImage(m.shaded, 0, 0, 1, 1);
        ctx.restore();
        m.shaded.close();
      } else if (m.fill) {
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

/// Shader fill `[source, [a, b, c, d, e, f], color]` drawn at `k` pixels per unit at time `t`,
/// as fastanim-bevy's `encode` sizes it, as an ImageBitmap; null where it can't, for the solid
/// fill instead.
async function shade([source, [a, b, c, d], color], t, k) {
  gpu ??= Gpu.create().catch((e) => { console.warn("no WebGPU; shader fills draw their solid fill:", e.message); return null; });
  const g = await gpu;
  if (!g) return null;
  const sw = Math.hypot(a, b), sh = Math.hypot(c, d);
  const px = (v) => Math.min(Math.max(Math.ceil(v * k), 1), MAX_SIZE);
  const w = px(sw), h = px(sh);
  // `Shape`'s fields, padded to its 48-byte uniform layout.
  return g.draw(source, w, h, new Float32Array([...color, sw, sh, w, h, t, 0, 0, 0]));
}

/// The WebGPU device shader fills draw with, or null without one; made on the first shader fill.
let gpu = null;

/// Textures are at most this many pixels a side.
const MAX_SIZE = 2048;

/// fastanim-bevy/src/shader.rs's prelude.
const PRELUDE = `
struct Shape { color: vec4<f32>, size: vec2<f32>, res: vec2<f32>, time: f32 }
@group(0) @binding(0) var<uniform> shape: Shape;

@vertex
fn fastanim_vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fastanim_fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return shade(p.xy / shape.res);
}
`;

/// Shader fills on WebGPU: each draws into a texture that is read back as straight-alpha RGBA,
/// as fastanim-bevy's are, rather than through a WebGPU canvas, which came back blank in headless
/// Chromium. Support and speed are the browser's: off by default on Linux, usable in Chromium,
/// very slow in Firefox; the desktop renderer is the dependable one.
class Gpu {
  #device;
  #uniforms;
  /// Pipeline promises by WGSL source; null for a source that didn't compile.
  #pipelines = new Map();

  static async create() {
    if (!navigator.gpu) throw new Error("navigator.gpu is missing");
    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) throw new Error("no GPU adapter");
    return new Gpu(await adapter.requestDevice());
  }

  constructor(device) {
    this.#device = device;
    this.#uniforms = device.createBuffer({ size: 48, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
  }

  /// Draws `source` `w`×`h` pixels with `uniforms` as an ImageBitmap, or null if it didn't
  /// compile.
  async draw(source, w, h, uniforms) {
    if (!this.#pipelines.has(source)) this.#pipelines.set(source, this.#compile(source));
    const pipeline = await this.#pipelines.get(source);
    if (!pipeline) return null;
    const device = this.#device;
    device.queue.writeBuffer(this.#uniforms, 0, uniforms);
    const bind = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [{ binding: 0, resource: { buffer: this.#uniforms } }],
    });
    const texture = device.createTexture({
      size: [w, h],
      format: "rgba8unorm",
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
    });
    // Rows are copied out 256-byte aligned.
    const row = Math.ceil(w * 4 / 256) * 256;
    const out = device.createBuffer({ size: row * h, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
    const enc = device.createCommandEncoder();
    const pass = enc.beginRenderPass({
      colorAttachments: [{ view: texture.createView(), loadOp: "clear", clearValue: [0, 0, 0, 0], storeOp: "store" }],
    });
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, bind);
    pass.draw(3);
    pass.end();
    enc.copyTextureToBuffer({ texture }, { buffer: out, bytesPerRow: row }, [w, h]);
    device.queue.submit([enc.finish()]);
    await out.mapAsync(GPUMapMode.READ);
    const padded = new Uint8Array(out.getMappedRange());
    const px = new Uint8ClampedArray(w * 4 * h);
    for (let y = 0; y < h; y++) px.set(padded.subarray(y * row, y * row + w * 4), y * w * 4);
    out.destroy();
    texture.destroy();
    return createImageBitmap(new ImageData(px, w, h));
  }

  /// The pipeline for `source`; on a compile error, logs it once and resolves to null.
  async #compile(source) {
    const module = this.#device.createShaderModule({ code: PRELUDE + source });
    // Line numbers in the user's source, past the prelude.
    const skip = PRELUDE.split("\n").length - 1;
    const errors = (await module.getCompilationInfo()).messages.filter((m) => m.type === "error");
    const fail = (msg) => { console.error(`shader fill failed to compile; drawing its solid fill instead:\n${msg}`); return null; };
    if (errors.length) return fail(errors.map((m) => `${m.lineNum - skip}:${m.linePos}: ${m.message}`).join("\n"));
    try {
      return await this.#device.createRenderPipelineAsync({
        layout: "auto",
        vertex: { module, entryPoint: "fastanim_vs" },
        fragment: { module, entryPoint: "fastanim_fs", targets: [{ format: "rgba8unorm" }] },
      });
    } catch (e) {
      return fail(e.message);
    }
  }
}

function gradient(ctx, [x0, y0, x1, y1, stops]) {
  const g = ctx.createLinearGradient(x0, y0, x1, y1);
  for (const [offset, css] of stops) g.addColorStop(offset, css);
  return g;
}
