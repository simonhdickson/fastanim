// Packs an RGBA frame into planar yuv420p (Y plane, then U, then V), BT.601 limited range:
// the same conversion ffmpeg's swscale applies to the RGBA frames libx264 would otherwise get.
// Each invocation writes one little-endian u32, i.e. four consecutive output bytes.

@group(0) @binding(0) var frame: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> out: array<u32>;

fn rgb(x: u32, y: u32) -> vec3<f32> {
    return textureLoad(frame, vec2(x, y), 0).rgb;
}

fn byte_at(i: u32, w: u32, h: u32) -> u32 {
    let cw = (w + 1u) / 2u;
    let ch = (h + 1u) / 2u;
    if i < w * h {
        return u32(round(16.0 + dot(rgb(i % w, i / w), vec3(65.481, 128.553, 24.966))));
    }
    var j = i - w * h;
    var k = vec3(-37.797, -74.203, 112.0);
    if j >= cw * ch {
        j -= cw * ch;
        k = vec3(112.0, -93.786, -18.214);
    }
    if j >= cw * ch {
        return 0u; // padding up to a whole u32
    }
    // Chroma is the mean of its 2x2 block, clamped at odd edges.
    let x = (j % cw) * 2u;
    let y = (j / cw) * 2u;
    let x1 = min(x + 1u, w - 1u);
    let y1 = min(y + 1u, h - 1u);
    let c = (rgb(x, y) + rgb(x1, y) + rgb(x, y1) + rgb(x1, y1)) * 0.25;
    return u32(round(128.0 + dot(c, k)));
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= arrayLength(&out) {
        return;
    }
    let d = textureDimensions(frame);
    var word = 0u;
    for (var b = 0u; b < 4u; b++) {
        word |= byte_at(id.x * 4u + b, d.x, d.y) << (8u * b);
    }
    out[id.x] = word;
}
