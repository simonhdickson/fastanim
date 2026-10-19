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
    let (quality, output, range) = match cmd {
        None | Some(Command::Preview) => return Ok(None),
        Some(Command::Render(RenderArgs {
            quality,
            output,
            section,
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
            (quality, output, range)
        }
        Some(Command::Still(StillArgs {
            quality,
            output,
            at,
            frame,
        })) => {
            let t = frame.map_or(at, |n| n as f32 / quality.fps as f32);
            (quality, output, (t, t))
        }
    };
    Ok(Some(export::Export {
        quality,
        output,
        range,
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
/// to clear it.
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
            .init_resource::<ShaderJobs>()
            .add_plugins(ExtractResourcePlugin::<ShaderJobs>::default())
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    (fit_camera, label_blocks).run_if(resource_changed::<Timeline>),
                    controls,
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

/// Height of the timeline panel: one lane per row of blocks, at least one.
fn panel_height(tl: &BakedTimeline) -> f64 {
    let lanes = tl.events().iter().map(|e| e.lane + 1).max().unwrap_or(1);
    lanes as f64 * LANE_H + 2.0 * PAD
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

/// Where event `e` is drawn on the timeline, in scene units.
fn block_rect(e: &SceneEvent, dur: f32) -> Rect {
    let top = PANEL_TOP - PAD - e.lane as f64 * LANE_H;
    let (x0, x1) = (track_x(e.start, dur), track_x(e.end, dur));
    Rect::new(x0, top - LANE_H + 0.04, x1.max(x0 + 0.03), top - 0.04)
}

fn track_x(t: f32, dur: f32) -> f64 {
    TRACK_X0 + TRACK_W * f64::from(t / dur.max(f32::EPSILON)).clamp(0.0, 1.0)
}

/// The time under scene x coordinate `x` on the timeline.
fn track_t(x: f64, dur: f32) -> f32 {
    ((x - TRACK_X0) / TRACK_W) as f32 * dur
}

/// One label per timeline block, rebuilt with the timeline.
fn label_blocks(mut commands: Commands, tl: Res<Timeline>, old: Query<Entity, With<BlockLabel>>) {
    for e in &old {
        commands.entity(e).despawn();
    }
    let dur = tl.0.duration();
    for e in tl.0.events() {
        let r = block_rect(e, dur);
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
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    tl: Res<Timeline>,
    mut clock: ResMut<SceneClock>,
    mut region: ResMut<LoopRegion>,
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
    jobs.0 = encode(&mut scene, &tl.eval(clock.t), clock.t, PREVIEW_PX_PER_UNIT);
    draw_timeline(&mut scene, tl, clock.t, region.0);
}

/// Encodes one frame at scene time `t` in scene units (y up) into a Vello scene, in draw
/// order, and returns the shader fills to render at `px_per_unit` before Vello draws it.
pub fn encode(
    scene: &mut vello::Scene,
    state: &SceneState,
    t: f32,
    px_per_unit: f64,
) -> Vec<ShaderJob> {
    let mut order: Vec<_> = state.iter().collect();
    order.sort_by_key(|(id, m)| (m.z_index, **id));
    let mut jobs = Vec::new();
    for (id, m) in order {
        let job = m.shader.as_ref().map(|f| {
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
fn encode_one(scene: &mut vello::Scene, m: &VState, shader: Option<&ShaderJob>) {
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
        let brush = ImageBrush::new(img.clone()).with_alpha(m.fill.a * m.opacity);
        let to_unit =
            Affine::scale_non_uniform(1.0 / f64::from(img.width), 1.0 / f64::from(img.height));
        scene.fill(
            Fill::NonZero,
            flip,
            &brush,
            Some(f.transform * to_unit),
            &path,
        );
    } else if m.fill.a > 0.0 {
        let brush = paint(m.fill, m.fill_gradient.as_deref(), m.opacity);
        scene.fill(Fill::NonZero, flip, &brush, None, &path);
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
        scene.stroke(&stroke, flip, &brush, None, &path);
    }
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
fn encode_image(scene: &mut vello::Scene, i: &ImageFill, alpha: f32) {
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
    let to_unit =
        Affine::scale_non_uniform(1.0 / f64::from(img.width()), 1.0 / f64::from(img.height()));
    scene.draw_image(&brush, Affine::FLIP_Y * i.transform * to_unit);
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

/// The timeline panel under the frame: a block per event, a tick per marker, the loop region and
/// the playhead.
fn draw_timeline(scene: &mut vello::Scene, tl: &BakedTimeline, t: f32, region: Option<(f32, f32)>) {
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
        let r = block_rect(e, dur).to_rounded_rect(0.04);
        scene.fill(Fill::NonZero, flip, rgb(fill), None, &r);
        scene.stroke(&edge, flip, rgb(line), None, &r);
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

    #[test]
    fn gpu_render_is_deterministic() {
        let Ok(mut r) = export::FrameRenderer::new(64, 36) else {
            eprintln!("skipping: no GPU adapter");
            return;
        };
        let tl = timeline();
        let a = r.render(&tl.eval(0.5), 0.5, tl.background()).unwrap();
        assert_eq!(a.len(), 64 * 36 * 4);
        assert_eq!(a, r.render(&tl.eval(0.5), 0.5, tl.background()).unwrap());
        assert_ne!(a, r.render(&tl.eval(0.0), 0.0, tl.background()).unwrap());
    }

    #[test]
    fn shader_fills_render_on_the_gpu() {
        let Ok(mut r) = export::FrameRenderer::new(64, 36) else {
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
