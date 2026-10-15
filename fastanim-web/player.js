// <fastanim-player src="scene.rhai"></fastanim-player>: a scene script played in the page,
// with play/pause, a scrubber, marker buttons and a timeline of the scene's plays, updaters and
// sounds (SPEC §14.4); clicking one seeks to it and boxes the shapes it affects. A `.bundle` beside the script,
// from `fastanim bundle`, is loaded too, so its text is never typeset in the browser.
// Focus it for keys: Space play/pause, ←/→ step a frame, [/] previous/next marker, Esc deselect.
//
// Baking and drawing happen in `worker.js`. Scripting the element:
//   await el.open(url)          fetch, bake and play a script (and its bundle); returns the script
//   el.load(script, bundle?)    bake a script given as text
//   el.download("svg" | "png")  save the current frame
//   el.download("video")        save the whole scene as WebM (fires `recorded` when saved)
// It fires `baked` (detail: { duration, markers, events, bakeMs, typesetMs, planMs, snapshotMs }) and `bakeerror`
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
  .tracks { position: relative; margin-top: 8px; background: #111; cursor: pointer; }
  .tracks .ev { position: absolute; height: 20px; min-width: 3px; padding: 0 4px; box-sizing: border-box;
    overflow: hidden; white-space: nowrap; text-overflow: ellipsis; font-size: 12px; text-align: left;
    background: #1e3d4a; border: 1px solid #3a7990; border-radius: 3px; }
  .tracks .updater { background: #24402a; border-color: #4c8a58; }
  .tracks .sound { background: #3b2a4a; border-color: #7a5a96; }
  .tracks .ev.sel { background: #5c5400; border-color: #ff0; color: #fff; }
  .tracks .head, .tracks .tick { position: absolute; top: 0; bottom: 0; pointer-events: none; }
  .tracks .head { width: 2px; margin-left: -1px; background: #58c4dd; }
  .tracks .tick { width: 1px; background: #fff6; }
`;
const LANE = 24;

class FastanimPlayer extends HTMLElement {
  static observedAttributes = ["src"];
  #worker; #ui = {}; #duration = 0; #t = 0; #playing = false; #last = null; #markers = [];
  #blocks = []; #selected = null;

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
      <div class="tracks" hidden></div>
      <div class="bar markers"></div>
      <div class="error" role="alert"></div>`;
    for (const c of ["play", "scrub", "time", "tracks", "markers", "error"]) {
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
    // Clicking the empty track scrubs; blocks stop the click.
    this.#ui.tracks.onclick = (e) => {
      const r = this.#ui.tracks.getBoundingClientRect();
      this.#seek((e.clientX - r.left) / r.width * this.#duration);
    };
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
      this.#tracks(b.events);
      this.#seek(this.#t);
      this.dispatchEvent(new CustomEvent("baked", { detail: b }));
    } else if (data.svg !== undefined) {
      save(new Blob([data.svg], { type: "image/svg+xml" }), `frame-${this.#t.toFixed(2)}.svg`);
    } else if (data.png) {
      save(data.png, `frame-${this.#t.toFixed(2)}.png`);
    } else if (data.video) {
      save(data.video, "scene.webm");
      this.dispatchEvent(new CustomEvent("recorded"));
    }
  }

  #error(message) {
    this.#ui.error.textContent = message;
    this.dispatchEvent(new CustomEvent("bakeerror", { detail: message }));
  }

  // One block per event, `lane` rows deep, plus the playhead and a tick per marker.
  #tracks(events) {
    const pct = (t) => `${this.#duration > 0 ? t / this.#duration * 100 : 0}%`;
    const lanes = Math.max(0, ...events.map((e) => e.lane + 1));
    const tracks = this.#ui.tracks;
    tracks.hidden = events.length === 0;
    tracks.style.height = `${lanes * LANE}px`;
    this.#selected = null;
    this.#blocks = events.map((e, i) => {
      const btn = document.createElement("button");
      btn.className = `ev ${e.kind}`;
      btn.textContent = e.label;
      btn.title = `${e.label}  ${e.start.toFixed(2)}–${e.end.toFixed(2)} s`;
      Object.assign(btn.style, {
        left: pct(e.start), width: pct(e.end - e.start), top: `${e.lane * LANE + 2}px`,
      });
      btn.onclick = (ev) => {
        ev.stopPropagation();
        if (this.#selected === i) return this.#select(null);
        this.#select(i);
        this.#seek(e.start);
      };
      return btn;
    });
    const tick = (m) => {
      const d = document.createElement("div");
      d.className = "tick";
      d.style.left = pct(m.t);
      return d;
    };
    this.#ui.head = document.createElement("div");
    this.#ui.head.className = "head";
    tracks.replaceChildren(...this.#blocks, ...this.#markers.map(tick), this.#ui.head);
  }

  #select(i) {
    this.#blocks[this.#selected]?.classList.remove("sel");
    this.#selected = i;
    this.#blocks[i]?.classList.add("sel");
    this.#worker.postMessage({ select: i });
  }

  #seek(t) {
    this.#t = Math.min(Math.max(t, 0), this.#duration);
    this.#worker.postMessage({ t: this.#t });
    this.#ui.scrub.value = this.#t;
    if (this.#ui.head) this.#ui.head.style.left = `${this.#duration > 0 ? this.#t / this.#duration * 100 : 0}%`;
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
    else if (e.key === "Escape" && this.#selected !== null) this.#select(null);
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
