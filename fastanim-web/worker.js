// Bakes and draws off the page's thread (SPEC §14.5). The page sends any of
//   { canvas }  the OffscreenCanvas to draw on, once
//   { bundle }  pre-typeset text to fill the cache from, before baking; errors reply { error }
//   { src, lang } a script to bake, Rhai or with lang "python" Python (pyplayer.js); replies { baked: { duration, markers, events, bakeMs, typesetMs, planMs,
//               snapshotMs } } or { error }, and keeps the last good scene on error
//   { t }       the time to draw
//   { svg }     reply { svg } with the frame at the current time
//   { png }     reply { png: Blob } with the canvas
// Messages that arrive while a bake runs are merged, so only the latest source
// and time are worked on. Typeset text persists in IndexedDB across visits (SPEC §14.5).
import init, { Player, load_bundle, save_bundle } from "./fastanim-web.js";
import { bakePython } from "./pyplayer.js";

const ready = init().then(restore);
let player = null, ctx = null, t = 0, next = {}, scheduled = false, queue = Promise.resolve();

self.onmessage = ({ data }) => {
  Object.assign(next, data);
  // Queued, so a Python bake finishes before the next job swaps the player out under it.
  if (!scheduled) { scheduled = true; queue = queue.then(work); }
};

async function work() {
  await ready;
  const job = next;
  next = {};
  scheduled = false;
  if (job.canvas) ctx = job.canvas.getContext("2d");
  if (job.t !== undefined) t = job.t;
  if (job.bundle !== undefined) {
    try { load_bundle(job.bundle); } catch (e) { postMessage({ error: e.message ?? String(e) }); }
  }
  if (job.src !== undefined) {
    try {
      const p = job.lang === "python" ? await bakePython(job.src) : new Player(job.src);
      player?.free();
      player = p;
      if (p.typeset_ms() > 0) persist();
      const names = p.marker_names(), times = p.marker_times();
      postMessage({ baked: {
        duration: p.duration(),
        markers: names.map((name, i) => ({ name, t: times[i] })),
        events: p.events(),
        bakeMs: p.bake_ms(),
        typesetMs: p.typeset_ms(),
        planMs: p.plan_ms(),
        snapshotMs: p.snapshot_ms(),
      } });
    } catch (e) {
      postMessage({ error: e.message ?? String(e) });
    }
  }
  if (!player || !ctx) return;
  t = Math.min(t, player.duration());
  player.draw(ctx, t);
  if (job.svg) postMessage({ svg: player.svg(t) });
  if (job.png) postMessage({ png: await ctx.canvas.convertToBlob() });
}

// The typesetting cache, as one bundle under one key. A missing, blocked or stale (other
// format version) store just means typesetting again.
// ponytail: grows with every snippet ever typeset and never evicts; prune to the snippets the
// last script used if it gets large.
function db() {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open("fastanim", 1);
    req.onupgradeneeded = () => req.result.createObjectStore("cache");
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

function store(mode, f) {
  return db().then((d) => new Promise((resolve, reject) => {
    const tx = d.transaction("cache", mode);
    const req = f(tx.objectStore("cache"));
    tx.oncomplete = () => { d.close(); resolve(req.result); };
    tx.onerror = tx.onabort = () => { d.close(); reject(tx.error); };
  }));
}

async function restore() {
  try {
    const bundle = await store("readonly", (s) => s.get("bundle"));
    if (bundle) load_bundle(bundle);
  } catch {
    store("readwrite", (s) => s.delete("bundle")).catch(() => {});
  }
}

function persist() {
  store("readwrite", (s) => s.put(save_bundle(), "bundle")).catch(() => {});
}
