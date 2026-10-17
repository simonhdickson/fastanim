# Roadmap: manim parity (M11)

The goal is to run most of the 2D scenes in
[manim's example gallery](https://docs.manim.community/en/stable/examples.html), with images
and visual polish getting the most attention. 3D is still out of scope (SPEC §1.2). API
compatibility with manim is not a goal: each feature is added the fastanim way, with frames
that are pure in `t`, diff-driven transforms, and the same API in Rust, Rhai and Python.

## Where we are

- **Shapes:** `circle`, `arc`, `square`, `rectangle`, `polygon`, `line`, `dot`,
  `function_graph`, `image`, `lottie` (played with `play_lottie`), and `axes` (`plot`, `c2p`/`p2c`, `vertical_line`, `numbers`,
  `labels`, arrow tips).
- **Text:** `text`, `math_tex`, `latex`, `code`, `list` (Typst, per-glyph shapes), in any
  bundled font or one loaded with `load_font`.
- **Groups:** `group([...])` of values (with `arrange`) or of added mobjects; indexable,
  accepted by every animation that does not need a single shape or text.
- **Animations:** creation/removal, fades, `transform`, `replacement_transform`,
  `transform_diff`, grow/shrink, `shift`/`rotate`/`scale`/`move_to`, `apply_function`,
  `indicate`, `wiggle`, `circumscribe`, `flash`, `update`, `parallel`, `sequence`,
  `lagged_start`, easing.
- **Updaters:** `scene.always(m, |state, t| ...)`. These are pure in `t`; there is no `dt`.

What limits most of the remaining work is the data model. A `VState`
(`fastanim-core/src/mobject.rs`) holds one path, a solid fill, one stroke, opacity, z-index and
a draw range, plus an optional image. There are no gradients or dashes. Texts are the only handle that covers
more than one mobject (`Mob::Text` in `fastanim-script/src/lib.rs`). There is no camera. Every
new drawable kind or style has to be added to all three renderers: Vello
(`fastanim-bevy/src/lib.rs`), SVG (`fastanim-core/src/svg.rs`) and Canvas 2D
(`fastanim-web/player.js`).

## Gallery checklist

The number after each example is the roadmap item it waits on.

| Example | Status | Missing |
|---|---|---|
| SinAndCosFunctionPlot | ✅ `scenes/sin_and_cos.rhai` | `get_graph_label` (9) would tidy it |
| SineCurveUnitCircle | ✅ `scenes/sine_curve.rhai` | `always_redraw` (5) would tidy it |
| SquareToCircle | ⬜ almost | `fill` colour + opacity in one call (3) |
| MovingAround | ⬜ | `.animate` fill/scale/rotate (4) |
| ManimCELogo | ⬜ | background colour (3), `triangle` (7), groups (1) |
| BraceAnnotation | ⬜ | `brace`, `brace_between_points`, brace labels (8) |
| VectorArrow | ⬜ | `arrow` (6), `number_plane` (9) |
| GradientImageFromArray | ✅ `scenes/gradient_image.rhai` | `surrounding_rectangle` (8) would tidy it |
| BooleanOperations | ⬜ | `ellipse` (7), boolean ops (10), groups (1), move to target (4) |
| PointMovingOnShapes | ⬜ | `move_along_path`, rotate about a point (12) |
| MovingAngle | ⬜ | value trackers, `always_redraw` (5), `angle` (7) |
| MovingDots | ⬜ | value trackers (5), groups (1) |
| MovingGroupToDestination | ✅ `scenes/moving_group.rhai` | |
| MovingFrameBox | ⬜ | math_tex substring selection (11), `surrounding_rectangle` (8) |
| RotationUpdater | ⬜ | rotation over time (16), or write it in `t` |
| PointWithTrace | ⬜ | `traced_path` (16) |
| ArgMinExample | ⬜ | value trackers (5) |
| GraphAreaPlot | ⬜ | `get_area`, `riemann_rectangles`, dashed lines (3, 9) |
| PolygonOnAxes | ⬜ | value trackers, `always_redraw` (5) |
| HeatDiagramPlot | ⬜ | `plot_line_graph`, axis labels (9) |
| FollowingGraphCamera | ⬜ | moving camera (18), `move_along_path` (12) |
| MovingZoomedSceneAround | ⬜ | zoomed scene (21) |
| OpeningManim | ⬜ | `number_plane` (9), nonlinear `apply_function` on a grid |

Each example becomes a `.rhai` scene in `fastanim-script/scenes/` when it is ported, and serves
as that feature's acceptance test, as in SPEC §10.

## P0: Foundations

These items unblock the most examples, and many later items build on them.

1. ✅ **Groups.** A handle for any set of mobjects: shapes, texts and other groups.
   - Every animation and layout function accepts a group: `arrange`, `next_to`, `to_edge`,
     `shift`, `scale`, `rotate`, `fade_in` and the rest.
   - Build on `UpdateGroup` (`fastanim-core/src/anim.rs`) and `Group<K>`
     (`fastanim-core/src/transform_diff.rs`). In scripts, generalise `Mob::Text`.
   - Unblocks: ManimCELogo, BooleanOperations, MovingDots, MovingGroupToDestination.
2. ✅ **Images.** `VState::image` / `image(path or pixels)`: a rectangle with an
   `ImageFill` (shared RGBA pixels, a placement affine that every transform updates, and
   nearest/linear resampling), so images move, fade, layout and group as shapes do.
   - Loaded from PNG/JPEG with the `image` crate, or from a Rhai array or NumPy array.
   - Rendered with Vello `draw_image`, an SVG `<image>` with a PNG data URI, and Canvas
     `drawImage` from a per-image canvas the worker fills once.
   - Still to do: image files in the browser playground (they need fetching before the bake).
     Morphing an image into a shape is out of scope; cross-fade instead.
   - Unblocks: GradientImageFromArray and MovingZoomedSceneAround (with 21).
3. **Style completeness.**
   - Set the background colour from a scene or script. `to_svg` already takes one.
   - Separate fill and stroke opacity: `fill(color, opacity)`.
   - Gradients: `set_color_by_gradient` across a group, and linear gradient fill/stroke on a
     shape. All three renderers support linear gradients.
   - Dashed lines: `dashed_line`, `dashed(shape)`, implemented with `VPath::trim` so `create`
     still works.
   - Line cap and line join options.
4. **Animate to a target state.** manim's `.animate`: an animation that moves a mobject to any
   new state, interpolating geometry *and* style together. This is `transform` with style
   interpolation, plus helpers in scripts: `animate(m, |s| s.fill(RED).scale(2.0))`.
   It covers `generate_target`/`MoveToTarget` and `become`.
5. **Value trackers and `always_redraw`.**
   - `tracker(0.0)` is a scalar driven by `play(set_value(tr, 5.0))`.
   - Updaters and redraw closures read the scalar as a pure function of `t`, keeping to
     SPEC §13.5.
   - `always_redraw(|t| shape)` rebuilds a whole shape each frame.
   - Unblocks: MovingAngle, ArgMinExample, PolygonOnAxes, MovingDots.

## P1: Shape and annotation library

6. **Arrows:** `arrow`, `double_arrow`, `vector`, and tip styles. Reuse the axes tips in
   `fastanim-core/src/axes.rs`.
7. **More shapes:** `ellipse`, `triangle`, `regular_polygon`, `star`, `annulus`, `sector`,
   `rounded_rectangle`, `arc_between_points`, `curved_arrow`, `angle`, `right_angle`.
8. **Annotations:** `surrounding_rectangle`, `background_rectangle`, `brace`,
   `brace_between_points`, brace labels, `cross`, `underline`.
9. **Coordinate systems:**
   - `number_line` and `number_plane` (with faded grid lines).
   - On axes: `get_graph_label`, `get_area`, `riemann_rectangles`, `plot_line_graph`,
     dashed `vertical_line`/`horizontal_line`, `input_to_graph_point`.
10. **Boolean operations:** `union`, `intersection`, `difference`, `exclusion` on paths, using
    a clipping crate that works with kurbo paths.
11. **Text polish:**
    - Font size, weight and slant. Typst already supports these. Family is done: the `font`
      option, with `load_font` for font files (not yet in the browser).
    - Colour substrings (manim's `t2c`).
    - Select parts of a `math_tex` by substring or index, to colour, box or animate them.
    - `decimal_number`/`integer` that follow a tracker, re-typeset through the memo cache.

## P2: Animation polish

12. **Motion:** `move_along_path`, rotation about any point, and `about_point`/`about_edge` on
    `scale` and `rotate`.
13. **Transforms:**
    - `fade_transform`.
    - `transform_matching_shapes` and `transform_matching_tex`. These are a natural fit for the
      diff engine.
    - Clockwise and counter-clockwise transforms, `cyclic_replace`, `swap`.
14. **Composition:** `succession`, plus a `lag_ratio` option on `parallel`.
15. **Creation variants:** `grow_from_point`, `grow_from_edge`, `grow_arrow`, `fade_in` with a
    shift or scale, letter-by-letter text, `show_passing_flash`, `apply_wave`, `focus_on`,
    `blink`.
16. **Traced paths and rotation over time.** `traced_path(m)` and `rotating(m, rate)`, computed
    from `t` (sampled while baking) so seeking still works. Document why there is no `dt`.
17. **Rate functions:** the rest of manim's set: `rush_into`, `rush_from`,
    `there_and_back_with_pause`, `double_smooth`, `lingering`, `running_start`,
    `exponential_decay`.

## P3: Scene and camera

18. **Moving camera.** The camera frame (centre, width, rotation) becomes animated timeline
    state that all three renderers read. Unblocks FollowingGraphCamera.
19. **SVG import:** `svg_mobject(path)` via `usvg`, giving shapes that diff and morph.
    Lottie import is done (`fastanim-core/src/lottie.rs`): each fill and stroke becomes a
    shape evaluated per frame. Still to do there: masks, mattes, repeaters and dashes, which
    wait on the style work in 3, and Lottie files in the browser playground.
20. **Tables, matrices and bar charts.**
21. **Zoomed scene.** It needs a second render of the scene inside the frame, so it comes last.
22. ~~**Sound:** `add_sound` muxed into the export.~~ Done: `audio`, `sound`, `add_sound` and
    `wait_for_sounds`, played in the preview and mixed into MP4 and WebM. Not in the browser
    player yet.

## Every item needs

- A Rust API in `fastanim-core` or `fastanim-text`, and Rhai bindings in
  `fastanim-script/src/lib.rs`.
- The Python mirror in `fastanim-py`, and a regenerated `docs/book/src/reference.md`
  (`fastanim script-api`).
- Rendering in Vello, SVG and Canvas 2D, checked against each other with
  `fastanim-bevy/examples/parity.rs` or a still export.
- A book page or section, and a playground scene where it makes sense.
- Tests in `fastanim-core/tests`, including SVG output tests for new shapes and styles.
