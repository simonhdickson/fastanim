// Bakes and draws off the page's thread (SPEC §14.5). The page sends any of
//   { canvas }  the OffscreenCanvas to draw on, once
//   { bundle }  pre-typeset text to fill the cache from, before baking; errors reply { error }
//   { src }     a script to bake; replies { baked: { duration, markers, bakeMs, typesetMs } }
//               or { error }, and keeps the last good scene on error
//   { t }       the time to draw
//   { svg }     reply { svg } with the frame at the current time
//   { png }     reply { png: Blob } with the canvas
//   { video }   reply { video: Blob }, the whole scene as WebM, or { error }
// Messages that arrive while a bake or recording runs are merged, so only the latest source
// and time are worked on.
import init, { Player, load_bundle } from "./fastanim-web.js";
import { webm } from "./webm.js";

const ready = init();
let player = null, ctx = null, t = 0, next = {}, scheduled = false, queue = Promise.resolve();

self.onmessage = ({ data }) => {
  Object.assign(next, data);
  // Queued, so a recording finishes before the next job swaps the player out under it.
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
  if (job.video) {
    try { postMessage({ video: await record(player, ctx) }); }
    catch (e) { postMessage({ error: e.message ?? String(e) }); }
    player.draw(ctx, t);
  }
}

const FPS = 60;

// Draws every frame onto the canvas and encodes it, so nothing is dropped however slow it is.
async function record(p, ctx) {
  const { width, height } = ctx.canvas;
  const codecs = [["vp09.00.40.08", "V_VP9"], ["vp8", "V_VP8"]];
  let config, codec;
  for (const [c, id] of codecs) {
    const cfg = { codec: c, width, height, bitrate: 8e6, framerate: FPS };
    if ((await VideoEncoder.isConfigSupported(cfg)).supported) { config = cfg; codec = id; break; }
  }
  if (!config) throw new Error("this browser can't encode VP9 or VP8 video");

  const frames = [];
  let failed = null;
  const encoder = new VideoEncoder({
    output: (chunk) => {
      const data = new Uint8Array(chunk.byteLength);
      chunk.copyTo(data);
      frames.push({ data, ms: Math.round(chunk.timestamp / 1000), key: chunk.type === "key" });
    },
    error: (e) => { failed = e; },
  });
  encoder.configure(config);
  const n = Math.max(1, Math.ceil(p.duration() * FPS));
  for (let i = 0; i <= n && !failed; i++) {
    p.draw(ctx, Math.min(i / FPS, p.duration()));
    const frame = new VideoFrame(ctx.canvas, { timestamp: Math.round(i * 1e6 / FPS) });
    encoder.encode(frame, { keyFrame: i % (2 * FPS) === 0 });
    frame.close();
    // Keep the encoder's queue, and so memory, bounded.
    while (encoder.encodeQueueSize > 8) {
      await new Promise((r) => encoder.addEventListener("dequeue", r, { once: true }));
    }
  }
  if (!failed) await encoder.flush();
  encoder.close();
  if (failed) throw failed;
  const durationMs = (n + 1) * 1000 / FPS;
  return new Blob([webm({ codec, width, height, durationMs, frames })], { type: "video/webm" });
}
