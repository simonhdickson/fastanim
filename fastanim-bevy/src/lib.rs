//! Bevy integration for fastanim: `FastanimPlugin`, the scene clock, Vello rendering and the preview
//! scrubber, and headless video export (see `docs/SPEC.md` §8).
//!
//! All the logic lives in `fastanim-core`; the systems here only advance a clock, call
//! [`BakedTimeline::eval`] and encode the result.
//!
//! ```no_run
//! use fastanim_core::{Scene, VState, create};
//!
//! let mut s = Scene::new();
//! let c = s.add(VState::circle(1.0));
//! s.play(create(c));
//! fastanim_bevy::preview(s.bake());
//! ```

#[cfg(feature = "audio")]
mod audio;
pub mod export;
pub mod shader;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};

use bevy::camera::ScalingMode;
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::sprite::Anchor;
use bevy_vello::VelloPlugin;
use bevy_vello::prelude::*;
use bevy_vello::render::VelloRenderer;
use clap::{Parser, Subcommand};
use fastanim_core::{
    BakedTimeline, EventKind, FRAME_HEIGHT, FRAME_WIDTH, ImageFill, LineCap, LineJoin, Scene,
    SceneEvent, SceneState, VState,
};
use shader::{ShaderFills, ShaderJob};

use bevy_vello::vello;
use bevy_vello::vello::kurbo::{self, Affine, BezPath, Cap, Join, Rect};
use bevy_vello::vello::peniko::{
    self, Blob, Brush, Color, ColorStop, Fill, ImageAlphaType, ImageBrush, ImageData, ImageFormat,
    ImageQuality,
};

/// Preview or export a fastanim scene.
#[derive(Debug, Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

/// What to do with a baked scene: the subcommands of [`run`].
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Open a window with a scrubber (default)
    Preview,
    /// Export video or frames
    Render(RenderArgs),
    /// Export one frame
    Still(StillArgs),
}

/// Options for [`Command::Render`].
#[derive(Debug, clap::Args)]
pub struct RenderArgs {
    /// 480p15, 720p30, 1080p60 or 4k60
    #[arg(short, long, value_name = "PRESET", default_value = "1080p60")]
    pub quality: export::Quality,
    /// Output; its extension picks the format: mp4, webm, gif, png or svg
    #[arg(short, long, value_name = "PATH", default_value = "out.mp4")]
    pub output: PathBuf,
    /// Only render from <MARKER> to the next marker
    #[arg(long, value_name = "MARKER")]
    pub section: Option<String>,
    /// Render on the CPU even when a GPU is available
    #[arg(long)]
    pub cpu: bool,
}

/// Options for [`Command::Still`].
#[derive(Debug, clap::Args)]
pub struct StillArgs {
    /// 480p15, 720p30, 1080p60 or 4k60
    #[arg(short, long, value_name = "PRESET", default_value = "1080p60")]
    pub quality: export::Quality,
    /// Output; its extension picks the format: png or svg
    #[arg(short, long, value_name = "PATH", default_value = "frame.png")]
    pub output: PathBuf,
    /// Time to render, e.g. 3.5 or 3.5s
    #[arg(long, value_name = "SECS", value_parser = parse_secs, default_value = "0")]
    pub at: f32,
    /// Frame index to render
    #[arg(long, value_name = "N", conflicts_with = "at")]
    pub frame: Option<u32>,
    /// Render on the CPU even when a GPU is available
    #[arg(long)]
    pub cpu: bool,
}

fn parse_secs(v: &str) -> Result<f32, String> {
    let secs = v.strip_suffix('s').unwrap_or(v);
    secs.parse().map_err(|_| format!("bad time `{v}`"))
}

/// Builds the scene with `construct`, then previews or exports it depending on the command-line
/// arguments (run with `--help` for usage). Exits the process on error.
pub fn run(construct: impl FnOnce(&mut Scene)) {
    let cli = Cli::parse();
    let mut s = Scene::new();
    construct(&mut s);
    if let Err(e) = run_command(cli.command, s.bake(), None) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

/// Polled every preview frame; a timeline it returns replaces the one shown.
pub type Reload = Box<dyn FnMut() -> Option<BakedTimeline> + Send + Sync>;

/// Previews or exports `tl` as `cmd` says (`None` previews). In preview, `reload` is polled
/// every frame, and a timeline it returns is shown from the current time on.
pub fn run_command(
    cmd: Option<Command>,
    tl: BakedTimeline,
    reload: Option<Reload>,
) -> Result<(), String> {
    match resolve(cmd, &tl)? {
        None => {
            preview_with(tl, reload);
            Ok(())
        }
        Some(ex) => export::export(&tl, &ex),
    }
}

/// Turns a command into what to export, or `None` to preview.
fn resolve(cmd: Option<Command>, tl: &BakedTimeline) -> Result<Option<export::Export>, String> {
    let (quality, output, range, cpu) = match cmd {
        None | Some(Command::Preview) => return Ok(None),
        Some(Command::Render(RenderArgs {
            quality,
            output,
            section,
            cpu,
        })) => {
            let dur = tl.duration();
            let range = match section {
                None => (0.0, dur),
                Some(name) => {
                    let start = tl.marker(&name).ok_or(format!("no marker `{name}`"))?;
                    let end = tl
                        .markers()
                        .iter()
                        .map(|&(_, t)| t)
                        .filter(|&t| t > start)
                        .reduce(f32::min)
                        .unwrap_or(dur);
                    (start, end)
                }
            };
            (quality, output, range, cpu)
        }
        Some(Command::Still(StillArgs {
            quality,
            output,
            at,
            frame,
            cpu,
        })) => {
            let t = frame.map_or(at, |n| n as f32 / quality.fps as f32);
            (quality, output, (t, t), cpu)
        }
    };
    Ok(Some(export::Export {
        quality,
        output,
        range,
        workers: None,
        cpu,
    }))
}

/// Opens a window that plays `timeline` with a scrubber. Blocks until the window closes.
pub fn preview(timeline: BakedTimeline) {
    preview_with(timeline, None);
}

fn preview_with(timeline: BakedTimeline, reload: Option<Reload>) {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "fastanim".into(),
            resolution: (1280, 720).into(),
            ..default()
        }),
        ..default()
    }))
    .add_plugins(FastanimPlugin::new(timeline));
    if let Some(r) = reload {
        app.insert_resource(Reloader(r))
            .add_systems(Update, reload_timeline.before(controls));
    }
    app.run();
}

#[derive(Resource)]
struct Reloader(Reload);

fn reload_timeline(
    mut reload: ResMut<Reloader>,
    mut tl: ResMut<Timeline>,
    mut clock: ResMut<SceneClock>,
    mut region: ResMut<LoopRegion>,
) {
    if let Some(new) = (reload.0)() {
        let dur = new.duration();
        clock.t = clock.t.min(dur);
        // The region survives reloads, cut to the new duration.
        region.0 = (region.0)
            .map(|(a, b)| (a.min(dur), b.min(dur)))
            .filter(|(a, b)| b > a);
        tl.0 = Arc::new(new);
    }
}

/// Plays a [`BakedTimeline`] in a window above a timeline of its plays, updaters and sounds, with
/// keyboard and mouse controls: Space play/pause, ←/→ step a frame, `[`/`]` previous/next marker,
/// drag along the timeline to scrub, Shift+drag to select a region that playback loops over, Esc
/// to clear it, and the mouse wheel over the timeline to scroll lanes past the first five.
pub struct FastanimPlugin {
    timeline: Arc<BakedTimeline>,
}

impl FastanimPlugin {
    /// A plugin playing `timeline`.
    pub fn new(timeline: BakedTimeline) -> Self {
        Self {
            timeline: Arc::new(timeline),
        }
    }
}

/// The baked scene being shown.
#[derive(Resource, Clone)]
pub struct Timeline(pub Arc<BakedTimeline>);

/// The only source of scene time (not Bevy's `Time`).
#[derive(Resource, Debug, Clone, Copy)]
pub struct SceneClock {
    /// Current time in seconds.
    pub t: f32,
    /// Whether `t` advances with real time.
    pub playing: bool,
    /// Playback speed multiplier.
    pub speed: f32,
}

/// The `(start, end)` seconds playback loops over, if a region of the timeline is selected.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct LoopRegion(pub Option<(f32, f32)>);

/// The first timeline lane shown; the panel shows [`MAX_LANES`] from here.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
struct LaneScroll(usize);

/// The shader fills [`draw`] left for the render world to draw before Vello.
#[derive(Resource, Clone, Default, ExtractResource)]
struct ShaderJobs(Vec<ShaderJob>);

/// The preview device's compiled shader fills, in the render world.
#[derive(Resource, Default)]
struct PreviewShaders(ShaderFills);

fn prepare_shaders(
    jobs: Res<ShaderJobs>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    renderer: Res<VelloRenderer>,
    mut fills: ResMut<PreviewShaders>,
) {
    let mut renderer = renderer.lock().unwrap();
    fills
        .0
        .prepare(device.wgpu_device(), &queue, &mut renderer, &jobs.0);
}

/// Marks the entity holding the encoded scene.
#[derive(Component)]
struct Canvas;

/// Marks a timeline block's label.
#[derive(Component)]
struct BlockLabel;

const STEP: f32 = 1.0 / 60.0;
// shortcut: preview shader fills at 1080p density whatever the window size; follow the
// camera's scale if large windows look soft.
const PREVIEW_PX_PER_UNIT: f64 = 1920.0 / FRAME_WIDTH;
// The timeline panel sits in scene units (y up) under the frame.
const TRACK_X0: f64 = -FRAME_WIDTH / 2.0 + 0.2;
const TRACK_W: f64 = FRAME_WIDTH - 0.4;
const PANEL_TOP: f64 = -FRAME_HEIGHT / 2.0;
const LANE_H: f64 = 0.4;
/// Lanes the timeline panel shows at once; the mouse wheel scrolls through the rest.
const MAX_LANES: usize = 5;
const PAD: f64 = 0.1;
/// Labels are typeset at this many pixels and scaled down to scene units, so they stay sharp.
const LABEL_PX: f32 = 20.0;
const LABEL_SCALE: f32 = 0.01;

impl Plugin for FastanimPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(VelloPlugin::default())
            .insert_resource(ClearColor(Color::BLACK.to_bevy()))
            .insert_resource(Timeline(self.timeline.clone()))
            .insert_resource(SceneClock {
                t: 0.0,
                playing: true,
                speed: 1.0,
            })
            .init_resource::<LoopRegion>()
            .init_resource::<LaneScroll>()
            .init_resource::<ShaderJobs>()
            .add_plugins(ExtractResourcePlugin::<ShaderJobs>::default())
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    fit_camera.run_if(resource_changed::<Timeline>),
                    controls,
                    label_blocks.run_if(
                        resource_changed::<Timeline>.or_else(resource_changed::<LaneScroll>),
                    ),
                    advance_clock,
                    draw,
                )
                    .chain(),
            );
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.init_resource::<PreviewShaders>().add_systems(
                Render,
                prepare_shaders
                    .in_set(RenderSystems::Prepare)
                    .run_if(resource_exists::<VelloRenderer>),
            );
        }
        #[cfg(feature = "audio")]
        app.init_resource::<audio::AudioOut>()
            .add_systems(Update, audio::sync_audio.after(advance_clock));
    }
}

trait ToBevy {
    fn to_bevy(self) -> bevy::color::Color;
}

impl ToBevy for Color {
    fn to_bevy(self) -> bevy::color::Color {
        let [r, g, b, a] = self.components;
        bevy::color::Color::srgba(r, g, b, a)
    }
}

fn setup(mut commands: Commands) {
    commands.spawn((Camera2d, VelloView));
    commands.spawn((Canvas, VelloScene2d::new()));
}

/// Rows of blocks on the timeline, at least one.
fn lanes(tl: &BakedTimeline) -> usize {
    tl.events().iter().map(|e| e.lane + 1).max().unwrap_or(1)
}

/// Height of the timeline panel: one lane per row of blocks, at most [`MAX_LANES`].
fn panel_height(tl: &BakedTimeline) -> f64 {
    lanes(tl).min(MAX_LANES) as f64 * LANE_H + 2.0 * PAD
}

/// The first lane to show when scrolled to `scroll`, so the last lane never scrolls past the
/// bottom.
fn first_lane(tl: &BakedTimeline, scroll: usize) -> usize {
    scroll.min(lanes(tl).saturating_sub(MAX_LANES))
}

/// World units are scene units: the whole 16:9 frame and the panel under it always fit.
fn fit_camera(tl: Res<Timeline>, mut cams: Query<(&mut Projection, &mut Transform), With<Camera>>) {
    let h = panel_height(&tl.0);
    for (mut proj, mut tf) in &mut cams {
        *proj = Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: FRAME_WIDTH as f32,
                min_height: (FRAME_HEIGHT + h) as f32,
            },
            ..OrthographicProjection::default_2d()
        });
        tf.translation.y = -(h / 2.0) as f32;
    }
}

/// Where event `e` is drawn on the timeline scrolled to lane `first`, in scene units; `None`
/// if its lane is scrolled out of view.
fn block_rect(e: &SceneEvent, dur: f32, first: usize) -> Option<Rect> {
    let row = e.lane.checked_sub(first).filter(|&r| r < MAX_LANES)?;
    let top = PANEL_TOP - PAD - row as f64 * LANE_H;
    let (x0, x1) = (track_x(e.start, dur), track_x(e.end, dur));
    Some(Rect::new(
        x0,
        top - LANE_H + 0.04,
        x1.max(x0 + 0.03),
        top - 0.04,
    ))
}

fn track_x(t: f32, dur: f32) -> f64 {
    TRACK_X0 + TRACK_W * f64::from(t / dur.max(f32::EPSILON)).clamp(0.0, 1.0)
}

/// The time under scene x coordinate `x` on the timeline.
fn track_t(x: f64, dur: f32) -> f32 {
    ((x - TRACK_X0) / TRACK_W) as f32 * dur
}

/// One label per timeline block, rebuilt with the timeline.
fn label_blocks(
    mut commands: Commands,
    tl: Res<Timeline>,
    scroll: Res<LaneScroll>,
    old: Query<Entity, With<BlockLabel>>,
) {
    for e in &old {
        commands.entity(e).despawn();
    }
    let dur = tl.0.duration();
    let first = first_lane(&tl.0, scroll.0);
    for e in tl.0.events() {
        let Some(r) = block_rect(e, dur, first) else {
            continue;
        };
        let text = fit_label(&e.label, r.width() - 0.1);
        if text.is_empty() {
            continue;
        }
        let at = Vec3::new((r.x0 + 0.05) as f32, r.center().y as f32, 1.0);
        commands.spawn((
            BlockLabel,
            Text2d::new(text),
            TextFont::from_font_size(LABEL_PX),
            Anchor::CENTER_LEFT,
            Transform::from_translation(at).with_scale(Vec3::splat(LABEL_SCALE)),
        ));
    }
}

/// `label`, cut to fit `width` scene units in Bevy's default font (monospace, 0.6 em wide).
fn fit_label(label: &str, width: f64) -> String {
    let n = (width / (0.6 * f64::from(LABEL_PX * LABEL_SCALE))).max(0.0) as usize;
    label.chars().take(n).collect()
}

fn advance_clock(
    time: Res<Time>,
    tl: Res<Timeline>,
    region: Res<LoopRegion>,
    mut clock: ResMut<SceneClock>,
) {
    if clock.playing {
        clock.t += time.delta_secs() * clock.speed;
        if let Some((a, b)) = region.0 {
            if clock.t < a || clock.t >= b {
                clock.t = a;
            }
        } else if clock.t >= tl.0.duration() {
            clock.t = tl.0.duration();
            clock.playing = false;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn controls(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    wheel: Res<AccumulatedMouseScroll>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    tl: Res<Timeline>,
    mut clock: ResMut<SceneClock>,
    mut region: ResMut<LoopRegion>,
    mut scroll: ResMut<LaneScroll>,
    // Wheel movement not yet a whole lane, in lanes.
    mut wheeled: Local<f32>,
    // Whether the current drag started on the timeline, so it scrubs even off it.
    mut scrubbing: Local<bool>,
    // Where the current Shift+drag started, in seconds.
    mut anchor: Local<Option<f32>>,
) {
    let dur = tl.0.duration();
    if keys.just_pressed(KeyCode::Space) {
        if clock.t >= dur {
            clock.t = 0.0;
        }
        clock.playing = !clock.playing;
    }
    let now = clock.t;
    // Half a frame of slack so repeated presses step past the marker just landed on.
    let markers = tl.0.markers().iter().map(|&(_, t)| t);
    let mut target = None;
    if keys.just_pressed(KeyCode::ArrowRight) {
        target = Some(now + STEP);
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        target = Some(now - STEP);
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        let next = markers
            .clone()
            .filter(|&m| m > now + STEP / 2.0)
            .reduce(f32::min);
        target = Some(next.unwrap_or(dur));
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        let prev = markers.filter(|&m| m < now - STEP / 2.0).reduce(f32::max);
        target = Some(prev.unwrap_or(0.0));
    }
    if keys.just_pressed(KeyCode::Escape) {
        region.0 = None;
    }
    if !mouse.pressed(MouseButton::Left) {
        *scrubbing = false;
        *anchor = None;
    }
    if let (Ok(window), Ok((cam, cam_tf))) = (windows.single(), camera.single())
        && let Some(p) = window
            .cursor_position()
            .and_then(|c| cam.viewport_to_world_2d(cam_tf, c).ok())
    {
        let (x, y) = (f64::from(p.x), f64::from(p.y));
        let t = track_t(x, dur).clamp(0.0, dur);
        if y <= PANEL_TOP {
            // Wheel down shows later lanes. Touchpads report pixels; take 20 as a line.
            *wheeled -= match wheel.unit {
                MouseScrollUnit::Line => wheel.delta.y,
                MouseScrollUnit::Pixel => wheel.delta.y / 20.0,
            };
            let lanes = wheeled.trunc();
            *wheeled -= lanes;
            let first = first_lane(&tl.0, scroll.0) as f32 + lanes;
            scroll.set_if_neq(LaneScroll(first_lane(&tl.0, first.max(0.0) as usize)));
        }
        if mouse.just_pressed(MouseButton::Left) && y <= PANEL_TOP {
            if keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
                *anchor = Some(t);
            } else {
                *scrubbing = true;
            }
        }
        if *scrubbing {
            target = Some(t);
        }
        // A Shift+click without a drag selects nothing, so it leaves the region alone.
        if let Some(a) = *anchor
            && (t - a).abs() > STEP
        {
            region.0 = Some((a.min(t), a.max(t)));
            clock.t = a.min(t);
        }
    }
    if let Some(t) = target {
        clock.t = t.clamp(0.0, dur);
        clock.playing = false;
    }
}

fn draw(
    tl: Res<Timeline>,
    clock: Res<SceneClock>,
    region: Res<LoopRegion>,
    scroll: Res<LaneScroll>,
    mut canvas: Query<&mut VelloScene2d, With<Canvas>>,
    mut jobs: ResMut<ShaderJobs>,
) {
    let Ok(mut scene) = canvas.single_mut() else {
        return;
    };
    let tl = &tl.0;
    scene.reset();
    // The frame's background; the window around it stays black.
    let frame = Rect::new(
        -FRAME_WIDTH / 2.0,
        -FRAME_HEIGHT / 2.0,
        FRAME_WIDTH / 2.0,
        FRAME_HEIGHT / 2.0,
    );
    let bg = color(tl.background(), 1.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &frame);
    jobs.0 = encode(
        &mut ***scene,
        &tl.eval(clock.t),
        clock.t,
        PREVIEW_PX_PER_UNIT,
    );
    let first = first_lane(tl, scroll.0);
    draw_timeline(&mut scene, tl, clock.t, region.0, first);
}

/// What [`encode`] draws into: a Vello scene for the GPU, or the CPU renderer of
/// [`export::FrameRenderer`].
pub trait DrawTarget {
    /// Fills `path`, placed by `transform`, with `brush`, placed by `brush_transform` within it.
    fn fill(
        &mut self,
        transform: Affine,
        brush: &Brush,
        brush_transform: Option<Affine>,
        path: &BezPath,
    );
    /// Strokes `path`, placed by `transform`, with `brush`.
    fn stroke(&mut self, stroke: &kurbo::Stroke, transform: Affine, brush: &Brush, path: &BezPath);
    /// Whether this target can draw shader fills; where not, they draw their solid fill.
    fn shaders(&self) -> bool {
        true
    }
}

impl DrawTarget for vello::Scene {
    fn fill(
        &mut self,
        transform: Affine,
        brush: &Brush,
        brush_transform: Option<Affine>,
        path: &BezPath,
    ) {
        self.fill(Fill::NonZero, transform, brush, brush_transform, path);
    }

    fn stroke(&mut self, stroke: &kurbo::Stroke, transform: Affine, brush: &Brush, path: &BezPath) {
        self.stroke(stroke, transform, brush, None, path);
    }
}

/// Encodes one frame at scene time `t` in scene units (y up) into `scene`, in draw order, and
/// returns the shader fills to render at `px_per_unit` before Vello draws it.
pub fn encode(
    scene: &mut impl DrawTarget,
    state: &SceneState,
    t: f32,
    px_per_unit: f64,
) -> Vec<ShaderJob> {
    let mut order: Vec<_> = state.iter().collect();
    order.sort_by_key(|(id, m)| (m.z_index, **id));
    let mut jobs = Vec::new();
    let shaders = scene.shaders();
    for (id, m) in order {
        let job = m.shader.as_ref().filter(|_| shaders).map(|f| {
            let a = f.transform.as_coeffs();
            let size = (a[0].hypot(a[1]), a[2].hypot(a[3]));
            let c = m.fill;
            ShaderJob::new(
                id.0,
                f.source.clone(),
                size,
                px_per_unit,
                [c.r, c.g, c.b, c.a],
                t,
            )
        });
        encode_one(scene, m, job.as_ref());
        jobs.extend(job);
    }
    jobs
}

// ponytail: opacity multiplies fill and stroke alpha separately, so overlapping fill and
// stroke show through each other while fading; use a Vello layer per mobject if that shows.
fn encode_one(scene: &mut impl DrawTarget, m: &VState, shader: Option<&ShaderJob>) {
    if m.opacity <= 0.0 {
        return;
    }
    if let Some(i) = &m.image {
        encode_image(scene, i, ImageFill::alpha(m.opacity, &m.draw_range));
    }
    let path = bez_path(&m.path.trim(m.draw_range.clone()));
    if path.elements().is_empty() {
        return;
    }
    let flip = Affine::FLIP_Y;
    if let (Some(job), Some(f)) = (shader, &m.shader) {
        let img = &job.image;
        let brush = Brush::Image(ImageBrush::new(img.clone()).with_alpha(m.fill.a * m.opacity));
        let to_unit =
            Affine::scale_non_uniform(1.0 / f64::from(img.width), 1.0 / f64::from(img.height));
        scene.fill(flip, &brush, Some(f.transform * to_unit), &path);
    } else if m.fill.a > 0.0 {
        let brush = paint(m.fill, m.fill_gradient.as_deref(), m.opacity);
        scene.fill(flip, &brush, None, &path);
    }
    if m.stroke.color.a > 0.0 && m.stroke.width > 0.0 {
        let stroke = kurbo::Stroke::new(m.stroke.width)
            .with_join(match m.stroke.join {
                LineJoin::Miter => Join::Miter,
                LineJoin::Round => Join::Round,
                LineJoin::Bevel => Join::Bevel,
            })
            .with_caps(match m.stroke.cap {
                LineCap::Butt => Cap::Butt,
                LineCap::Round => Cap::Round,
                LineCap::Square => Cap::Square,
            });
        let brush = paint(m.stroke.color, m.stroke_gradient.as_deref(), m.opacity);
        match small_stroke(&path, &stroke) {
            Some(outline) => scene.fill(flip, &brush, None, &outline),
            None => scene.stroke(&stroke, flip, &brush, &path),
        }
    }
}

/// The outline of `path` stroked with `stroke`, worked out on the CPU, when `path` is smaller
/// than the stroke is wide; `None` for any other path, which Vello strokes itself.
///
/// Vello's GPU stroker gets curves tighter than half the stroke width wrong: a circle of radius
/// 1e-6 under a 0.01 stroke comes out as a stray dash, one of radius 1e-3 under a 0.05 stroke as
/// a hollow ring instead of a disk: the inner offset contour turns inside out and cuts a hole.
/// kurbo's CPU stroker does the same, but a closed path smaller than the stroke has nothing
/// inside its outer contour farther than half the width from the curve, so that contour alone
/// is the outline. Below kurbo's tolerance its outline is wrong too (ten times the area); a
/// subpath that small is a point.
fn small_stroke(path: &BezPath, stroke: &kurbo::Stroke) -> Option<BezPath> {
    use kurbo::Shape as _;
    let b = path.bounding_box();
    let w = stroke.width;
    if b.width().max(b.height()) >= w {
        return None;
    }
    let (opts, tolerance) = (kurbo::StrokeOpts::default(), 1e-3 * w);
    let area = |p: &BezPath| p.bounding_box().area();
    let mut out = BezPath::new();
    for sp in contours(path) {
        let closed = sp.elements().last() == Some(&kurbo::PathEl::ClosePath);
        let b = sp.bounding_box();
        if b.width().max(b.height()) < tolerance {
            let dot = kurbo::Circle::new(b.center(), w / 2.0).to_path(tolerance);
            out.extend(match stroke.start_cap {
                _ if closed => dot,
                Cap::Round => dot,
                Cap::Square => Rect::from_center_size(b.center(), (w, w)).to_path(0.0),
                Cap::Butt => BezPath::new(),
            });
            continue;
        }
        let outline = kurbo::stroke(sp.iter(), stroke, &opts, tolerance);
        if closed {
            let outer = contours(&outline).into_iter();
            out.extend(
                outer
                    .max_by(|a, b| area(a).total_cmp(&area(b)))
                    .unwrap_or_default(),
            );
        } else {
            out.extend(outline);
        }
    }
    Some(out)
}

/// `p`'s subpaths, one path each.
fn contours(p: &BezPath) -> Vec<BezPath> {
    let mut out: Vec<BezPath> = Vec::new();
    for &el in p.elements() {
        match (el, out.last_mut()) {
            (kurbo::PathEl::MoveTo(_), _) | (_, None) => out.push(BezPath::from_vec(vec![el])),
            (_, Some(c)) => c.push(el),
        }
    }
    out
}

fn color(c: fastanim_core::Color, alpha: f32) -> Color {
    Color::new([c.r, c.g, c.b, c.a * alpha])
}

/// A solid `c`, or `gradient` with its alpha scaled by `c`'s; both faded by `opacity`.
fn paint(
    c: fastanim_core::Color,
    gradient: Option<&fastanim_core::Gradient>,
    opacity: f32,
) -> Brush {
    let Some(g) = gradient else {
        return Brush::Solid(color(c, opacity));
    };
    let stops: Vec<ColorStop> = (g.stops(c.a * opacity))
        .map(|(offset, c)| ColorStop {
            offset,
            color: color(c, 1.0).into(),
        })
        .collect();
    let (a, b) = (g.from, g.to);
    Brush::Gradient(peniko::Gradient::new_linear((a.x, a.y), (b.x, b.y)).with_stops(&stops[..]))
}

/// Draws `i`'s pixels over the unit square its transform places.
fn encode_image(scene: &mut impl DrawTarget, i: &ImageFill, alpha: f32) {
    if alpha <= 0.0 {
        return;
    }
    let img = &i.image;
    let data = ImageData {
        data: blob(img),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: img.width(),
        height: img.height(),
    };
    // Vello's `Low` is nearest-neighbour.
    let quality = if i.smooth {
        ImageQuality::Medium
    } else {
        ImageQuality::Low
    };
    let brush = ImageBrush::new(data)
        .with_quality(quality)
        .with_alpha(alpha);
    let (w, h) = (f64::from(img.width()), f64::from(img.height()));
    let to_unit = Affine::scale_non_uniform(1.0 / w, 1.0 / h);
    // What `vello::Scene::draw_image` does: fill the image's pixel rectangle with it.
    let rect = kurbo::Shape::to_path(&Rect::new(0.0, 0.0, w, h), 0.1);
    let transform = Affine::FLIP_Y * i.transform * to_unit;
    scene.fill(transform, &Brush::Image(brush), None, &rect);
}

/// One Vello blob per image, so the renderer sees the same image id every frame instead of a
/// new one to upload.
fn blob(img: &fastanim_core::Image) -> Blob<u8> {
    /// Each image's pixels, weakly, and the blob made for them.
    type Blobs = Vec<(Weak<Vec<u8>>, Blob<u8>)>;
    static BLOBS: Mutex<Blobs> = Mutex::new(Vec::new());
    let px = img.shared_pixels();
    let mut blobs = BLOBS.lock().unwrap();
    // The blob holds one reference itself; drop it once nothing else does.
    blobs.retain(|(w, _)| w.strong_count() > 1);
    if let Some((_, b)) = blobs.iter().find(|(w, _)| w.as_ptr() == Arc::as_ptr(&px)) {
        return b.clone();
    }
    let b = Blob::new(px.clone());
    blobs.push((Arc::downgrade(&px), b.clone()));
    b
}

fn bez_path(p: &fastanim_core::VPath) -> BezPath {
    let mut out = BezPath::new();
    for sp in &p.subpaths {
        let Some(first) = sp.segments.first() else {
            continue;
        };
        out.move_to((first.p0.x, first.p0.y));
        for s in &sp.segments {
            out.curve_to((s.p1.x, s.p1.y), (s.p2.x, s.p2.y), (s.p3.x, s.p3.y));
        }
        if sp.closed {
            out.close_path();
        }
    }
    out
}

/// The timeline panel under the frame, scrolled to lane `first`: a block per event, a tick per
/// marker, the loop region, the playhead and, with more lanes than fit, a scrollbar.
fn draw_timeline(
    scene: &mut vello::Scene,
    tl: &BakedTimeline,
    t: f32,
    region: Option<(f32, f32)>,
    first: usize,
) {
    let flip = Affine::FLIP_Y;
    let dur = tl.duration();
    let bottom = PANEL_TOP - panel_height(tl);
    // Opaque and wider than any window, so shapes past the frame's bottom edge stay hidden.
    let panel = Rect::new(-1e3, -1e3, 1e3, PANEL_TOP);
    scene.fill(
        Fill::NonZero,
        flip,
        Color::from_rgb8(17, 17, 17),
        None,
        &panel,
    );
    let edge = kurbo::Stroke::new(0.015);
    for e in tl.events() {
        let (fill, line) = match e.kind {
            EventKind::Play => ((30, 61, 74), (58, 121, 144)),
            EventKind::Updater => ((36, 64, 42), (76, 138, 88)),
            EventKind::Sound => ((59, 42, 74), (122, 90, 150)),
        };
        let rgb = |(r, g, b)| Color::from_rgb8(r, g, b);
        let Some(r) = block_rect(e, dur, first) else {
            continue;
        };
        let r = r.to_rounded_rect(0.04);
        scene.fill(Fill::NonZero, flip, rgb(fill), None, &r);
        scene.stroke(&edge, flip, rgb(line), None, &r);
    }
    let lanes = lanes(tl);
    if lanes > MAX_LANES {
        // In the margin right of the track: the visible lanes' share of all of them.
        let (x, h) = (FRAME_WIDTH / 2.0 - 0.09, PANEL_TOP - PAD - (bottom + PAD));
        let bar = |from: usize, to: usize| {
            let y = |lane: usize| PANEL_TOP - PAD - h * lane as f64 / lanes as f64;
            Rect::new(x - 0.02, y(to), x + 0.02, y(from)).to_rounded_rect(0.02)
        };
        let track = Color::new([1.0, 1.0, 1.0, 0.1]);
        scene.fill(Fill::NonZero, flip, track, None, &bar(0, lanes));
        let thumb = Color::new([1.0, 1.0, 1.0, 0.4]);
        scene.fill(
            Fill::NonZero,
            flip,
            thumb,
            None,
            &bar(first, first + MAX_LANES),
        );
    }
    let line = |x: f64, w: f64| Rect::new(x - w, bottom, x + w, PANEL_TOP);
    if let Some((a, b)) = region {
        let r = Rect::new(track_x(a, dur), bottom, track_x(b, dur), PANEL_TOP);
        scene.fill(
            Fill::NonZero,
            flip,
            Color::new([1.0, 1.0, 0.0, 0.15]),
            None,
            &r,
        );
        let yellow = Color::from_rgb8(255, 255, 0);
        scene.fill(Fill::NonZero, flip, yellow, None, &line(r.x0, 0.008));
        scene.fill(Fill::NonZero, flip, yellow, None, &line(r.x1, 0.008));
    }
    for &(_, m) in tl.markers() {
        let tick = Color::new([1.0, 1.0, 1.0, 0.4]);
        scene.fill(
            Fill::NonZero,
            flip,
            tick,
            None,
            &line(track_x(m, dur), 0.005),
        );
    }
    let accent = Color::new([0.345, 0.769, 0.867, 1.0]);
    scene.fill(
        Fill::NonZero,
        flip,
        accent,
        None,
        &line(track_x(t, dur), 0.012),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timeline() -> BakedTimeline {
        let mut s = Scene::new();
        let c = s.add(VState::circle(1.0));
        s.play(fastanim_core::create(c));
        s.marker("a");
        s.wait(1.0);
        s.marker("b");
        s.wait(1.0);
        s.bake()
    }

    fn parse(args: &str) -> Result<Option<export::Export>, String> {
        let cli = Cli::try_parse_from(["scene"].into_iter().chain(args.split_whitespace()))
            .map_err(|e| e.to_string())?;
        resolve(cli.command, &timeline())
    }

    fn export(args: &str) -> export::Export {
        match parse(args) {
            Ok(Some(ex)) => ex,
            other => panic!("{args}: {other:?}"),
        }
    }

    #[test]
    fn commands() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
        assert!(matches!(parse(""), Ok(None)));
        assert!(matches!(parse("preview"), Ok(None)));
        let ex = export("render -q 720p30 -o x.webm");
        assert_eq!((ex.range, ex.output), ((0.0, 3.0), "x.webm".into()));
        assert_eq!(ex.quality.fps, 30);
        assert_eq!(export("render").output, PathBuf::from("out.mp4"));
        assert_eq!(export("render --section a").range, (1.0, 2.0));
        assert_eq!(export("render --section b").range, (2.0, 3.0));
        assert_eq!(export("still --at 1.5s").range, (1.5, 1.5));
        assert_eq!(export("still --frame 30 -q 720p30").range, (1.0, 1.0));
        assert_eq!(export("still").output, PathBuf::from("frame.png"));
        assert!(!export("render").cpu);
        assert!(export("render --cpu").cpu);
        assert!(export("still --cpu").cpu);
        for bad in [
            "render --help",
            "render -q 8k",
            "render --section zz",
            "still --at x",
            "still --at 1 --frame 2",
            "nope",
            "-o",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn small_strokes_are_outlined_on_the_cpu() {
        use kurbo::Shape as _;
        let stroke = kurbo::Stroke::new(0.05);
        let outline = |r: f64| small_stroke(&bez_path(&VState::circle(r).path), &stroke);
        // A stroked circle smaller than the stroke is a disk: its centre is covered.
        for r in [0.0, 1e-12, 1e-8, 1e-7, 1e-6, 1e-4, 1e-3, 0.02] {
            let o = outline(r).unwrap();
            let disk = std::f64::consts::PI * (r + 0.025) * (r + 0.025);
            assert!(
                (o.area().abs() - disk).abs() < 1e-2 * disk,
                "{r}: {}",
                o.area()
            );
            assert_ne!(o.winding(kurbo::Point::ORIGIN), 0, "{r}");
        }
        assert_eq!(outline(0.1), None);
        // A short arc with round caps is a dot too.
        let arc = bez_path(&VState::arc(1e-4, 0.0, 1.0).path);
        let dot = small_stroke(&arc, &stroke.clone().with_caps(Cap::Round)).unwrap();
        assert_ne!(dot.winding(kurbo::Point::new(1e-4, 0.0)), 0);
        assert!((dot.area().abs() - std::f64::consts::PI * 0.025 * 0.025).abs() < 1e-5);
        // A line too short to see takes its caps' shape.
        let point =
            bez_path(&VState::line(kurbo::Point::ORIGIN, kurbo::Point::new(1e-9, 0.0)).path);
        let capped = |cap| small_stroke(&point, &stroke.clone().with_caps(cap)).unwrap();
        assert!(capped(Cap::Butt).is_empty());
        assert!((capped(Cap::Square).area().abs() - 0.05 * 0.05).abs() < 1e-9);
        // Each subpath keeps its own outer contour.
        let mut two = bez_path(&VState::circle(1e-3).path);
        two.extend(bez_path(
            &VState::circle(1e-3)
                .shift(fastanim_core::Vec2::new(0.0, 0.03))
                .path,
        ));
        let both = small_stroke(&two, &stroke).unwrap();
        assert_ne!(both.winding(kurbo::Point::ORIGIN), 0);
        assert_ne!(both.winding(kurbo::Point::new(0.0, 0.03)), 0);
    }

    #[test]
    fn the_timeline_shows_five_lanes_and_scrolls_through_the_rest() {
        let mut s = Scene::new();
        let ids: Vec<_> = (0..8).map(|_| s.add(VState::circle(1.0))).collect();
        for &id in &ids {
            s.always(id, move |st, _| st[&id].clone());
        }
        s.play(fastanim_core::create(ids[0]));
        let tl = s.bake();
        assert!(lanes(&tl) > MAX_LANES);
        assert_eq!(panel_height(&tl), MAX_LANES as f64 * LANE_H + 2.0 * PAD);
        let last = lanes(&tl) - MAX_LANES;
        assert_eq!(first_lane(&tl, 100), last);
        let shown = |first| {
            (tl.events().iter())
                .filter(|e| block_rect(e, tl.duration(), first).is_some())
                .map(|e| e.lane)
                .collect::<std::collections::BTreeSet<_>>()
        };
        assert_eq!(shown(0), (0..MAX_LANES).collect());
        assert_eq!(shown(last), (last..lanes(&tl)).collect());
        assert_eq!(first_lane(&timeline(), 3), 0);
    }

    #[test]
    fn frames_sit_on_the_global_grid() {
        let t: Vec<f32> = export::frame_times(1.0, 1.1, 30).collect();
        assert_eq!(t, [1.0, 31.0 / 30.0, 32.0 / 30.0, 1.1]);
        assert_eq!(export::frame_times(0.0, 0.0, 60).count(), 1);
    }

    #[test]
    fn sounds_are_trimmed_and_delayed_to_the_range() {
        use fastanim_core::{Sound, SoundEvent};
        let at = |at: f32, offset: f32, duration: f32| SoundEvent {
            at,
            sound: Sound {
                path: std::path::Path::new("a.wav").into(),
                offset,
                duration,
            },
        };
        let sounds = [at(0.0, 0.5, 2.0), at(3.0, 0.0, 4.0), at(9.0, 0.0, 1.0)];
        let args = |range, ext| export::audio_args(&sounds, range, ext).join(" ");

        assert_eq!(
            args((0.0, 5.0), "mp4"),
            "-ss 0.5 -t 2 -i a.wav -ss 0 -t 2 -i a.wav \
             -filter_complex [1:a]adelay=0:all=1[a1];[2:a]adelay=3000:all=1[a2];\
             [a1][a2]amix=inputs=2:normalize=0[aout] -map 0:v -map [aout] -c:a aac"
        );
        // A section starting mid-sound skips into it.
        assert_eq!(
            args((4.0, 9.5), "webm"),
            "-ss 1 -t 3 -i a.wav -ss 0 -t 0.5 -i a.wav \
             -filter_complex [1:a]adelay=0:all=1[a1];[2:a]adelay=5000:all=1[a2];\
             [a1][a2]amix=inputs=2:normalize=0[aout] -map 0:v -map [aout] -c:a libopus"
        );
        assert_eq!(args((7.5, 8.5), "mp4"), "");
        assert_eq!(args((0.0, 5.0), "gif"), "");
    }

    #[test]
    fn labels_fit_blocks() {
        assert!((track_t(track_x(1.5, 3.0), 3.0) - 1.5).abs() < 1e-5);
        assert_eq!(fit_label("Create", 10.0), "Create");
        assert_eq!(fit_label("Create", 0.5), "Crea");
        assert_eq!(fit_label("Create", 0.1), "");
    }

    /// 64×36 renderers: the CPU one, and the GPU one if there is a GPU.
    fn renderers() -> Vec<export::FrameRenderer> {
        let gpu = export::FrameRenderer::new(64, 36, false);
        if gpu.is_err() {
            eprintln!("no GPU adapter: testing the CPU renderer only");
        }
        let cpu = export::FrameRenderer::new(64, 36, true).unwrap();
        gpu.into_iter().chain([cpu]).collect()
    }

    #[test]
    fn render_is_deterministic() {
        let tl = timeline();
        for mut r in renderers() {
            let a = r.render(&tl.eval(0.5), 0.5, tl.background()).unwrap();
            assert_eq!(a.len(), 64 * 36 * 4);
            assert_eq!(a, r.render(&tl.eval(0.5), 0.5, tl.background()).unwrap());
            assert_ne!(a, r.render(&tl.eval(0.0), 0.0, tl.background()).unwrap());
        }
    }

    #[test]
    fn frames_in_flight_come_back_in_order_and_unchanged() {
        let tl = timeline();
        let times: Vec<f32> = (0..7).map(|i| i as f32 / 6.0).collect();
        for mut r in renderers() {
            let one_by_one: Vec<_> = (times.iter())
                .map(|&t| r.render(&tl.eval(t), t, tl.background()).unwrap())
                .collect();
            let mut queued = Vec::new();
            for &t in &times {
                queued.extend(r.submit(&tl.eval(t), t, tl.background()).unwrap());
            }
            while let Some(px) = r.finish().unwrap() {
                queued.push(px);
            }
            assert_eq!(queued, one_by_one);
        }
    }

    #[test]
    fn cpu_draws_shapes_where_the_gpu_does() {
        use fastanim_core::color::{BLUE, RED};
        let mut s = Scene::new();
        // Red over the top half of the frame (y up), blue over the bottom half by a shader the
        // CPU can't run, so it draws the solid fill.
        let half = || VState::rectangle(FRAME_WIDTH, FRAME_HEIGHT / 2.0);
        s.add(
            half()
                .fill(RED)
                .shift(fastanim_core::Vec2::new(0.0, FRAME_HEIGHT / 4.0)),
        );
        let wgsl = "fn shade(uv: vec2<f32>) -> vec4<f32> { return shape.color; }";
        s.add(
            half()
                .fill(BLUE)
                .shift(fastanim_core::Vec2::new(0.0, -FRAME_HEIGHT / 4.0))
                .shader(wgsl),
        );
        let tl = s.bake();
        for mut r in renderers() {
            let px = r.render(&tl.eval(0.0), 0.0, tl.background()).unwrap();
            let near = |x: usize, y: usize, c: fastanim_core::Color| {
                let i = (y * 64 + x) * 4;
                let want = [c.r, c.g, c.b, 1.0].map(|v| (v * 255.0).round() as u8);
                let got = &px[i..i + 4];
                assert!(
                    got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 2),
                    "{got:?} {want:?}"
                );
            };
            near(32, 4, RED);
            near(32, 31, BLUE);
        }
    }

    #[test]
    fn shader_fills_render_on_the_gpu() {
        let Ok(mut r) = export::FrameRenderer::new(64, 36, false) else {
            eprintln!("skipping: no GPU adapter");
            return;
        };
        // The center pixel of a full-frame square filled by `wgsl`, over black.
        let mut center = |wgsl: &str| {
            let mut s = Scene::new();
            let sq = VState::rectangle(FRAME_WIDTH, FRAME_HEIGHT).fill(fastanim_core::color::BLUE);
            s.add(sq.shader(wgsl));
            let tl = s.bake();
            let px = r.render(&tl.eval(0.0), 0.0, tl.background()).unwrap();
            let i = (18 * 64 + 32) * 4;
            [px[i], px[i + 1], px[i + 2]]
        };
        let red =
            center("fn shade(uv: vec2<f32>) -> vec4<f32> { return vec4(1.0, 0.0, 0.0, 1.0); }");
        assert!(red[0] > 250 && red[1] < 5 && red[2] < 5, "{red:?}");
        // `shape.color` is the fill; uv is 0.5 at the center.
        let uv =
            center("fn shade(uv: vec2<f32>) -> vec4<f32> { return vec4(uv, shape.color.b, 1.0); }");
        assert!(
            uv[0].abs_diff(128) < 8 && uv[1].abs_diff(128) < 8 && uv[2] > 200,
            "{uv:?}"
        );
        // A shader that doesn't compile draws the solid fill.
        let bad = center("fn shade(uv: vec2<f32>) -> vec4<f32> { return oops; }");
        assert_eq!(
            bad,
            center("fn shade(uv: vec2<f32>) -> vec4<f32> { return shape.color; }")
        );
        assert!(bad[2] > 200 && bad[0] < 100, "{bad:?}");
    }
}
