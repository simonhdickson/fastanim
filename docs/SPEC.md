# ranim — Design Specification

> A programmatic mathematical-animation engine in Rust, built on Bevy, in the spirit of
> [manim](https://github.com/ManimCommunity/manim). Its defining feature is that
> **transformations between scenes, equations, code and shapes are planned with Myers' diff**,
> so things that stayed the same visibly *stay*, things that moved *move*, and only what
> actually changed is animated in or out.

Status: **Draft v0.1** · Owner: @simonhdickson

---

## 1. Goals and non-goals

### 1.1 Goals

1. **Code-first animation.** Scenes are ordinary Rust programs: `scene.play(...)`, `scene.wait(...)`,
   like manim's `construct()`.
2. **Diff-driven transforms.** A first-class `TransformDiff` that turns *A → B* (equations, text,
   code, groups of shapes, lists) into a minimal, readable edit script using Myers' algorithm, then
   choreographs that script as an animation.
3. **Deterministic, scrubbable timeline.** Every frame is a pure function of time `t`. You can seek
   to any frame instantly, render out of order, and get bit-identical output on re-runs.
4. **Bevy for runtime and rendering.** Live preview window with a timeline scrubber, plus headless
   export to video/PNG sequence/SVG.
5. **Rust-native text and math.** Typst (pure Rust) for math and rich text, so there is no LaTeX
   install required.

### 1.2 Non-goals (v1)

- 3D scenes (camera is 2D orthographic; the architecture should not *preclude* 3D later).
- Being API-compatible with manim.
- Interactive/game-style runtime behaviour (physics, user input in scenes).
- LaTeX as the primary math backend (may be added as an optional plugin, see §12).

---

## 2. Why Myers' diff?

Manim's matching transforms (`TransformMatchingTex`, `TransformMatchingShapes`) pair parts by key
lookup. That works for simple cases but breaks down with repeated symbols and reorderings:

```
a + b = c        →        a + b + d = c
```

A key-lookup approach sees two `+` on the right and only one on the left and has to guess which
`+` is "the same" one. Myers' diff computes the **shortest edit script** (equivalently, the longest
common subsequence) between two *ordered* sequences, giving:

```
=a  =+  =b  +[+]  +[d]  ==  =c
```

Properties we rely on:

| Property | Consequence for animation |
|---|---|
| Minimal edit script (fewest inserts + deletes) | The least amount of visual churn |
| Order-preserving matches | Equal tokens never cross over each other; the motion reads as "sliding apart / together" |
| Deterministic tie-breaking | Identical inputs always give identical animations |
| O((N+M)·D) time, D = edit distance | Very fast for the typical case of *small edits on larger inputs* |
| Linear-space variant exists | Safe for large inputs (long code files, big lists) |

Myers alone doesn't detect **moves** (`a + b → b + a` comes out as delete+insert), so the spec adds
a move-detection pass on top (§5.4).

---

## 3. Architecture overview

```
┌───────────────────────────────────────────────────────────────────────────┐
│ user scene crate:  fn construct(s: &mut Scene) { ... }                    │
└──────────────┬────────────────────────────────────────────────────────────┘
               │ records
┌──────────────▼──────────────┐     ┌───────────────────────────────┐
│ ranim-core                  │◄────┤ ranim-diff                    │
│  geometry (VPath, Bezier)   │     │  Myers (greedy + linear space)│
│  Mobject state + Interpolate│     │  move detection, cleanup      │
│  Animation trait, rate fns  │     │  hierarchical diff            │
│  Scene builder → Timeline   │     └───────────────────────────────┘
│  Timeline baking + eval(t)  │     ┌───────────────────────────────┐
└──────────────┬──────────────┘◄────┤ ranim-text                    │
               │ BakedTimeline      │  Typst layout → glyph paths   │
┌──────────────▼──────────────┐     │  token spans, code highlighter│
│ ranim-bevy (RanimPlugin)    │     └───────────────────────────────┘
│  ECS sync, SceneClock       │
│  vector rendering (Vello)   │
│  preview UI / scrubber      │
│  headless capture → ffmpeg  │
└──────────────┬──────────────┘
┌──────────────▼──────────────┐
│ ranim-cli: render / preview │
└─────────────────────────────┘
```

**Key decision:** scene construction and timeline evaluation live in `ranim-core` with **no Bevy
dependency**. Bevy is the runtime/renderer, not the source of truth. This keeps the core
testable with plain `cargo test`, keeps Bevy-version churn contained in one crate, and makes the
timeline seekable.

### 3.1 Workspace layout

```
ranim/
├── Cargo.toml                # workspace
├── crates/
│   ├── ranim-diff/           # zero-dependency generic diff library
│   ├── ranim-core/           # geometry, mobjects, animations, timeline
│   ├── ranim-text/           # typst integration, fonts, code tokenizing
│   ├── ranim-bevy/           # Bevy plugin: sync, render, preview, export
│   ├── ranim-cli/            # `ranim` binary
│   ├── ranim-script/         # Rhai bindings over core + text (§14)
│   ├── ranim-web/            # wasm-bindgen browser player and playground (§14)
│   └── ranim/                # facade crate re-exporting a prelude
└── examples/
```

### 3.2 Key dependencies

| Concern | Choice | Notes |
|---|---|---|
| Engine | `bevy` (latest stable at project start, pinned) | Only `ranim-bevy` depends on it |
| Vector rendering | `vello` via `bevy_vello` | GPU path rendering, AA strokes/fills, gradients. Behind a `VectorBackend` trait so a `lyon` tessellation backend can be swapped in |
| Geometry | `kurbo` | Béziers, arc length, affine, subdivision |
| Math & rich text | `typst` (as library) | Layout gives glyph positions *and* source spans |
| Fonts / glyph outlines | `ttf-parser` / `skrifa` | Glyph → `kurbo::BezPath` |
| SVG import | `usvg` | `SvgMobject` |
| Code highlighting | `syntect` or `tree-sitter-highlight` | Token kinds for coloring & diff keys |
| Colors | `palette` | Perceptual (Oklab) color interpolation |
| Video | `ffmpeg` subprocess | Raw RGBA piped to stdin |
| Scripting | `rhai` | Scenes as scripts, natively and in the browser (§14) |
| Web | `wasm-bindgen`, `web-sys` | Browser player; Canvas 2D rendering (§14) |

---

## 4. Core model (`ranim-core`)

### 4.1 Geometry

```rust
/// A single closed or open contour made of cubic Béziers.
pub struct SubPath {
    pub segments: Vec<CubicBez>,   // kurbo::CubicBez; all curves normalized to cubic
    pub closed: bool,
}

/// A vector shape: possibly many contours (e.g. the glyph "B" has 3).
pub struct VPath {
    pub subpaths: Vec<SubPath>,
}
```

All geometry is normalized to cubic Béziers so interpolation is a uniform control-point lerp.
Lines and quadratics are degree-elevated on import.

### 4.2 Mobject state

Everything drawable is a **mobject** (mathematical object). Its animatable state is a plain value
type so it can be cloned, interpolated, hashed and snapshotted:

```rust
pub struct MobjectId(u32);

pub struct VState {
    pub path: VPath,
    pub fill: Paint,              // solid / linear / radial gradient
    pub stroke: Stroke,           // paint, width, join, cap, dash
    pub transform: Affine,        // local transform
    pub opacity: f32,
    pub z_index: i32,
    pub draw_range: Range<f32>,   // 0..1 of arc length; drives Create/Write
}

pub enum MobjectKind {
    Shape(VState),
    Group(Vec<MobjectId>),        // VGroup: children, ordered
    Text(TextMobject),            // VState per glyph + token metadata (see §6)
}
```

Built-in constructors: `Circle`, `Square`, `Rectangle`, `Polygon`, `Line`, `Arrow`, `Arc`,
`Dot`, `Axes`, `NumberPlane`, `FunctionGraph`, `Text`, `MathTex` (Typst math), `Code`,
`SvgMobject`, `Brace`, `SurroundingRect`.

### 4.3 Interpolation and path alignment

```rust
pub trait Interpolate: Clone {
    fn lerp(a: &Self, b: &Self, t: f32) -> Self;
}
```

To morph between two `VPath`s they must first be **aligned** to the same topology (as in manim's
`align_points`):

1. **Subpath count** — pad the shorter list with degenerate zero-length subpaths placed at the
   centroid of the subpath they'll grow into.
2. **Subpath correspondence** — pair subpaths by *diff* (§5.6), not by index, so that e.g. the
   inner hole of "o" maps to the inner hole of "p".
3. **Segment count** — within paired subpaths, subdivide the longest segments of the shorter one
   until counts match (keeps curvature distribution even).
4. **Start point** — for closed subpaths, rotate the segment list to minimize total control-point
   travel (avoids "twisting" morphs).

Colors interpolate in Oklab. Transforms decompose to translate/rotate/scale and interpolate
componentwise (avoids shear artifacts mid-rotation).

### 4.4 Animations

```rust
pub trait Animation: Send + Sync {
    /// Called once at bake time with the state of all targets at the start of the clip.
    /// Returns the targets' end state and any mobjects created/removed.
    fn plan(&mut self, world: &SceneState) -> AnimPlan;

    /// Pure function: state of the targets at local progress `alpha` (already eased).
    fn sample(&self, alpha: f32, out: &mut SceneState);

    fn run_time(&self) -> f32 { 1.0 }
    fn rate_fn(&self) -> RateFn { RateFn::Smooth }
}
```

`plan` does the expensive work (path alignment, diffing) **once**; `sample` must be cheap and pure,
which is what makes scrubbing and parallel frame rendering possible.

Standard library (v1):

| Category | Animations |
|---|---|
| Creation | `Create`, `Write`, `DrawBorderThenFill`, `FadeIn`, `GrowFromCenter`, `SpinIn` |
| Removal | `Uncreate`, `Unwrite`, `FadeOut`, `ShrinkToCenter` |
| Transform | `Transform`, `ReplacementTransform`, **`TransformDiff`**, `MoveTo`, `Rotate`, `Scale`, `ApplyFunction` |
| Emphasis | `Indicate`, `Circumscribe`, `Flash`, `Wiggle` |
| Composition | `Parallel` (AnimationGroup), `Sequence` (Succession), `LaggedStart { lag_ratio }` |
| Updaters | `always(|m, t| ...)` — functions of time, also pure in `t` |

Rate functions: `Linear`, `Smooth` (manim default), `EaseIn/Out/InOut{Quad,Cubic,Expo,Back}`,
`ThereAndBack`, `Spring { stiffness, damping }`, `Custom(fn(f32) -> f32)`.

### 4.5 Scene builder and timeline

Authoring is imperative, like manim, but it **records** rather than renders:

```rust
pub struct Scene { /* cursor time, SceneState, Vec<Clip>, id allocator */ }

impl Scene {
    pub fn add(&mut self, m: impl Into<Mobject>) -> MobjectId;
    pub fn remove(&mut self, id: MobjectId);
    pub fn play(&mut self, anim: impl IntoAnimation) -> PlayHandle; // .run_time(), .rate()
    pub fn wait(&mut self, secs: f32);
    pub fn marker(&mut self, name: &str);        // named seek points for preview & sections
    pub fn camera(&mut self) -> CameraHandle;    // camera is an animatable mobject too
}
```

`play` immediately calls `plan()` against the current `SceneState`, appends a `Clip` and advances
the cursor. The result of building is a **`BakedTimeline`**:

```rust
pub struct Clip { start: f32, end: f32, anim: Box<dyn Animation>, rate: RateFn }

pub struct BakedTimeline {
    clips: Vec<Clip>,                 // sorted by start
    keyframes: Vec<(f32, SceneState)>,// snapshot at every clip boundary
    duration: f32,
    markers: Vec<(String, f32)>,
}

impl BakedTimeline {
    pub fn eval(&self, t: f32) -> SceneState; // nearest keyframe ≤ t, then sample active clips
}
```

Because keyframes are snapshotted at boundaries, `eval(t)` costs only the clips active at `t`,
independent of how long the video is.

---

## 5. The diff engine (`ranim-diff`)

A small, dependency-free, generic crate. It knows nothing about animation.

### 5.1 API

```rust
pub enum Op {
    Equal  { a: usize, b: usize },
    Delete { a: usize },
    Insert { b: usize },
    // produced by post-processing passes:
    Move    { a: usize, b: usize },
    Replace { a: Range<usize>, b: Range<usize> },
}

pub struct DiffOptions {
    pub algorithm: Algorithm,       // Myers (default) | MyersLinearSpace | Patience
    pub detect_moves: bool,         // default: true
    pub cleanup: Cleanup,           // None | Semantic { min_equal_run: usize }
    pub pair_replacements: bool,    // default: true
}

pub fn diff<T, K: Eq + Hash>(
    a: &[T], b: &[T],
    key: impl Fn(&T) -> K,
    opts: &DiffOptions,
) -> Vec<Op>;
```

Items are compared by a **key**, not by `PartialEq` on the item, so callers decide what "same" means
(e.g. glyph text but not position; code token text + kind but not color).

### 5.2 Myers greedy algorithm

Standard forward Myers on the edit graph. `V[k]` holds the furthest-reaching `x` on diagonal
`k = x − y`:

```
for D in 0..=N+M:
    for k in (-D..=D).step_by(2):
        if k == -D || (k != D && V[k-1] < V[k+1]):
            x = V[k+1]            # move down: insertion
        else:
            x = V[k-1] + 1        # move right: deletion
        y = x - k
        while x < N && y < M && key(A[x]) == key(B[y]):
            x += 1; y += 1        # follow the snake (free diagonal = Equal)
        V[k] = x
        if x >= N && y >= M:
            return backtrack(trace, D)
    trace.push(V.clone())
```

- Time O((N+M)·D), memory O(D²) for the trace. Used when `N + M ≤ 10_000`.
- Tie-breaking is fixed (deletions before insertions within a hunk) for determinism.
- When several edit scripts share the minimal cost, the default `TieBreak::Stable` picks the one
  whose unchanged blocks shift least (then fewest blocks, then least per-item displacement), via
  an O(N·M) pass for inputs up to ~1M cells. This is what makes `a + b = c → b + a = c` keep
  `+ = c` still. `TieBreak::Myers` keeps the raw search order.
- Keys are pre-hashed to `u64` and common prefix/suffix are trimmed before running — the dominant
  case for equation edits is "small change in the middle".

### 5.3 Linear-space variant

For large inputs (long code listings, big data lists) use Myers' **middle-snake** divide-and-conquer
(run forward and reverse searches simultaneously, find the overlapping snake, recurse on both
halves). O((N+M)·D) time, O(N+M) memory. Selected automatically above the threshold.

### 5.4 Move detection (post-pass)

Myers only produces Equal/Delete/Insert. For animation, a token that was deleted in one place and
inserted elsewhere should **travel**, not fade out and back in.

1. Collect deleted indices `Da` and inserted indices `Ib`, bucketed by key.
2. For each key present in both buckets, pair items by minimal total *visual* distance — the
   caller supplies `cost(a_idx, b_idx)`, typically centroid distance. For small buckets use the
   Hungarian algorithm; for large buckets, greedy nearest-first.
3. Replace each paired Delete/Insert with `Move { a, b }`.

Example: `a + b = c → b + a = c` becomes `Move(a) =+ Move(b) == =c` — `a` and `b` swap along arcs
while `+ = c` stay put.

### 5.5 Replacement pairing and cleanup

- **Replace pairing:** an adjacent run of Deletes immediately followed by Inserts (a "hunk") becomes
  `Replace { a: range, b: range }`. The animator morphs these instead of fade-out/fade-in, e.g.
  `x²` → `x³` morphs the `2` into a `3` in place.
- **Cross-hunk pairing** (after move detection): leftover Deletes and Inserts that are *not*
  adjacent but share a caller-defined `class` (e.g. both are binary operators) may be paired into
  a `Replace` that morphs while travelling. Pairing uses the same `cost` function as §5.4 and is
  limited to one partner per item.
- **Semantic cleanup** (as in diff-match-patch): Equal runs shorter than `min_equal_run` sandwiched
  between edits are folded into the surrounding hunk. This prevents distracting "one letter stays
  still while everything around it changes" artifacts in prose and code.

### 5.6 Hierarchical diff

Many inputs are naturally nested. Diff coarse-to-fine:

| Domain | Level 1 key | Level 2 key | Level 3 key |
|---|---|---|---|
| Code | line (trimmed text) | token (kind + text) | char |
| Prose | word | char | — |
| Math (Typst) | top-level math node | leaf token | glyph |
| VGroup | child structural hash | subpath shape signature | — |

Run Myers at level 1; for each `Replace` hunk, recurse at level 2 on just that hunk; and so on.
This is the same idea as `git diff --word-diff` and keeps each Myers run small.

**Shape signature** (for subpath matching in §4.3 and group diffs): a quantized descriptor — number
of segments, closedness, normalized area sign, and a coarse 8-bucket turning-angle histogram —
hashed to `u64`. It's insensitive to position and scale so the same shape at a new place is
recognized as Equal/Move.

### 5.7 Correctness requirements

- `apply(a, ops) == b` for every op script (property test with `proptest`).
- Myers result length equals the LCS DP baseline for random inputs up to N, M ≤ 200.
- Linear-space and greedy variants produce scripts of equal cost.
- Determinism: same input ⇒ byte-identical script across runs and platforms.

---

## 6. Text, math and code (`ranim-text`)

### 6.1 Tokens

Every text-like mobject is laid out into glyphs, and glyphs are grouped into **tokens** that carry
a diff key:

```rust
pub struct Token {
    pub key: TokenKey,            // what the diff compares
    pub glyphs: Range<usize>,     // indices into TextMobject.glyphs
    pub span: Range<usize>,       // byte range in the source string
    pub kind: TokenKind,          // Ident, Number, Operator, Keyword, MathSymbol, Space, ...
}

pub struct TextMobject {
    pub source: String,
    pub glyphs: Vec<VState>,
    pub tokens: Vec<Token>,
    pub lines: Vec<Range<usize>>, // token ranges per line, for hierarchical diff
}
```

### 6.2 Math via Typst

`MathTex::new("a^2 + b^2 = c^2")` compiles a minimal Typst document (`$ ... $`) in memory with
embedded fonts. Typst's layout frames give each glyph's outline and a **source span**, which is how
glyphs get tied to tokens. Non-glyph marks (fraction bars, radicals, delimiters stretched by
layout) become tokens with synthetic keys like `frac-bar`, `sqrt-sign`.

Keys include structural context where it matters: a `2` used as a superscript gets the key
`("2", Sup)`, distinct from a baseline `2`, so `x^2 → x 2` animates as a shape *move/rescale*
rather than an Equal that silently changes size.

Users can override keys explicitly for tricky cases (the manim `{{ }}` isolation idea):

```rust
MathTex::new("{{a}}^2 + {{b}}^2 = {{c}}^2")  // braces define explicit tokens
```

### 6.3 Code

`Code::new(src).language("rust").theme("...")` highlights with `syntect`/`tree-sitter`, producing
tokens whose key is `(kind, text)`. Whitespace tokens are excluded from the diff sequence (layout
moves them implicitly). Diff runs line → token → char (§5.6).

---

## 7. `TransformDiff`: from edit script to animation

### 7.1 Usage

```rust
let eq = s.add(MathTex::new("a^2 + b^2 = c^2"));
s.play(Write::new(eq));
s.play(TransformDiff::new(eq, MathTex::new("a^2 = c^2 - b^2")));

let code = s.add(Code::new(BEFORE).language("rust"));
s.play(TransformDiff::new(code, Code::new(AFTER).language("rust")).run_time(2.0));
```

`TransformDiff` works on anything implementing:

```rust
pub trait Diffable {
    type Item;
    type Key: Eq + Hash;
    fn items(&self) -> &[Self::Item];        // the sequence to diff (tokens, children...)
    fn key(item: &Self::Item) -> Self::Key;
    fn levels(&self) -> &[DiffLevel] { &[] } // optional hierarchy (§5.6)
    fn state(&self, item: &Self::Item) -> VState;
}
```

Implemented for `TextMobject` (MathTex/Text/Code), `VGroup`, and `ListMobject` (array/stack
visualizations for algorithm animations).

### 7.2 Op → animation mapping

| Op | Default animation | Notes |
|---|---|---|
| `Equal` | `Transform` from old to new position/scale | Often a pure translation — the "sliding" look |
| `Move` | `Transform` along an arc (`path_arc = ±π/3`) | Sign alternates for pairs that swap, so they pass on opposite sides |
| `Delete` | `FadeOut` + shrink toward its nearest Equal neighbour | Looks like it's being "absorbed" |
| `Insert` | `FadeIn` + grow from its nearest Equal neighbour | |
| `Replace` | Aligned path morph (§4.3) | Falls back to cross-fade when shapes are very dissimilar (signature distance > threshold) |

### 7.3 Choreography

```rust
pub struct DiffStyle {
    pub phasing: Phasing,          // Sequential | Overlapped (default)
    pub lag_ratio: f32,            // stagger within a phase, default 0.05
    pub delete: AnimFactory,       // overridable per op kind
    pub insert: AnimFactory,
    pub mov: MoveStyle,            // Straight | Arc(f32)
    pub replace: ReplaceStyle,     // Morph | CrossFade
    pub highlight_changes: bool,   // briefly tint inserted/replaced tokens
}
```

Default **Overlapped** phasing, as fractions of `run_time`:

```
Delete   ███████░░░░░░░░░░░░░   0.00 – 0.35
Equal    ░░░░███████████░░░░░   0.20 – 0.80
Move     ░░░░███████████░░░░░   0.20 – 0.80
Replace  ░░░░░░███████████░░░   0.30 – 0.85
Insert   ░░░░░░░░░░░░████████   0.60 – 1.00
```

Rationale: removing things first makes room; moving survivors second keeps the eye anchored on
what's stable; new material arrives last, when the viewer's attention is free.

### 7.4 Diff debug view

`TransformDiff::debug()` (or the `D` key in preview) overlays the edit script: green = insert,
red = delete, blue arrows = move, amber = replace, grey = equal. Essential for tuning keys.

---

## 8. Bevy integration (`ranim-bevy`)

### 8.1 Plugin

```rust
App::new()
    .add_plugins(DefaultPlugins)
    .add_plugins(RanimPlugin::new(my_scene).mode(Mode::Preview))
    .run();
```

### 8.2 ECS mapping

| ECS | Purpose |
|---|---|
| `Resource<BakedTimeline>` | Output of the scene builder |
| `Resource<SceneClock { t, playing, speed }>` | The only source of time. **Not** Bevy's `Time` |
| `Component<MobjectRef(MobjectId)>` | One entity per mobject (glyphs are children of their text entity) |
| `Component<VState>` | Current evaluated state |
| `Component<RenderOrder(z, insertion)>` | Stable draw ordering |

Systems (in order, in `Update`):

1. `advance_clock` — preview: `t += dt * speed` if playing; export: `t = frame / fps`.
2. `evaluate_timeline` — `timeline.eval(t)`; spawns/despawns entities for mobjects that enter/leave
   the scene; writes `VState` components (only for changed mobjects).
3. `sync_camera` — applies camera mobject state to the 2D orthographic camera.
4. `build_vector_scene` — encodes all `VState`s (sorted by `RenderOrder`) into a Vello scene,
   applying `draw_range` by trimming paths by arc length.

All the interesting logic is in `ranim-core`; Bevy systems are thin adapters.

### 8.3 Coordinate system

Manim-like units: frame is 14.22 × 8 units, origin centered, y up. Constants `UP`, `DOWN`, `LEFT`,
`RIGHT`, `ORIGIN`. Positioning helpers: `next_to`, `align_to`, `arrange(direction, buff)`,
`to_edge`, `shift`.

### 8.4 Preview mode

- Window with the scene and a bottom timeline bar showing clips and markers.
- Space = play/pause, ←/→ = step frame, `[`/`]` = jump to previous/next marker, drag = scrub.
- Hot reload (stretch): `ranim preview` watches the scene crate, rebuilds it as a `cdylib`,
  reloads, re-bakes and keeps the current `t`.

### 8.5 Export mode

- Headless Bevy (no window), render to an offscreen texture at the target resolution.
- GPU → CPU readback per frame; raw RGBA piped to `ffmpeg -f rawvideo -pix_fmt rgba ...`.
- Fixed `fps`; time comes from frame index only ⇒ deterministic.
- Outputs: MP4 (H.264), WebM (VP9, alpha), GIF, PNG sequence, and SVG per frame (rendered directly
  from `SceneState`, bypassing the GPU).
- `--section <marker>` renders only between markers; `--frame <n>` renders a still.

---

## 9. CLI (`ranim-cli`)

```
ranim new <name>                       # scaffold a scene crate
ranim preview [--scene Name]           # windowed preview with scrubber
ranim render  [--scene Name] [-q 1080p60] [-o out.mp4] [--format mp4|webm|gif|png|svg]
ranim still   [--scene Name] --at 3.5s -o frame.png
ranim diff    "a^2+b^2=c^2" "a^2=c^2-b^2" --math   # print the edit script (debugging keys)
ranim run     scene.rhai [preview|render|still] [...]   # a Rhai scene, no crate needed (§14)
```

Quality presets: `480p15`, `720p30`, `1080p60` (default), `4k60`.

---

## 10. Example scenes (acceptance targets)

These double as integration tests (§11) and should all render correctly before v1.

1. **Pythagoras rearrangement**
   `a^2 + b^2 = c^2 → a^2 = c^2 - b^2 → a = sqrt(c^2 - b^2)`
   Expect: `a^2` and `c^2` slide; `+ b^2` travels across `=` (Move) and its `+` morphs to `-`
   (cross-hunk Replace, §5.5); `sqrt` and its radical grow in around `c^2 - b^2`.
2. **Commutativity** `a + b = b + a → b + a = a + b`: two pairs of arcing moves, `+` and `=` fixed.
3. **Code refactor**: extracting a function in a 20-line Rust snippet. Unchanged lines slide to new
   positions; the new `fn` signature writes in; edited lines diff at token level.
4. **Bubble sort**: `ListMobject` of boxed numbers; each swap is a `TransformDiff` between list
   states and is detected as two Moves.
5. **Shape morph**: square → circle → the glyph "8" (two holes), verifying subpath alignment.
6. **Classic manim parity**: `Create(Circle)`, `Transform(square, circle)`, `Write(Text)`,
   graph of `sin(x)` on `Axes` with a moving `Dot` driven by an updater.

---

## 11. Testing strategy

| Layer | Tests |
|---|---|
| `ranim-diff` | Property tests (`apply(a, ops) == b`, cost == LCS baseline, variant equivalence), fuzzing, benchmarks with `criterion` |
| `ranim-core` | Interpolation endpoints (`lerp(a,b,0)==a`, `lerp(a,b,1)==b`), path alignment invariants, timeline `eval` at clip boundaries, `eval(t)` independent of evaluation order |
| `ranim-text` | Snapshot tests of token sequences for a corpus of Typst expressions and code snippets |
| `TransformDiff` | Snapshot tests of the *op script* (`insta`), which is stable and reviewable, rather than pixels |
| Rendering | Golden-frame tests: SVG output compared textually; PNG output compared with a perceptual tolerance in CI on a software GPU (lavapipe/llvmpipe) |

---

## 12. Milestones

| # | Milestone | Deliverable |
|---|---|---|
| M0 | Workspace skeleton | Crates, CI, `cargo test` green |
| M1 | `ranim-diff` | Myers greedy + linear space, moves, replace pairing, cleanup, property tests, `ranim diff` CLI for plain strings |
| M2 | Core geometry & timeline | `VPath`, shapes, interpolation/alignment, `Animation` trait, `Scene` builder, `BakedTimeline::eval`, SVG still export (no Bevy yet) |
| M3 | Bevy preview | `RanimPlugin`, Vello rendering, clock, scrubber; example 6 |
| M4 | Video export | Headless capture + ffmpeg, deterministic frames, quality presets |
| M5 | Text & math | Typst integration, tokens, `Write`, `MathTex`, `Text` |
| M6 | `TransformDiff` | Op→animation mapping, choreography, debug overlay; examples 1, 2, 5 |
| M7 | Code & lists | `Code`, hierarchical diff, `ListMobject`; examples 3, 4 |
| M8 | Polish | Hot reload, more animations, docs site with rendered examples |
| M9 | Scripting & web | `ranim-script` (Rhai), `ranim run`, `ranim-web` browser player and playground; §10 examples as `.rhai` scripts (§14) |

---

## 13. Open questions

1. **Vello vs. tessellation.** Vello gives the best quality but depends on compute shaders; is a
   `lyon` mesh fallback needed for WebGL2 / low-end targets in v1, or later?
2. **Patience diff as an option?** Patience/histogram diff often reads better for *code* (anchors
   on unique lines). Proposal: Myers default everywhere, Patience as an opt-in for `Code`. Decide
   after building example 3 and comparing.
3. **Cross-type diffs.** Should `TransformDiff(Text, MathTex)` be allowed (keys normalized to
   plain characters), or require same type?
4. **Key granularity for math.** Is `(text, script-level)` enough context, or do we need the full
   Typst syntax path (e.g. "inside fraction numerator")? Too much context reduces matches; too
   little creates wrong matches.
5. **Updaters vs. purity.** manim updaters can read other mobjects' live state. Keeping `eval(t)`
   pure means updaters must be `fn(t, &SceneState) -> VState` evaluated in dependency order.
   Is that expressive enough?
6. **LaTeX backend.** Optional `ranim-latex` crate shelling out to `latex` + `dvisvgm`, with token
   spans recovered via `\special` markers? Deferred until users ask.

---

## 14. Scripting and the browser (M9)

Scenes are compiled Rust today, which needs a toolchain and cannot run in a browser. M9 adds
**Rhai** scenes: the same API as a script, interpreted, so a scene can be written, edited and
played entirely in a web page, or run natively with `ranim run` and no scene crate. Rhai is
pure Rust, compiles to `wasm32-unknown-unknown`, and its syntax is close enough to Rust that
scenes port almost line for line. It covers what most scenes need; Rust stays available for
anything heavier.

### 14.1 What already runs in the browser

| Crate | In wasm | Notes |
|---|---|---|
| `ranim-diff`, `ranim-core` | As is | No I/O, threads or clocks; `eval(t)` and `to_svg` are pure |
| `ranim-text` | As is | Typst runs in wasm, and the `World` is in-memory. Embedded `typst-assets` fonts add several MB, so the web build fetches them separately, only when needed (§14.5) |
| `ranim-bevy` | Not used | Vello needs WebGPU (§13 Q1); export shells out to `ffmpeg`, writes files and blocks on GPU readback. The web player renders without it |

### 14.2 `ranim-script`: the binding layer

A crate over `ranim-core` and `ranim-text` that exposes the scene API to scripts. It is split in
two so other languages can be added later without redoing the work:

- **A language-neutral API**: plain Rust functions over handles (`MobjectId`, `TextMobject`
  groups, boxed `Animation`s), with options as name/value maps instead of builder generics.
  This is what every frontend binds; it is also where defaults and validation live.
- **The Rhai frontend**: registers that API with a `rhai::Engine`. A JavaScript frontend
  (`wasm-bindgen` classes in `ranim-web`) or another embedded language would bind the same
  layer.

```rhai
// pythagoras.rhai
let title = scene.add(text("Solving for a").to_edge(UP));
scene.play(write(title));

let eq = scene.add(math_tex("a^2 + b^2 = c^2").scale(1.5));
scene.play(write(eq).run_time(1.5));
scene.wait(0.5);
scene.marker("rearrange");

scene.play(transform_diff(eq, math_tex("a^2 = c^2 - b^2").scale(1.5)));
scene.play(circumscribe(eq));
let dot = scene.add(dot(point(1.0, 0.0)));
scene.always(dot, |state, t| state[dot].move_to(point(cos(t), sin(t))));
```

Rules:

- **Bake once.** A script runs top to bottom once, recording into a `Scene`, then is baked as
  usual; playback and seeking never run the script again. Determinism (§1.1) is unchanged.
- **Closures.** `apply_function`, `always` and `Update` take Rhai function pointers. They run at
  sample time, so the baked timeline holds the compiled `AST` (behind an `Arc`, Rhai's `sync`
  feature, to satisfy `Animation: Send + Sync`). They are called every frame, so they must stay
  pure functions of their arguments: no access to script globals that change.
- **Limits.** Scripts run with Rhai's operation, call-depth and string-size limits, so an
  infinite loop in a browser tab fails with an error instead of hanging the page.
- **Errors.** Script and Typst errors carry line and column and are shown inline in the
  playground and as `file:line:col` from `ranim run`.

### 14.3 `ranim run`

`ranim run scene.rhai [preview|render|still] [options]` takes the same commands and options as
a compiled scene (§8.4, §8.5) and renders through `ranim-bevy`. In preview it watches the file
and re-runs and re-bakes it on save, keeping the current `t`: the hot reload of §8.4 without
rebuilding a `cdylib`. `ranim run scene.rhai --bundle` writes the pre-typeset text for the web player
(§14.5).

### 14.4 `ranim-web`: the browser player

A `wasm-bindgen` crate built with `wasm-bindgen-cli` (`build.sh`), without Bevy:

- **Rendering**: each frame calls `eval(t)` and draws to a `<canvas>` with Canvas 2D, building a
  `Path2D` from each `VPath`'s cubics, trimmed by `draw_range` with `VPath::trim`, in
  `z_index` order. This works in every browser; a WebGPU/Vello backend can come later.
- **Clock and scrubber**: `requestAnimationFrame` drives `t`; an HTML timeline bar gives
  play/pause, frame stepping, markers and dragging, as in §8.4.
- **Playground**: an editor next to the canvas runs the script on change (debounced), re-bakes
  and keeps `t`. Baking runs in a Web Worker so long scenes don't freeze the page.
- **Embedding**: `<ranim-player src="scene.rhai">` (or a bundled scene, §14.5) so the docs site's
  rendered examples (M8) are live and scrubbable.
- **Export**: SVG frames (from `to_svg`) and PNG stills (`canvas.toBlob`) as downloads. Video
  through WebCodecs `VideoEncoder` plus an MP4/WebM muxer is a stretch goal, with frames still
  timed by index so output stays deterministic.

### 14.5 Text performance in the browser

Typst parallelizes with `rayon`, which has no threads on `wasm32-unknown-unknown` and runs on
the calling thread instead. That costs little here: each `math_tex`, `text` or `code` call
typesets one small snippet onto one page, which has little internal parallelism to lose. What
does cost is blocking the page while a scene bakes, the one-off font parsing and `Library`
setup (`shared()` in `ranim-text`), and re-typesetting snippets that haven't changed. In order:

1. **Bake off the main thread (required).** Script runs and baking happen in a Web Worker
   (§14.4); the page only receives the baked timeline. This removes the freeze, not the cost.
2. **Cache typeset text (required).** Typesetting is a pure function of `(kind, source,
   language)`, so `ranim-text` memoizes `TextMobject`s on that key. In the playground, an edit
   then only re-typesets the snippets it changed. The cache can be persisted in IndexedDB
   across visits.
3. **Pre-typeset published scenes (required for the docs site).** `ranim run --bundle` typesets
   every snippet natively and writes the glyph outlines next to the script; the player fills
   the cache from that bundle, so published scenes never run Typst in the browser and skip the
   font download. The fonts (the `typst-assets` set by default) are fetched as a separate,
   cacheable asset only when a script typesets something not in the cache.
4. **Typeset in a worker pool (if long scripts still bake slowly).** Snippets are independent,
   so they can be spread across workers, each its own wasm instance with its own fonts:
   - **Collect:** run the script once, recording every text call as a request.
   - **Fan out:** typeset the requests across the pool, filling the cache.
   - **Bake:** run the script again, answered from the cache.

   This works in every browser with no special headers. It costs memory per worker and a second
   script run; making `TextMobject` a lazy handle resolved before baking would avoid the second
   run, but positioning that needs a bounding box would then have to wait for it.
5. **Not planned: `wasm-bindgen-rayon`.** Shared-memory threads would give Typst its thread
   pool back, but they need a nightly toolchain with `-Z build-std` and atomics, and the page
   must be cross-origin isolated (`COOP: same-origin`, `COEP: require-corp`). That conflicts
   with embedding `<ranim-player>` in other sites, and the gain on small snippets is modest.
   It could be offered later as an opt-in for the standalone playground.

None of this is measured yet; the playground should report time spent typesetting vs. baking
so the order above can be checked against real scenes.

### 14.6 Acceptance

- The §10 examples exist as `.rhai` scripts, and each bakes to the same timeline as its Rust
  version (compared through `to_svg` at sampled times, §11).
- They play in the browser player in current Chrome, Firefox and Safari, and render with
  `ranim run`.
- A script with an infinite loop or a Typst error reports an error and leaves the page usable.
- Editing one equation in a long script re-typesets only that equation (§14.5), and a bundled
  docs-site scene plays without downloading fonts.

### 14.7 Open questions

1. **Naming in scripts.** Mirror the Rust names exactly (`replacement_transform`) or adopt
   shorter script names? Mirroring keeps docs shared across languages.
2. **Updater performance.** Rhai closures evaluated per frame are slower than Rust. Is that
   fine for typical updaters, or do common ones (follow, rotate around) need native helpers?
3. **Custom fonts.** Should scripts be able to load their own fonts on the web, and from where
   (URL, upload, a font picker)? The default set is covered by §14.5.

---

## Appendix A — End-to-end example

```rust
use ranim::prelude::*;

fn pythagoras(s: &mut Scene) {
    let title = s.add(Text::new("Solving for a").to_edge(UP));
    s.play(Write::new(title));

    let eq = s.add(MathTex::new("a^2 + b^2 = c^2").scale(1.5));
    s.play(Write::new(eq).run_time(1.5));
    s.wait(0.5);
    s.marker("rearrange");

    s.play(TransformDiff::new(eq, MathTex::new("a^2 = c^2 - b^2").scale(1.5)));
    s.wait(0.5);

    s.play(
        TransformDiff::new(eq, MathTex::new("a = sqrt(c^2 - b^2)").scale(1.5))
            .style(DiffStyle { highlight_changes: true, ..default() })
            .run_time(1.5),
    );
    s.play(Circumscribe::new(eq));
    s.wait(1.0);
}

fn main() {
    ranim::run(pythagoras); // CLI args select preview vs render
}
```

## Appendix B — Edit script for the first transform

`a^2 + b^2 = c^2  →  a^2 = c^2 - b^2`, tokens keyed by `(text, script-level)`:

```
A: a  ²  +  b  ²  =  c  ²
B: a  ²  =  c  ²  -  b  ²

Myers:       =a =² -[+] -[b] -[²] == =c =² +[-] +[b] +[²]
Moves:       b,² (A[3..5]) → (B[6..8])            → Move, Move
Replace:     -[+] … +[-]  (not adjacent, same class "operator")
                                                  → cross-hunk Replace(+ → -)
Final:       =a =² ~(+→-) ↷b ↷² == =c =²
```

Animated: `a²`, `=`, `c²` slide left as a block; `b²` arcs over to the right end; the `+` morphs
into `-` while travelling to its new slot.
