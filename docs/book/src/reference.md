# API reference

Every function, class and constant in the `fastanim` Python package. Methods are listed under
their class; `.name` without parentheses is a property. Shape functions all take the
`stroke=`, `stroke_width=`, `fill=` and `fill_opacity=` keywords. Anywhere a `Point` is taken,
an `(x, y)` tuple works too, and anywhere a `Color` is, a `"#rrggbb"` string.

## Functions

```text
apply_function(m, f)
arc(radius, start, sweep, *, stroke=None, stroke_width=None, fill=None, fill_opacity=None)
audio(path)
axes(x_range, y_range, width=None, height=None)
circle(radius, *, stroke=None, stroke_width=None, fill=None, fill_opacity=None)
circumscribe(m)
code(s, lang, *, font=None)
create(m)
dashed_line(a, b, dash_length=0.05, ratio=0.5, *, stroke=None, stroke_width=None)
difference(a, b, *, stroke=None, stroke_width=None, fill=None, fill_opacity=None)
dot(p, *, stroke=None, stroke_width=None, fill=None, fill_opacity=None)
draw_border_then_fill(m)
ease_in(name)
ease_in_out(name)
ease_out(name)
ellipse(width, height, *, stroke=None, stroke_width=None, fill=None, fill_opacity=None)
exclusion(a, b, *, stroke=None, stroke_width=None, fill=None, fill_opacity=None)
fade_in(m, shift=None, scale=None)
fade_out(m, shift=None, scale=None)
fit_axes(f, x_range, width=None, height=None)
flash(p)
function_graph(f, x0, x1, segments)
group(*items)
grow_from_center(m)
hex(rgb)
image(src, *, height=None, width=None, pixelated=False)
indicate(m)
intersection(a, b, *, stroke=None, stroke_width=None, fill=None, fill_opacity=None)
lagged_start(*anims, lag_ratio)
latex(s, *, font=None)
line(a, b, *, stroke=None, stroke_width=None, fill=None, fill_opacity=None)
list(values)
load_font(path)
lottie(path, *, height=None, width=None)
math_tex(s, *, font=None)
move_to(m, p)
parallel(*anims)
play_lottie(m, *, start=None, end=None, loops=None)
play_video(m, *, start=None, end=None, loops=None)
point(x, y)
polygon(points, *, stroke=None, stroke_width=None, fill=None, fill_opacity=None)
polyline(points, *, stroke=None, stroke_width=None, fill=None, fill_opacity=None)
rectangle(width, height, *, stroke=None, stroke_width=None, fill=None, fill_opacity=None)
replacement_transform(m, target)
rgb(r, g, b, a=1.0)
rotate(m, angle)
scale(m, k)
scene(f)
sequence(*anims)
shift(m, v)
shrink_to_center(m)
sound(audio, start=None, end=None)
spin_in(m)
spring(stiffness, damping)
square(side, *, stroke=None, stroke_width=None, fill=None, fill_opacity=None)
text(s, *, font=None)
transform(m, target)
transform_diff(m, to, **opts)
transform_list(m, to)
uncreate(m)
union(a, b, *, stroke=None, stroke_width=None, fill=None, fill_opacity=None)
unwrite(m)
update(m, f)
video(path, *, height=None, width=None, resolution=720)
wiggle(m)
write(m)
```

## Classes

```text
Animate
    .align_to(other, direction)
    .fill(color, opacity=None)
    .move_to(p)
    .next_to(other, direction, buff=DEFAULT_BUFF)
    .rotate(angle)
    .scale(k)
    .shift(v)
    .stroke(color, width=None)
    .to_edge(d)
Animation
    .rate(r)
    .run_time(t)
Audio
    .duration
Axes
    .align_to(other, direction)
    .c2p(x, y)
    .center()
    .height
    .labels(x, y)
    .move_to(p)
    .next_to(other, direction, buff=DEFAULT_BUFF)
    .numbers()
    .p2c(p)
    .plot(f, x0=None, x1=None, segments=64)
    .scale(k)
    .scale_to_fit_height(height)
    .scale_to_fit_width(width)
    .shape()
    .shift(v)
    .to_edge(d)
    .vertical_line(p)
    .width
Color
    .with_alpha(a)
Group
    len()
    .align_to(other, direction)
    .arrange(direction, buff=DEFAULT_BUFF)
    .center()
    .height
    .move_to(p)
    .next_to(other, direction, buff=DEFAULT_BUFF)
    .rotate(angle)
    .scale(k)
    .scale_to_fit_height(height)
    .scale_to_fit_width(width)
    .set_color_by_gradient(colors)
    .shift(v)
    .to_edge(d)
    .width
Lottie
    .align_to(other, direction)
    .at_frame(frame)
    .center()
    .duration
    .height
    .move_to(p)
    .next_to(other, direction, buff=DEFAULT_BUFF)
    .rotate(angle)
    .scale(k)
    .scale_to_fit_height(height)
    .scale_to_fit_width(width)
    .shift(v)
    .to_edge(d)
    .width
Mobject
    [i]
    len()
    .animate
Point
    + - * / and unary -
    .x
    .y
Rate
Scene
    .add(m)
    .add_sound(audio, start=None, end=None)
    .always(m, f=None)
    .background(color)
    .get(m)
    .marker(name)
    .play(*anims, run_time=None, rate=None)
    .remove(m)
    .time
    .wait(seconds=1.0)
    .wait_for_sounds()
SceneFn
    .bake()
    .preview(*, watch=True)
    .render(path=None, *, format=None, quality='1080p60', section=None)
    .still(path=None, *, format=None, at=0.0, frame=None, quality='1080p60')
Shape
    .align_to(other, direction)
    .center()
    .dashed(dashes=15, ratio=0.5)
    .fill(color, opacity=None)
    .fill_gradient(colors, direction=None)
    .fill_opacity(opacity)
    .height
    .line_cap(cap)
    .line_join(join)
    .move_to(p)
    .next_to(other, direction, buff=DEFAULT_BUFF)
    .pixelated()
    .rotate(angle)
    .scale(k)
    .scale_to_fit_height(height)
    .scale_to_fit_width(width)
    .shader(source)
    .shift(v)
    .stroke(color, width=None)
    .stroke_gradient(colors, direction=None)
    .stroke_opacity(opacity)
    .to_edge(d)
    .width
    .z_index(z)
State
    [m]
Text
    .align_to(other, direction)
    .center()
    .fill(color, opacity=None)
    .fill_gradient(colors, direction=None)
    .glyphs
    .height
    .move_to(p)
    .next_to(other, direction, buff=DEFAULT_BUFF)
    .rotate(angle)
    .scale(k)
    .scale_to_fit_height(height)
    .scale_to_fit_width(width)
    .set_color_by_gradient(colors)
    .shift(v)
    .to_edge(d)
    .width
Timeline
    .duration
    .marker(name)
    .markers
    .render(path=None, *, format=None, quality='1080p60', section=None)
    .still(path=None, *, format=None, at=0.0, frame=None, quality='1080p60')
    .svg(t, background=None)
```

## Constants

```text
BLACK
BLUE
DEFAULT_BUFF
DOWN
FRAME_HEIGHT
FRAME_WIDTH
GREEN
GREY
LEFT
LINEAR
ORANGE
ORIGIN
PI
PINK
PURPLE
RED
RIGHT
SMOOTH
TAU
THERE_AND_BACK
TRANSPARENT
UP
WHITE
YELLOW
```
