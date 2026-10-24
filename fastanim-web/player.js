// <fastanim-player src="scene.py"></fastanim-player>: a Python scene played in the page, run
// with Pyodide (pyplayer.js), with play/pause, a scrubber, marker buttons and a timeline of the
// scene's plays, updaters and sounds (SPEC §14.4), five lanes tall and scrolling past that; drag
// along it to scrub, Shift+drag to select a region that playback loops over.
// Focus it for keys: Space play/pause, ←/→ step a frame, [/] previous/next marker, Esc clear the
// loop region.
//
// Baking and drawing happen in `worker.js`. Scripting the element:
//   await el.open(url)          fetch, bake and play a scene; returns the script
//   el.load(script)             bake a scene given as text
//   el.download("svg" | "png")  save the current frame
// It fires `baked` (detail: { duration, markers, events, bakeMs }) and `bakeerror`
// (detail: the message, also shown under the player); on error the last good scene stays.

const LANE = 24;
// Lanes the timeline shows at once; it scrolls through the rest.
const MAX_LANES = 5;

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
  .tracks { margin-top: 8px; background: #111; max-height: ${MAX_LANES * LANE}px; overflow-y: auto; }
  .lanes { position: relative; cursor: pointer; user-select: none; }
  .tracks .ev { position: absolute; height: 20px; min-width: 3px; padding: 0 4px; box-sizing: border-box;
    overflow: hidden; white-space: nowrap; text-overflow: ellipsis; font-size: 12px; line-height: 18px;
    background: #1e3d4a; border: 1px solid #3a7990; border-radius: 3px; }
  .tracks .updater { background: #24402a; border-color: #4c8a58; }
  .tracks .sound { background: #3b2a4a; border-color: #7a5a96; }
  .tracks .head, .tracks .tick, .tracks .loop { position: absolute; top: 0; bottom: 0; pointer-events: none; }
  .tracks .loop { background: #ff02; border: solid #ff0; border-width: 0 1px; box-sizing: border-box; }
  .tracks .head { width: 2px; margin-left: -1px; background: #58c4dd; }
  .tracks .tick { width: 1px; background: #fff6; }
`;

class FastanimPlayer extends HTMLElement {
  static observedAttributes = ["src"];
  #worker; #ui = {}; #duration = 0; #t = 0; #playing = false; #last = null; #markers = [];
  #region = null; #drag = null;

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
      <div class="tracks" hidden><div class="lanes"></div></div>
      <div class="bar markers"></div>
      <div class="error" role="alert"></div>`;
    for (const c of ["play", "scrub", "time", "tracks", "lanes", "markers", "error"]) {
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
    // On the lanes rather than the scrolling box, so its scrollbar neither scrubs nor counts.
    const tracks = this.#ui.lanes;
    const at = (e) => {
      const r = tracks.getBoundingClientRect();
      return Math.min(Math.max((e.clientX - r.left) / r.width, 0), 1) * this.#duration;
    };
    // A drag scrubs; a Shift+drag selects the loop region from where it started.
    tracks.onpointerdown = (e) => {
      if (e.button !== 0) return;
      tracks.setPointerCapture(e.pointerId);
      this.#drag = e.shiftKey ? { anchor: at(e) } : {};
      this.#dragTo(at(e));
    };
    tracks.onpointermove = (e) => { if (this.#drag) this.#dragTo(at(e)); };
    tracks.onpointerup = tracks.onpointercancel = () => { this.#drag = null; };
    this.addEventListener("keydown", (e) => this.#key(e));
    requestAnimationFrame((now) => this.#frame(now));
  }

  attributeChangedCallback(_, __, src) {
    if (src) this.open(src).catch(() => {});
  }

  async open(url) {
    try {
      url = new URL(url, document.baseURI);
      const r = await fetch(url);
      if (!r.ok) throw new Error(`${url}: ${r.status}`);
      const script = await r.text();
      this.#t = 0;
      this.#region = null;
      this.load(script);
      return script;
    } catch (e) {
      this.#error(e.message);
      throw e;
    }
  }

  load(script) {
    this.#worker.postMessage({ src: script });
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
      this.#tracks(b.events);
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

  #dragTo(t) {
    const a = this.#drag.anchor;
    if (a === undefined) return this.#seek(t);
    // A Shift+click without a drag selects nothing, so it leaves the region alone.
    if (Math.abs(t - a) <= 1 / 60) return;
    this.#setRegion([Math.min(a, t), Math.max(a, t)]);
    this.#seek(Math.min(a, t));
  }

  #setRegion(r) {
    this.#region = r;
    const loop = this.#ui.loop;
    loop.hidden = !r;
    if (r) Object.assign(loop.style, { left: this.#pct(r[0]), width: this.#pct(r[1] - r[0]) });
  }

  #pct(t) {
    return `${this.#duration > 0 ? t / this.#duration * 100 : 0}%`;
  }

  // One block per event, `lane` rows deep, plus the loop region, the playhead and a tick per marker.
  #tracks(events) {
    const pct = (t) => this.#pct(t);
    const lanes = Math.max(0, ...events.map((e) => e.lane + 1));
    this.#ui.tracks.hidden = events.length === 0;
    const tracks = this.#ui.lanes;
    tracks.style.height = `${lanes * LANE}px`;
    const blocks = events.map((e) => {
      const div = document.createElement("div");
      div.className = `ev ${e.kind}`;
      div.textContent = e.label;
      div.title = `${e.label}  ${e.start.toFixed(2)}–${e.end.toFixed(2)} s`;
      Object.assign(div.style, {
        left: pct(e.start), width: pct(e.end - e.start), top: `${e.lane * LANE + 2}px`,
      });
      return div;
    });
    const tick = (m) => {
      const d = document.createElement("div");
      d.className = "tick";
      d.style.left = pct(m.t);
      return d;
    };
    this.#ui.head = document.createElement("div");
    this.#ui.head.className = "head";
    this.#ui.loop = document.createElement("div");
    this.#ui.loop.className = "loop";
    // The region survives re-bakes, cut to the new duration.
    const r = this.#region?.map((t) => Math.min(t, this.#duration));
    this.#setRegion(r && r[1] > r[0] ? r : null);
    tracks.replaceChildren(...blocks, this.#ui.loop, ...this.#markers.map(tick), this.#ui.head);
  }

  #seek(t) {
    this.#t = Math.min(Math.max(t, 0), this.#duration);
    this.#worker.postMessage({ t: this.#t });
    this.#ui.scrub.value = this.#t;
    if (this.#ui.head) this.#ui.head.style.left = this.#pct(this.#t);
    this.#ui.time.textContent = `${this.#t.toFixed(2)} / ${this.#duration.toFixed(2)}`;
  }

  #setPlaying(p) {
    this.#playing = p;
    if (p && !this.#region && this.#t >= this.#duration) this.#seek(0);
    this.#ui.play.textContent = p ? "Pause" : "Play";
  }

  #frame(now) {
    if (this.#playing) {
      let t = this.#t + (now - (this.#last ?? now)) / 1000;
      const r = this.#region;
      if (r && (t < r[0] || t >= r[1])) t = r[0];
      this.#seek(t);
      if (!r && this.#t >= this.#duration) this.#setPlaying(false);
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
    else if (e.key === "Escape" && this.#region) this.#setRegion(null);
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
