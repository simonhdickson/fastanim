// A shader fill: WGSL drawn on the GPU by preview and export, animated by `shape.time`.
// SVG and the web player draw the solid fill instead.
let plasma = `
fn shade(uv: vec2<f32>) -> vec4<f32> {
    let p = (uv - 0.5) * shape.size;
    let t = shape.time;
    let v = sin(p.x * 3.0 + t) + sin(p.y * 3.0 - t) + sin(length(p) * 4.0 - t * 2.0);
    let c = 0.5 + 0.5 * cos(v + vec3<f32>(0.0, 2.0, 4.0));
    return vec4<f32>(c * shape.color.rgb, 1.0);
}`;
let card = scene.add(square(3.0).fill(WHITE).shader(plasma).shift(LEFT * 3.0));
let disc = scene.add(circle(1.5).fill(BLUE).shader(plasma).shift(RIGHT * 3.0));
scene.play([fade_in(card), create(disc)]);
scene.play(rotate(card, PI / 2.0).run_time(2.0));
scene.play(scale(disc, 0.5));
scene.wait(1.0);
