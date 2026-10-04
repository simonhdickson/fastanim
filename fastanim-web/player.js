// <fastanim-player src="scene.rhai"></fastanim-player>: a scene script played in the page,
// with play/pause, a scrubber and marker buttons (SPEC §14.4). A `.bundle` beside the script,
// from `fastanim run --bundle`, is loaded too, so its text is never typeset in the browser.
// Focus it for keys: Space play/pause, ←/→ step a frame, [/] previous/next marker.
//
// Baking and drawing happen in `worker.js`. Scripting the element:
//   await el.open(url)          fetch, bake and play a script (and its bundle); returns the script
//   el.load(script, bundle?)    bake a script given as text
//   el.download("svg" | "png")  save the current frame
// It fires `baked` (detail: { duration, markers, bakeMs, typesetMs }) and `bakeerror`
// (detail: the message, also shown under the player); on error the last good scene stays.

const STYLE = `
  :host { display: block; color: #ddd; font: 14px system-ui, sans-serif; outline: none; }
  canvas { width: 100%; aspect-ratio: 16 / 9; display: block; background: #000; }
  .bar { display: flex; gap: 8px; align-items: center; margin-top: 8px; flex-wrap: wrap; }
  .bar:empty { display: none; }
  input { flex: 1; min-width: 120px; }
  button { background: #222; color: #ddd; border: 1px solid #444; padding: 4px 10px; }
  .time { font-variant-numeric: tabular-nums; }
  .error { color: #f77; white-space: pre-wrap; font-family: ui-monospace, monospace; }
  .error:empty { display: none; }
`;

class FastanimPlayer extends HTMLElement {
  static observedAttributes = ["src"];
  #worker; #ui = {}; #duration = 0; #t = 0; #playing = false; #last = null; #markers = [];

  constructor() {
    super();
    const root = this.attachShadow({ mode: "open" });
    root.innerHTML = `<style>${STYLE}</style>
      <canvas width="1920" height="1080"></canvas>
      <div class="bar">
        <button class="play">Play</button>
        <input class="scrub" type="range" min="0" max="0" step="0.001" value="0" aria-label="Time">
        <span class="time">0.00 / 0.00</span>
      </div>
      <div class="bar markers"></div>
      <div class="error" role="alert"></div>`;
    for (const c of ["play", "scrub", "time", "markers", "error"]) {
      this.#ui[c] = root.querySelector("." + c);
    }
    if (!this.hasAttribute("tabindex")) this.tabIndex = 0;

    // ponytail: one worker per element for its whole life; terminate on disconnect if pages
    // ever add and remove many players.
    this.#worker = new Worker(new URL("worker.js", import.meta.url), { type: "module" });
    const offscreen = root.querySelector("canvas").transferControlToOffscreen();
    this.#worker.postMessage({ canvas: offscreen }, [offscreen]);
    this.#worker.onmessage = ({ data }) => this.#message(data);

    this.#ui.play.onclick = () => this.#setPlaying(!this.#playing);
    this.#ui.scrub.oninput = (e) => this.#seek(+e.target.value);
    this.addEventListener("keydown", (e) => this.#key(e));
    requestAnimationFrame((now) => this.#frame(now));
  }

  attributeChangedCallback(_, __, src) {
    if (src) this.open(src).catch(() => {});
  }

  async open(url) {
    try {
      url = new URL(url, document.baseURI);
      const bundleUrl = url.pathname.endsWith(".rhai")
        ? new URL(url.pathname.replace(/\.rhai$/, ".bundle"), url) : null;
      const [script, bundle] = await Promise.all([
        fetch(url).then((r) => r.ok ? r.text() : Promise.reject(new Error(`${url}: ${r.status}`))),
        bundleUrl && fetch(bundleUrl).then((r) => r.ok ? r.text() : undefined, () => undefined),
      ]);
      this.#t = 0;
      this.load(script, bundle ?? undefined);
      return script;
    } catch (e) {
      this.#error(e.message);
      throw e;
    }
  }

  load(script, bundle) {
    this.#worker.postMessage(bundle === undefined ? { src: script } : { bundle, src: script });
  }

  download(kind) {
    this.#worker.postMessage({ t: this.#t, [kind]: true });
  }

  #message(data) {
    if (data.error !== undefined) {
      this.#error(data.error);
    } else if (data.baked) {
      const b = data.baked;
      this.#ui.error.textContent = "";
      this.#duration = b.duration;
      this.#markers = b.markers;
      this.#ui.scrub.max = b.duration;
      this.#ui.markers.replaceChildren(...b.markers.map((m) => {
        const btn = document.createElement("button");
        btn.textContent = m.name;
        btn.onclick = () => this.#seek(m.t);
        return btn;
      }));
      this.#seek(this.#t);
      this.dispatchEvent(new CustomEvent("baked", { detail: b }));
    } else if (data.svg !== undefined) {
      save(new Blob([data.svg], { type: "image/svg+xml" }), `frame-${this.#t.toFixed(2)}.svg`);
    } else if (data.png) {
      save(data.png, `frame-${this.#t.toFixed(2)}.png`);
    }
  }

  #error(message) {
    this.#ui.error.textContent = message;
    this.dispatchEvent(new CustomEvent("bakeerror", { detail: message }));
  }

  #seek(t) {
    this.#t = Math.min(Math.max(t, 0), this.#duration);
    this.#worker.postMessage({ t: this.#t });
    this.#ui.scrub.value = this.#t;
    this.#ui.time.textContent = `${this.#t.toFixed(2)} / ${this.#duration.toFixed(2)}`;
  }

  #setPlaying(p) {
    this.#playing = p;
    if (p && this.#t >= this.#duration) this.#seek(0);
    this.#ui.play.textContent = p ? "Pause" : "Play";
  }

  #frame(now) {
    if (this.#playing) {
      this.#seek(this.#t + (now - (this.#last ?? now)) / 1000);
      if (this.#t >= this.#duration) this.#setPlaying(false);
    }
    this.#last = now;
    requestAnimationFrame((now) => this.#frame(now));
  }

  #key(e) {
    // The scrubber steps itself, and Space already clicks a focused button.
    const inner = e.composedPath()[0];
    if (inner.tagName === "INPUT" || (inner.tagName === "BUTTON" && e.key === " ")) return;
    // Markers a hair either side of t, so repeated presses walk past the current one.
    const ts = this.#markers.map((m) => m.t);
    const prev = ts.filter((t) => t < this.#t - 1e-3).at(-1) ?? 0;
    const next = ts.find((t) => t > this.#t + 1e-3) ?? this.#duration;
    const go = { ArrowRight: this.#t + 1 / 60, ArrowLeft: this.#t - 1 / 60, "[": prev, "]": next }[e.key];
    if (e.key === " ") this.#setPlaying(!this.#playing);
    else if (go !== undefined) this.#seek(go);
    else return;
    e.preventDefault();
  }
}

function save(blob, name) {
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = name;
  a.click();
  URL.revokeObjectURL(a.href);
}

customElements.define("fastanim-player", FastanimPlayer);
