# Shader fills

`.shader(wgsl)` paints a shape's fill with a WGSL fragment shader, run on the GPU every frame.
The desktop preview and `fastanim render` draw it. SVG output and the browser player can't run
shaders, so they draw the shape's solid fill instead.

```rhai
let plasma = `
fn shade(uv: vec2<f32>) -> vec4<f32> {
    let p = (uv - 0.5) * shape.size;
    let t = shape.time;
    let v = sin(p.x * 3.0 + t) + sin(p.y * 3.0 - t) + sin(length(p) * 4.0 - t * 2.0);
    let c = 0.5 + 0.5 * cos(v + vec3<f32>(0.0, 2.0, 4.0));
    return vec4<f32>(c * shape.color.rgb, 1.0);
}`;
let disc = scene.add(circle(1.5).fill(BLUE).shader(plasma));
scene.play(create(disc));
```

The full scene is `fastanim-script/scenes/shader.rhai`.

## Writing `shade`

The source defines one function, `fn shade(uv: vec2<f32>) -> vec4<f32>`, and can add helper
functions and constants of its own. `shade` returns the color at `uv` as straight (not
premultiplied) alpha sRGB, each part 0–1.

`uv` runs over the shape's bounding box, from `(0, 0)` at the top left to `(1, 1)` at the
bottom right. The shader only draws inside the shape, so a circle shows a disc cut from the
square it fills.

The uniform `shape` holds:

| Field | Type | Value |
| --- | --- | --- |
| `shape.color` | `vec4<f32>` | The fill color and opacity |
| `shape.size` | `vec2<f32>` | The box's width and height in scene units |
| `shape.time` | `f32` | Scene time in seconds |

`shape.time` is the scene time, not the time since the shape appeared, so a frame depends only
on `t` and seeking shows the same picture as playing. Use `shape.size` to keep a pattern the
same size in scene units whatever the box's shape: `(uv - 0.5) * shape.size` gives the
position from the box's center.

If the WGSL doesn't compile, the error goes to stderr once and the shape draws its solid fill.

## Style and animation

`.shader` keeps the fill color, using white if the shape had none, and passes it in as
`shape.color`. So `.fill` before or after `.shader` tints the shader, and the same color is
what SVG and the browser player draw. The fill's opacity and the shape's opacity multiply the
shader's alpha, so `fade_in`, `fade_out` and `.fill(color, opacity)` work as on a solid fill.
The outline is drawn on top as usual.

The box is fixed when `.shader` is called and moves, scales and rotates with the shape after
that, so the pattern turns with a `rotate` instead of staying upright. `create` draws the
outline and fill in the usual way, showing the shader as the fill grows.

In a `transform` between two shader fills, the box blends from one shape's to the other's, and
the shader source switches to the target's at the end. A `transform` between a shader fill and
a plain fill switches at the end too.

Each shader is drawn into a texture at the output's resolution, at most 2048 pixels a side.
The preview uses 1080p resolution whatever the window size.

## Python

The Python method has the same name and takes the same source string:

```python
disc = s.add(circle(1.5, fill=BLUE).shader(plasma))
```
