// Bakes and draws off the page's thread (SPEC §14.5). The page sends any of
//   { canvas }  the OffscreenCanvas to draw on, once
//   { src }     a script to bake; replies { baked: { duration, markers, bakeMs, typesetMs } }
//               or { error }, and keeps the last good scene on error
//   { t }       the time to draw
//   { svg }     reply { svg } with the frame at the current time
//   { png }     reply { png: Blob } with the canvas
// Messages that arrive while a bake runs are merged, so only the latest source and time are
// worked on.
import init, { Player } from "./fastanim_web.js";

const ready = init();
let player = null, ctx = null, t = 0, next = {}, scheduled = false;

self.onmessage = ({ data }) => {
  Object.assign(next, data);
  if (!scheduled) { scheduled = true; setTimeout(work); }
};

async function work() {
  await ready;
  const job = next;
  next = {};
  scheduled = false;
  if (job.canvas) ctx = job.canvas.getContext("2d");
  if (job.t !== undefined) t = job.t;
  if (job.src !== undefined) {
    try {
      const p = new Player(job.src);
      player?.free();
      player = p;
      const names = p.marker_names(), times = p.marker_times();
      postMessage({ baked: {
        duration: p.duration(),
        markers: names.map((name, i) => ({ name, t: times[i] })),
        bakeMs: p.bake_ms(),
        typesetMs: p.typeset_ms(),
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
