// A minimal WebM muxer for one video track of encoded chunks (WebCodecs `EncodedVideoChunk`
// data): no cues, so players seek by scanning, which is fine for short scenes.
//   webm({ codec: "V_VP9", width, height, durationMs, frames: [{ data, ms, key }] }) -> Uint8Array

const enc = new TextEncoder();

function uint(n) {
  const b = [];
  do { b.unshift(n & 0xff); n = Math.floor(n / 256); } while (n > 0);
  return new Uint8Array(b);
}

function float(x) {
  const b = new Uint8Array(8);
  new DataView(b.buffer).setFloat64(0, x);
  return b;
}

function concat(parts) {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) { out.set(p, at); at += p.length; }
  return out;
}

// An element: id bytes, its size as an 8-byte vint, then its body.
function el(id, ...body) {
  const data = concat(body.map((b) =>
    typeof b === "number" ? uint(b) : typeof b === "string" ? enc.encode(b) : b));
  const size = new Uint8Array(8);
  size[0] = 1;
  let n = data.length;
  for (let i = 7; i > 0; i--) { size[i] = n & 0xff; n = Math.floor(n / 256); }
  return concat([uint(id), size, data]);
}

export function webm({ codec, width, height, durationMs, frames }) {
  const clusters = [];
  let cluster = null;
  for (const f of frames) {
    // SimpleBlock times are int16 ms from their cluster's, so a cluster starts at every key
    // frame and at least every 30 s.
    if (!cluster || f.key || f.ms - cluster.ms > 30000) {
      cluster = { ms: f.ms, blocks: [] };
      clusters.push(cluster);
    }
    const head = new Uint8Array(4);
    head[0] = 0x81; // track 1
    new DataView(head.buffer).setInt16(1, f.ms - cluster.ms);
    head[3] = f.key ? 0x80 : 0;
    cluster.blocks.push(el(0xa3, head, f.data));
  }
  return concat([
    el(0x1a45dfa3, el(0x4286, 1), el(0x42f7, 1), el(0x42f2, 4), el(0x42f3, 8),
      el(0x4282, "webm"), el(0x4287, 2), el(0x4285, 2)),
    el(0x18538067,
      el(0x1549a966, el(0x2ad7b1, 1e6), el(0x4d80, "fastanim"), el(0x5741, "fastanim"),
        el(0x4489, float(durationMs))),
      el(0x1654ae6b, el(0xae, el(0xd7, 1), el(0x73c5, 1), el(0x83, 1), el(0x86, codec),
        el(0xe0, el(0xb0, width), el(0xba, height)))),
      ...clusters.map((c) => el(0x1f43b675, el(0xe7, c.ms), ...c.blocks))),
  ]);
}
