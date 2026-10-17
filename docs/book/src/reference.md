# API reference

Every function and constant a scene script can call, as printed by `fastanim script-api`.
The first argument of a method is the value it is called on: `fill(_: Shape, _: Color)` is
written `shape.fill(color)`. `get$x` is the property `.x`, and `index$get$` is `state[m]`.
Rhai's standard library is available too but not listed.

```text
*(_: Point, _: f64) -> Point
*(_: f64, _: Point) -> Point
+(_: Point, _: Point) -> Point
-(_: Point) -> Point
-(_: Point, _: Point) -> Point
/(_: Point, _: f64) -> Point
add(_: &mut Scene, _: Axes) -> Mobject
add(_: &mut Scene, _: Group) -> Mobject
add(_: &mut Scene, _: Lottie) -> Mobject
add(_: &mut Scene, _: Shape) -> Mobject
add(_: &mut Scene, _: Text) -> Mobject
add_sound(_: &mut Scene, _: Audio) -> ()
add_sound(_: &mut Scene, _: Audio, _: f64, _: f64) -> ()
align_to(_: Axes, _: Group, _: Point) -> Axes
align_to(_: Axes, _: Shape, _: Point) -> Axes
align_to(_: Axes, _: Text, _: Point) -> Axes
align_to(_: Group, _: Group, _: Point) -> Group
align_to(_: Group, _: Shape, _: Point) -> Group
align_to(_: Group, _: Text, _: Point) -> Group
align_to(_: Lottie, _: Group, _: Point) -> Lottie
align_to(_: Lottie, _: Shape, _: Point) -> Lottie
align_to(_: Lottie, _: Text, _: Point) -> Lottie
align_to(_: Shape, _: Group, _: Point) -> Shape
align_to(_: Shape, _: Shape, _: Point) -> Shape
align_to(_: Shape, _: Text, _: Point) -> Shape
align_to(_: Text, _: Group, _: Point) -> Text
align_to(_: Text, _: Shape, _: Point) -> Text
align_to(_: Text, _: Text, _: Point) -> Text
always(_: &mut Scene, _: Mobject, _: Fn) -> ()
apply_function(_: Mobject, _: Fn) -> Animation
arc(_: f64, _: f64, _: f64) -> Shape
arrange(_: Group, _: Point) -> Group
arrange(_: Group, _: Point, _: f64) -> Group
at_frame(_: Lottie, _: f64) -> Lottie
audio(_: string) -> Audio
axes(_: array, _: array) -> Axes
axes(_: array, _: array, _: f64, _: f64) -> Axes
c2p(_: &mut Axes, _: f64, _: f64) -> Point
center(_: Axes) -> Point
center(_: Group) -> Point
center(_: Lottie) -> Point
center(_: Shape) -> Point
center(_: Text) -> Point
circle(_: f64) -> Shape
circumscribe(_: Mobject) -> Animation
code(_: string, _: string) -> Text
code(_: string, _: string, _: map) -> Text
create(_: Mobject) -> Animation
dot(_: Point) -> Shape
draw_border_then_fill(_: Mobject) -> Animation
ease_in(_: string) -> Rate
ease_in_out(_: string) -> Rate
ease_out(_: string) -> Rate
fade_in(_: Mobject) -> Animation
fade_in(_: Mobject, _: Point) -> Animation
fade_in(_: Mobject, _: f64) -> Animation
fade_in(_: Mobject, _: Point, _: f64) -> Animation
fade_out(_: Mobject) -> Animation
fade_out(_: Mobject, _: Point) -> Animation
fade_out(_: Mobject, _: f64) -> Animation
fade_out(_: Mobject, _: Point, _: f64) -> Animation
fill(_: Shape, _: Color) -> Shape
fill(_: Text, _: Color) -> Text
fit_axes(_: Fn, _: array) -> Axes
fit_axes(_: Fn, _: array, _: f64, _: f64) -> Axes
flash(_: Point) -> Animation
function_graph(_: Fn, _: f64, _: f64, _: i64) -> Shape
get$duration(_: &mut Audio) -> f64
get$duration(_: &mut Lottie) -> f64
get$glyphs(_: &mut Text) -> array
get$time(_: &mut Scene) -> f64
get$x(_: &mut Point) -> f64
get$y(_: &mut Point) -> f64
get(_: &mut Scene, _: Mobject) -> Shape
group(_: array) -> types::dynamic::Dynamic
grow_from_center(_: Mobject) -> Animation
hex(_: i64) -> Color
image(_: array) -> Shape
image(_: string) -> Shape
index$get$(_: &mut Mobject, _: i64) -> Mobject
index$get$(_: &mut State, _: Mobject) -> Shape
indicate(_: Mobject) -> Animation
labels(_: &mut Axes, _: string, _: string) -> Text
lagged_start(_: f64, _: array) -> Animation
latex(_: string) -> Text
latex(_: string, _: map) -> Text
len(_: &mut Group) -> i64
len(_: &mut Mobject) -> i64
line(_: Point, _: Point) -> Shape
list(_: array) -> Text
load_font(_: string) -> array
lottie(_: string) -> Lottie
marker(_: &mut Scene, _: string) -> ()
math_tex(_: string) -> Text
math_tex(_: string, _: map) -> Text
move_to(_: Axes, _: Point) -> Axes
move_to(_: Group, _: Point) -> Group
move_to(_: Lottie, _: Point) -> Lottie
move_to(_: Mobject, _: Point) -> Animation
move_to(_: Shape, _: Point) -> Shape
move_to(_: Text, _: Point) -> Text
named(_: Animation, _: string) -> Animation
next_to(_: Axes, _: Group, _: Point) -> Axes
next_to(_: Axes, _: Group, _: Point, _: f64) -> Axes
next_to(_: Axes, _: Point, _: Point) -> Axes
next_to(_: Axes, _: Shape, _: Point) -> Axes
next_to(_: Axes, _: Shape, _: Point, _: f64) -> Axes
next_to(_: Axes, _: Text, _: Point) -> Axes
next_to(_: Axes, _: Text, _: Point, _: f64) -> Axes
next_to(_: Group, _: Group, _: Point) -> Group
next_to(_: Group, _: Group, _: Point, _: f64) -> Group
next_to(_: Group, _: Point, _: Point) -> Group
next_to(_: Group, _: Shape, _: Point) -> Group
next_to(_: Group, _: Shape, _: Point, _: f64) -> Group
next_to(_: Group, _: Text, _: Point) -> Group
next_to(_: Group, _: Text, _: Point, _: f64) -> Group
next_to(_: Lottie, _: Group, _: Point) -> Lottie
next_to(_: Lottie, _: Group, _: Point, _: f64) -> Lottie
next_to(_: Lottie, _: Point, _: Point) -> Lottie
next_to(_: Lottie, _: Shape, _: Point) -> Lottie
next_to(_: Lottie, _: Shape, _: Point, _: f64) -> Lottie
next_to(_: Lottie, _: Text, _: Point) -> Lottie
next_to(_: Lottie, _: Text, _: Point, _: f64) -> Lottie
next_to(_: Shape, _: Group, _: Point) -> Shape
next_to(_: Shape, _: Group, _: Point, _: f64) -> Shape
next_to(_: Shape, _: Point, _: Point) -> Shape
next_to(_: Shape, _: Shape, _: Point) -> Shape
next_to(_: Shape, _: Shape, _: Point, _: f64) -> Shape
next_to(_: Shape, _: Text, _: Point) -> Shape
next_to(_: Shape, _: Text, _: Point, _: f64) -> Shape
next_to(_: Text, _: Group, _: Point) -> Text
next_to(_: Text, _: Group, _: Point, _: f64) -> Text
next_to(_: Text, _: Point, _: Point) -> Text
next_to(_: Text, _: Shape, _: Point) -> Text
next_to(_: Text, _: Shape, _: Point, _: f64) -> Text
next_to(_: Text, _: Text, _: Point) -> Text
next_to(_: Text, _: Text, _: Point, _: f64) -> Text
numbers(_: &mut Axes) -> Text
p2c(_: &mut Axes, _: Point) -> Point
parallel(_: array) -> Animation
pixelated(_: Shape) -> Shape
play(_: &mut Scene, _: Animation) -> ()
play(_: &mut Scene, _: array) -> ()
play_lottie(_: Mobject) -> Animation
play_lottie(_: Mobject, _: map) -> Animation
plot(_: &mut Axes, _: Fn) -> Shape
plot(_: &mut Axes, _: Fn, _: f64, _: f64) -> Shape
plot(_: &mut Axes, _: Fn, _: f64, _: f64, _: i64) -> Shape
point(_: f64, _: f64) -> Point
polygon(_: array) -> Shape
rate(_: Animation, _: Rate) -> Animation
rectangle(_: f64, _: f64) -> Shape
remove(_: &mut Scene, _: Mobject) -> ()
replacement_transform(_: Mobject, _: Mobject) -> Animation
rgb(_: f64, _: f64, _: f64) -> Color
rotate(_: Group, _: f64) -> Group
rotate(_: Lottie, _: f64) -> Lottie
rotate(_: Mobject, _: f64) -> Animation
rotate(_: Shape, _: f64) -> Shape
rotate(_: Text, _: f64) -> Text
run_time(_: Animation, _: f64) -> Animation
scale(_: Axes, _: f64) -> Axes
scale(_: Group, _: f64) -> Group
scale(_: Lottie, _: f64) -> Lottie
scale(_: Mobject, _: f64) -> Animation
scale(_: Shape, _: f64) -> Shape
scale(_: Text, _: f64) -> Text
scale_to_fit_height(_: Axes, _: f64) -> Axes
scale_to_fit_height(_: Group, _: f64) -> Group
scale_to_fit_height(_: Lottie, _: f64) -> Lottie
scale_to_fit_height(_: Shape, _: f64) -> Shape
scale_to_fit_height(_: Text, _: f64) -> Text
scale_to_fit_width(_: Axes, _: f64) -> Axes
scale_to_fit_width(_: Group, _: f64) -> Group
scale_to_fit_width(_: Lottie, _: f64) -> Lottie
scale_to_fit_width(_: Shape, _: f64) -> Shape
scale_to_fit_width(_: Text, _: f64) -> Text
sequence(_: array) -> Animation
shape(_: &mut Axes) -> Shape
shift(_: Axes, _: Point) -> Axes
shift(_: Group, _: Point) -> Group
shift(_: Lottie, _: Point) -> Lottie
shift(_: Mobject, _: Point) -> Animation
shift(_: Shape, _: Point) -> Shape
shift(_: Text, _: Point) -> Text
shrink_to_center(_: Mobject) -> Animation
sound(_: Audio) -> Animation
sound(_: Audio, _: f64, _: f64) -> Animation
spin_in(_: Mobject) -> Animation
spring(_: f64, _: f64) -> Rate
square(_: f64) -> Shape
stroke(_: Shape, _: Color, _: f64) -> Shape
text(_: string) -> Text
text(_: string, _: map) -> Text
to_debug(_: &mut Point) -> string
to_edge(_: Axes, _: Point) -> Axes
to_edge(_: Group, _: Point) -> Group
to_edge(_: Lottie, _: Point) -> Lottie
to_edge(_: Shape, _: Point) -> Shape
to_edge(_: Text, _: Point) -> Text
to_string(_: &mut Point) -> string
transform(_: Mobject, _: Shape) -> Animation
transform_diff(_: Mobject, _: Text) -> Animation
transform_diff(_: Mobject, _: Text, _: map) -> Animation
transform_list(_: Mobject, _: Text) -> Animation
uncreate(_: Mobject) -> Animation
unwrite(_: Mobject) -> Animation
update(_: Mobject, _: Fn) -> Animation
vertical_line(_: &mut Axes, _: Point) -> Shape
wait(_: &mut Scene) -> ()
wait(_: &mut Scene, _: f64) -> ()
wait_for_sounds(_: &mut Scene) -> ()
wiggle(_: Mobject) -> Animation
with_alpha(_: Color, _: f64) -> Color
write(_: Mobject) -> Animation
z_index(_: Shape, _: i64) -> Shape
const UP
const DOWN
const LEFT
const RIGHT
const ORIGIN
const WHITE
const BLACK
const BLUE
const RED
const GREEN
const YELLOW
const GREY
const ORANGE
const TRANSPARENT
const LINEAR
const SMOOTH
const THERE_AND_BACK
const DEFAULT_BUFF
const FRAME_WIDTH
const FRAME_HEIGHT
const PI
const TAU
```
