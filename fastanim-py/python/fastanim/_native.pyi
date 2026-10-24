"""Types for the native module, for editors and type checkers; `fastanim` re-exports all of it.
Kept in step with src/lib.rs by the `stubs_match_the_module` test."""

import builtins
from collections.abc import Callable, Iterator, Sequence
from os import PathLike
from typing import Any, Protocol, Self, TypeAlias, TypeVar, overload

class _ArrayLike(Protocol):
    def tolist(self) -> Any: ...

_P: TypeAlias = Point | tuple[float, float] | Sequence[float]
_C: TypeAlias = Color | str | tuple[float, float, float] | tuple[float, float, float, float]
_Path: TypeAlias = str | PathLike[str]
_Range: TypeAlias = tuple[float, float] | tuple[float, float, float] | Sequence[float]
_Other: TypeAlias = Shape | Text | Axes | Group | _P
_Addable: TypeAlias = Shape | Text | Axes | Group | Lottie
_GroupItem: TypeAlias = Shape | Text | Axes | Group
_Anim: TypeAlias = Animation | Animate
# `"480p15"`, `"720p30"`, `"1080p60"` or `"4k60"`.
_Quality: TypeAlias = str
_Updater: TypeAlias = Callable[[State, float], Shape]
_F = TypeVar("_F", bound=_Updater)

class SceneError(ValueError):
    """Misuse of the scene API, such as playing a removed mobject."""

class TypesetError(ValueError):
    """Typst failed to typeset a snippet."""

# Values.

class Point:
    """A point or vector."""

    def __new__(cls, x: float, y: float) -> Self: ...
    @property
    def x(self) -> float: ...
    @property
    def y(self) -> float: ...
    def __add__(self, o: _P) -> Point: ...
    def __radd__(self, o: _P) -> Point: ...
    def __sub__(self, o: _P) -> Point: ...
    def __rsub__(self, o: _P) -> Point: ...
    def __mul__(self, k: float) -> Point: ...
    def __rmul__(self, k: float) -> Point: ...
    def __truediv__(self, k: float) -> Point: ...
    def __neg__(self) -> Point: ...
    def __abs__(self) -> float: ...
    def __len__(self) -> int: ...
    def __iter__(self) -> Iterator[float]: ...

class Color:
    """A color."""

    def with_alpha(self, a: float) -> Color: ...

class Rate:
    """A rate function: how an animation's progress follows time."""

class Audio:
    """A loaded WAV file."""

    @property
    def duration(self) -> float:
        """Length in seconds."""

# Shapes, texts, axes, groups and Lotties: positioned before they are added to a scene.

class _Positioned:
    def shift(self, v: _P) -> Self: ...
    def move_to(self, p: _P) -> Self: ...
    def scale(self, k: float) -> Self: ...
    def to_edge(self, d: _P) -> Self: ...
    def next_to(self, other: _Other, direction: _P, buff: float = ...) -> Self: ...
    def align_to(self, other: _Other, direction: _P) -> Self: ...
    def scale_to_fit_height(self, height: float) -> Self: ...
    def scale_to_fit_width(self, width: float) -> Self: ...
    def center(self) -> Point: ...
    @property
    def width(self) -> float: ...
    @property
    def height(self) -> float: ...

class Shape(_Positioned):
    """A shape, before or after it is added to a scene."""

    def fill(self, color: _C, opacity: float | None = None) -> Shape: ...
    def fill_opacity(self, opacity: float) -> Shape: ...
    def stroke(self, color: _C, width: float | None = None) -> Shape: ...
    def stroke_opacity(self, opacity: float) -> Shape: ...
    def fill_gradient(self, colors: Sequence[_C], direction: _P | None = None) -> Shape: ...
    def stroke_gradient(self, colors: Sequence[_C], direction: _P | None = None) -> Shape: ...
    def shader(self, source: str) -> Shape:
        """Paints the fill with a WGSL shader defining `fn shade(uv: vec2<f32>) -> vec4<f32>`."""
    def line_cap(self, cap: str) -> Shape:
        """`"round"` (default), `"butt"` or `"square"`."""
    def line_join(self, join: str) -> Shape:
        """`"round"` (default), `"miter"` or `"bevel"`."""
    def dashed(self, dashes: int = 15, ratio: float = 0.5) -> Shape: ...
    def z_index(self, z: int) -> Shape: ...
    def pixelated(self) -> Shape: ...
    def rotate(self, angle: float) -> Shape: ...

class Text(_Positioned):
    """Typeset text, before it is added to a scene."""

    def fill(self, color: _C, opacity: float | None = None) -> Text: ...
    def fill_gradient(self, colors: Sequence[_C], direction: _P | None = None) -> Text: ...
    def set_color_by_gradient(self, colors: Sequence[_C]) -> Text: ...
    def rotate(self, angle: float) -> Text: ...
    @property
    def glyphs(self) -> builtins.list[Shape]: ...

class Axes(_Positioned):
    """Coordinate axes."""

    def shape(self) -> Shape:
        """The axes as one shape."""
    def c2p(self, x: float, y: float) -> Point:
        """Coordinates to a point in the scene."""
    def p2c(self, p: _P) -> Point:
        """A point in the scene to coordinates."""
    def plot(
        self,
        f: Callable[[float], float],
        x0: float | None = None,
        x1: float | None = None,
        segments: int = 64,
    ) -> Shape:
        """The graph of `f` from `x0` to `x1` (default: the whole x range)."""
    def vertical_line(self, p: _P) -> Shape: ...
    def numbers(self) -> Text:
        """Tick numbers along both axes."""
    def labels(self, x: str, y: str) -> Text:
        """Math labels for the x and y axes."""

class Group(_Positioned):
    """Shapes, texts and groups positioned as one, before they are added to a scene."""

    def rotate(self, angle: float) -> Group: ...
    def arrange(self, direction: _P, buff: float = ...) -> Group:
        """Lays the items out in a row in `direction`, `buff` apart."""
    def set_color_by_gradient(self, colors: Sequence[_C]) -> Group: ...
    def __len__(self) -> int: ...

class Lottie(_Positioned):
    """A Lottie animation, before it is added to a scene."""

    def rotate(self, angle: float) -> Lottie: ...
    def at_frame(self, frame: float) -> Lottie:
        """Shows frame `frame` until it plays."""
    @property
    def duration(self) -> float:
        """Length in seconds at its own frame rate."""

# The scene.

class Mobject:
    """A handle to something in a scene, returned by `Scene.add`."""

    def __getitem__(self, i: int) -> Mobject:
        """Member `i` of a group; negative counts from the end."""
    def __len__(self) -> int:
        """How many members a group has; 1 for anything else."""
    @property
    def animate(self) -> Animate:
        """Chain methods on this to get an animation to their result."""

class Animate:
    """A mobject's `.animate`: an animation to the result of the methods chained on it."""

    def shift(self, v: _P) -> Animate: ...
    def move_to(self, p: _P) -> Animate: ...
    def scale(self, k: float) -> Animate: ...
    def rotate(self, angle: float) -> Animate: ...
    def to_edge(self, d: _P) -> Animate: ...
    def next_to(self, other: _Other, direction: _P, buff: float = ...) -> Animate: ...
    def align_to(self, other: _Other, direction: _P) -> Animate: ...
    def fill(self, color: _C, opacity: float | None = None) -> Animate: ...
    def stroke(self, color: _C, width: float | None = None) -> Animate: ...

class Animation:
    """An animation, played with `Scene.play`."""

    def run_time(self, t: float) -> Animation: ...
    def rate(self, r: Rate) -> Animation: ...

class State:
    """What an updater sees: index it with a shape's mobject to get its shape now."""

    def __getitem__(self, m: Mobject) -> Shape: ...

class Scene:
    """A scene being recorded; the argument of an `@scene` function."""

    def add(self, m: _Addable) -> Mobject:
        """Adds a shape, text, axes, group or Lottie, returning its handle."""
    def remove(self, m: Mobject) -> None: ...
    def get(self, m: Mobject) -> Shape:
        """A shape's state now."""
    @property
    def time(self) -> float:
        """Where the scene is up to, in seconds."""
    def play(self, *anims: _Anim, run_time: float | None = None, rate: Rate | None = None) -> None:
        """Plays `anims` together."""
    def wait(self, seconds: float = 1.0) -> None: ...
    def background(self, color: _C) -> None:
        """Sets the color behind every frame, black by default; one color for the whole scene."""
    def marker(self, name: str) -> None:
        """Names this point in time."""
    def add_sound(self, audio: Audio, start: float | None = None, end: float | None = None) -> None:
        """Starts `audio`, or seconds `start..end` of it, without waiting for it."""
    def wait_for_sounds(self) -> None:
        """Waits until every sound started so far has finished."""
    @overload
    def always(self, m: Mobject, f: _Updater) -> None:
        """Keeps shape `m` at `f(state, t)` from now on."""
    @overload
    def always(self, m: Mobject) -> Callable[[_F], _F]:
        """`@s.always(m)`: keeps shape `m` at the decorated `f(state, t)` from now on."""

class Timeline:
    """A baked scene: what playback and export read."""

    @property
    def duration(self) -> float: ...
    def marker(self, name: str) -> float | None:
        """When marker `name` is, if there is one."""
    @property
    def markers(self) -> builtins.list[tuple[str, float]]: ...
    def svg(self, t: float, background: _C | None = None) -> str:
        """The frame at `t` as SVG, over the scene's background unless `background` is given."""
    @property
    def background_css(self) -> str:
        """The background as an opaque CSS color, for `frame`'s canvas."""
    @property
    def events(self) -> builtins.list[dict[str, Any]]:
        """Every play, updater and sound, in start order."""
    def frame(self, t: float) -> builtins.list[dict[str, Any]]:
        """What to draw at `t`, back to front, for a canvas player such as the browser's."""
    def image(self, key: int) -> tuple[int, int, bytes] | None:
        """The image `key` from `frame` as `(width, height, rgba)`."""
    def render(
        self,
        path: _Path = "out.mp4",
        *,
        quality: _Quality = "1080p60",
        section: str | None = None,
    ) -> None:
        """Exports the scene, or from marker `section` to the next one; the extension picks the
        format: `mp4`, `webm`, `gif`, `png` or `svg`."""
    def still(
        self,
        path: _Path = "frame.png",
        *,
        at: float = 0.0,
        frame: int | None = None,
        quality: _Quality = "1080p60",
    ) -> None:
        """Writes the frame at `at` seconds, or frame index `frame`, as `png` or `svg`."""

class SceneFn:
    """A scene function, made with `@scene`."""

    def bake(self) -> Timeline:
        """Runs the scene and bakes what it recorded."""
    def render(
        self,
        path: _Path = "out.mp4",
        *,
        quality: _Quality = "1080p60",
        section: str | None = None,
    ) -> None:
        """Bakes the scene and exports it; see `Timeline.render`."""
    def still(
        self,
        path: _Path = "frame.png",
        *,
        at: float = 0.0,
        frame: int | None = None,
        quality: _Quality = "1080p60",
    ) -> None:
        """Bakes the scene and writes one frame; see `Timeline.still`."""
    def preview(self, *, watch: bool = True) -> None:
        """Opens a window that plays the scene with a scrubber, and blocks until it closes."""
    @property
    def __name__(self) -> str: ...
    @property
    def __wrapped__(self) -> Callable[[Scene], object]: ...

def scene(f: Callable[[Scene], object]) -> SceneFn:
    """Marks `f(scene)` as a scene: `f.bake()`, `f.still(path, at=t)`, `f.render(path)`,
    `f.preview()`."""

def point(x: float, y: float) -> Point: ...
def rgb(r: float, g: float, b: float, a: float = 1.0) -> Color: ...
def hex(rgb: int) -> Color: ...

# Shapes. Each takes `stroke=`, `stroke_width=`, `fill=` and `fill_opacity=`.

def circle(
    radius: float,
    *,
    stroke: _C | None = None,
    stroke_width: float | None = None,
    fill: _C | None = None,
    fill_opacity: float | None = None,
) -> Shape: ...
def square(
    side: float,
    *,
    stroke: _C | None = None,
    stroke_width: float | None = None,
    fill: _C | None = None,
    fill_opacity: float | None = None,
) -> Shape: ...
def rectangle(
    width: float,
    height: float,
    *,
    stroke: _C | None = None,
    stroke_width: float | None = None,
    fill: _C | None = None,
    fill_opacity: float | None = None,
) -> Shape: ...
def ellipse(
    width: float,
    height: float,
    *,
    stroke: _C | None = None,
    stroke_width: float | None = None,
    fill: _C | None = None,
    fill_opacity: float | None = None,
) -> Shape: ...
def union(
    a: Shape,
    b: Shape,
    *,
    stroke: _C | None = None,
    stroke_width: float | None = None,
    fill: _C | None = None,
    fill_opacity: float | None = None,
) -> Shape:
    """The area covered by either shape."""
def intersection(
    a: Shape,
    b: Shape,
    *,
    stroke: _C | None = None,
    stroke_width: float | None = None,
    fill: _C | None = None,
    fill_opacity: float | None = None,
) -> Shape:
    """The area covered by both shapes."""
def difference(
    a: Shape,
    b: Shape,
    *,
    stroke: _C | None = None,
    stroke_width: float | None = None,
    fill: _C | None = None,
    fill_opacity: float | None = None,
) -> Shape:
    """The area covered by `a` but not `b`."""
def exclusion(
    a: Shape,
    b: Shape,
    *,
    stroke: _C | None = None,
    stroke_width: float | None = None,
    fill: _C | None = None,
    fill_opacity: float | None = None,
) -> Shape:
    """The area covered by exactly one of the shapes."""
def arc(
    radius: float,
    start: float,
    sweep: float,
    *,
    stroke: _C | None = None,
    stroke_width: float | None = None,
    fill: _C | None = None,
    fill_opacity: float | None = None,
) -> Shape:
    """An arc of a circle, from angle `start` sweeping `sweep` radians."""
def line(
    a: _P,
    b: _P,
    *,
    stroke: _C | None = None,
    stroke_width: float | None = None,
    fill: _C | None = None,
    fill_opacity: float | None = None,
) -> Shape: ...
def dot(
    p: _P,
    *,
    stroke: _C | None = None,
    stroke_width: float | None = None,
    fill: _C | None = None,
    fill_opacity: float | None = None,
) -> Shape:
    """A small filled dot at `p`."""
def polygon(
    points: Sequence[_P],
    *,
    stroke: _C | None = None,
    stroke_width: float | None = None,
    fill: _C | None = None,
    fill_opacity: float | None = None,
) -> Shape: ...
def polyline(
    points: Sequence[_P],
    *,
    stroke: _C | None = None,
    stroke_width: float | None = None,
    fill: _C | None = None,
    fill_opacity: float | None = None,
) -> Shape:
    """Straight segments through `points`, left open."""
def dashed_line(
    a: _P,
    b: _P,
    dash_length: float = 0.05,
    ratio: float = 0.5,
    *,
    stroke: _C | None = None,
    stroke_width: float | None = None,
) -> Shape:
    """A dashed line from `a` to `b`, with dashes about `dash_length` long and `ratio` of each
    period."""
def function_graph(f: Callable[[float], float], x0: float, x1: float, segments: int) -> Shape:
    """The graph of `f` from `x0` to `x1`, with `segments` cubic pieces."""
def image(
    src: _Path | _ArrayLike | Sequence[Sequence[Any]],
    *,
    height: float | None = None,
    width: float | None = None,
    pixelated: bool = False,
) -> Shape:
    """An image: a PNG or JPEG file's path, or rows of pixels (a NumPy array or nested lists), each
    a grey value or `(r, g, b[, a])` from 0 to 255."""
def lottie(path: _Path, *, height: float | None = None, width: float | None = None) -> Lottie: ...
def load_font(path: _Path) -> builtins.list[str]:
    """Loads a font file, returning the families it holds."""

# Text.

def text(s: str, *, font: str | None = None) -> Text: ...
def math_tex(s: str, *, font: str | None = None) -> Text: ...
def latex(s: str, *, font: str | None = None) -> Text: ...
def code(s: str, lang: str, *, font: str | None = None) -> Text: ...
def list(values: Sequence[object]) -> Text:
    """A row of boxed values, for `transform_list`."""
@overload
def group(*items: Mobject) -> Mobject:
    """Mobjects already added, as one group mobject."""
@overload
def group(*items: _GroupItem) -> Group:
    """Shapes, texts, axes and groups, positioned as one."""

# Axes.

def axes(
    x_range: _Range, y_range: _Range, width: float | None = None, height: float | None = None
) -> Axes: ...
def fit_axes(
    f: Callable[[float], float],
    x_range: _Range,
    width: float | None = None,
    height: float | None = None,
) -> Axes:
    """Axes over `x_range` with the y range picked to fit `f`."""

# Rates.

def ease_in(name: str) -> Rate:
    """`name` is `"quad"`, `"cubic"`, `"expo"` or `"back"`."""
def ease_out(name: str) -> Rate:
    """`name` is `"quad"`, `"cubic"`, `"expo"` or `"back"`."""
def ease_in_out(name: str) -> Rate:
    """`name` is `"quad"`, `"cubic"`, `"expo"` or `"back"`."""
def spring(stiffness: float, damping: float) -> Rate: ...

# Animations.

def create(m: Mobject) -> Animation: ...
def uncreate(m: Mobject) -> Animation: ...
def write(m: Mobject) -> Animation: ...
def unwrite(m: Mobject) -> Animation: ...
def fade_in(m: Mobject, shift: _P | None = None, scale: float | None = None) -> Animation: ...
def fade_out(m: Mobject, shift: _P | None = None, scale: float | None = None) -> Animation: ...
def grow_from_center(m: Mobject) -> Animation: ...
def shrink_to_center(m: Mobject) -> Animation: ...
def spin_in(m: Mobject) -> Animation: ...
def draw_border_then_fill(m: Mobject) -> Animation: ...
def indicate(m: Mobject) -> Animation: ...
def wiggle(m: Mobject) -> Animation: ...
def circumscribe(m: Mobject) -> Animation: ...
def shift(m: Mobject, v: _P) -> Animation: ...
def rotate(m: Mobject, angle: float) -> Animation: ...
def scale(m: Mobject, k: float) -> Animation: ...
def move_to(m: Mobject, p: _P) -> Animation: ...
def flash(p: _P) -> Animation: ...
def transform(m: Mobject, target: Shape) -> Animation: ...
def replacement_transform(m: Mobject, target: Mobject) -> Animation: ...
def apply_function(m: Mobject, f: Callable[[Point], _P]) -> Animation: ...
def update(m: Mobject, f: Callable[[Shape, float], Shape]) -> Animation:
    """`f(start, alpha)` returns `m`'s shape at progress `alpha` (0 to 1)."""
def transform_diff(
    m: Mobject,
    to: Text,
    *,
    highlight_changes: bool = False,
    lag_ratio: float = 0.05,
    move_arc: float = ...,
    debug: bool = False,
) -> Animation:
    """Morphs text `m` into `to`, moving unchanged tokens."""
def transform_list(m: Mobject, to: Text) -> Animation:
    """Morphs list `m` into list `to`, moving cells by value."""
def play_lottie(
    m: Mobject, *, start: float | None = None, end: float | None = None, loops: int | None = None
) -> Animation:
    """Plays Lottie `m` at its own frame rate: frames `start` to `end`, `loops` times over."""
def parallel(*anims: _Anim) -> Animation: ...
def sequence(*anims: _Anim) -> Animation: ...
def lagged_start(*anims: _Anim, lag_ratio: float) -> Animation:
    """Starts each of `anims` `lag_ratio` of the way through the one before."""

# Sound.

def audio(path: _Path) -> Audio:
    """Loads the WAV at `path`, relative to the current directory."""
def sound(audio: Audio, start: float | None = None, end: float | None = None) -> Animation:
    """Plays `audio`, or seconds `start..end` of it, lasting until it finishes."""

# Constants.

UP: Point
DOWN: Point
LEFT: Point
RIGHT: Point
ORIGIN: Point
WHITE: Color
BLACK: Color
BLUE: Color
RED: Color
GREEN: Color
YELLOW: Color
GREY: Color
ORANGE: Color
PINK: Color
PURPLE: Color
TRANSPARENT: Color
LINEAR: Rate
SMOOTH: Rate
THERE_AND_BACK: Rate
DEFAULT_BUFF: float
FRAME_WIDTH: float
FRAME_HEIGHT: float
PI: float
TAU: float
