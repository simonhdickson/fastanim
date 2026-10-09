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
add(_: &mut Scene, _: Shape) -> Mobject
add(_: &mut Scene, _: Text) -> Mobject
align_to(_: Axes, _: Shape, _: Point) -> Axes
align_to(_: Axes, _: Text, _: Point) -> Axes
align_to(_: Shape, _: Shape, _: Point) -> Shape
align_to(_: Shape, _: Text, _: Point) -> Shape
align_to(_: Text, _: Shape, _: Point) -> Text
align_to(_: Text, _: Text, _: Point) -> Text
always(_: &mut Scene, _: Mobject, _: Fn) -> ()
apply_function(_: Mobject, _: Fn) -> Animation
arc(_: f64, _: f64, _: f64) -> Shape
axes(_: array, _: array) -> Axes
axes(_: array, _: array, _: f64, _: f64) -> Axes
c2p(_: &mut Axes, _: f64, _: f64) -> Point
circle(_: f64) -> Shape
circumscribe(_: Mobject) -> Animation
code(_: string, _: string) -> Text
create(_: Mobject) -> Animation
dot(_: Point) -> Shape
draw_border_then_fill(_: Mobject) -> Animation
ease_in(_: string) -> Rate
ease_in_out(_: string) -> Rate
ease_out(_: string) -> Rate
fade_in(_: Mobject) -> Animation
fade_out(_: Mobject) -> Animation
fill(_: Shape, _: Color) -> Shape
fill(_: Text, _: Color) -> Text
flash(_: Point) -> Animation
function_graph(_: Fn, _: f64, _: f64, _: i64) -> Shape
get$glyphs(_: &mut Text) -> array
get$time(_: &mut Scene) -> f64
get$x(_: &mut Point) -> f64
get$y(_: &mut Point) -> f64
get(_: &mut Scene, _: Mobject) -> Shape
grow_from_center(_: Mobject) -> Animation
hex(_: i64) -> Color
index$get$(_: &mut State, _: Mobject) -> Shape
indicate(_: Mobject) -> Animation
labels(_: &mut Axes, _: string, _: string) -> Text
lagged_start(_: f64, _: array) -> Animation
latex(_: string) -> Text
line(_: Point, _: Point) -> Shape
list(_: array) -> Text
marker(_: &mut Scene, _: string) -> ()
math_tex(_: string) -> Text
move_to(_: Axes, _: Point) -> Axes
move_to(_: Mobject, _: Point) -> Animation
move_to(_: Shape, _: Point) -> Shape
move_to(_: Text, _: Point) -> Text
next_to(_: Axes, _: Point, _: Point) -> Axes
next_to(_: Axes, _: Shape, _: Point) -> Axes
next_to(_: Axes, _: Shape, _: Point, _: f64) -> Axes
next_to(_: Axes, _: Text, _: Point) -> Axes
next_to(_: Axes, _: Text, _: Point, _: f64) -> Axes
next_to(_: Shape, _: Point, _: Point) -> Shape
next_to(_: Shape, _: Shape, _: Point) -> Shape
next_to(_: Shape, _: Shape, _: Point, _: f64) -> Shape
next_to(_: Shape, _: Text, _: Point) -> Shape
next_to(_: Shape, _: Text, _: Point, _: f64) -> Shape
next_to(_: Text, _: Point, _: Point) -> Text
next_to(_: Text, _: Shape, _: Point) -> Text
next_to(_: Text, _: Shape, _: Point, _: f64) -> Text
next_to(_: Text, _: Text, _: Point) -> Text
next_to(_: Text, _: Text, _: Point, _: f64) -> Text
numbers(_: &mut Axes) -> Text
p2c(_: &mut Axes, _: Point) -> Point
parallel(_: array) -> Animation
play(_: &mut Scene, _: Animation) -> ()
play(_: &mut Scene, _: array) -> ()
plot(_: &mut Axes, _: Fn, _: f64, _: f64) -> Shape
point(_: f64, _: f64) -> Point
polygon(_: array) -> Shape
rate(_: Animation, _: Rate) -> Animation
rectangle(_: f64, _: f64) -> Shape
remove(_: &mut Scene, _: Mobject) -> ()
replacement_transform(_: Mobject, _: Mobject) -> Animation
rgb(_: f64, _: f64, _: f64) -> Color
rotate(_: Mobject, _: f64) -> Animation
rotate(_: Shape, _: f64) -> Shape
rotate(_: Text, _: f64) -> Text
run_time(_: Animation, _: f64) -> Animation
scale(_: Axes, _: f64) -> Axes
scale(_: Mobject, _: f64) -> Animation
scale(_: Shape, _: f64) -> Shape
scale(_: Text, _: f64) -> Text
sequence(_: array) -> Animation
shape(_: &mut Axes) -> Shape
shift(_: Axes, _: Point) -> Axes
shift(_: Mobject, _: Point) -> Animation
shift(_: Shape, _: Point) -> Shape
shift(_: Text, _: Point) -> Text
shrink_to_center(_: Mobject) -> Animation
spin_in(_: Mobject) -> Animation
spring(_: f64, _: f64) -> Rate
square(_: f64) -> Shape
stroke(_: Shape, _: Color, _: f64) -> Shape
text(_: string) -> Text
to_debug(_: &mut Point) -> string
to_edge(_: Axes, _: Point) -> Axes
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
