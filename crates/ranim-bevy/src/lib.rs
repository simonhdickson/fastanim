//! Bevy integration for ranim: `RanimPlugin`, the scene clock, Vello rendering and the preview
//! scrubber (see `docs/SPEC.md` §8). Headless export lands in M4.
//!
//! All the logic lives in `ranim-core`; the systems here only advance a clock, call
//! [`BakedTimeline::eval`] and encode the result.
//!
//! ```no_run
//! use ranim_core::{Scene, VState, create};
//!
//! let mut s = Scene::new();
//! let c = s.add(VState::circle(1.0));
//! s.play(create(c));
//! ranim_bevy::preview(s.bake());
//! ```

use std::sync::Arc;

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy_vello::VelloPlugin;
use bevy_vello::prelude::*;
use ranim_core::{BakedTimeline, FRAME_HEIGHT, FRAME_WIDTH, SceneState, VState};

use bevy_vello::vello;
use bevy_vello::vello::kurbo::{self, Affine, BezPath, Cap, Join, Rect};
use bevy_vello::vello::peniko::{Color, Fill};

/// Opens a window that plays `timeline` with a scrubber. Blocks until the window closes.
pub fn preview(timeline: BakedTimeline) {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "ranim".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(RanimPlugin::new(timeline))
        .run();
}

/// Plays a [`BakedTimeline`] in a window, with keyboard and mouse controls:
/// Space play/pause, ←/→ step a frame, `[`/`]` previous/next marker, drag the bar to scrub.
pub struct RanimPlugin {
    timeline: Arc<BakedTimeline>,
}

impl RanimPlugin {
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

/// Marks the entity holding the encoded scene.
#[derive(Component)]
struct Canvas;

const STEP: f32 = 1.0 / 60.0;
const BAR_Y: f64 = -FRAME_HEIGHT / 2.0 + 0.2;
const BAR_X0: f64 = -FRAME_WIDTH / 2.0 + 0.4;
const BAR_W: f64 = FRAME_WIDTH - 0.8;
/// Clicks below this scene-space y scrub.
const BAR_HIT_Y: f64 = BAR_Y + 0.3;

impl Plugin for RanimPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(VelloPlugin::default())
            .insert_resource(ClearColor(Color::BLACK.to_bevy()))
            .insert_resource(Timeline(self.timeline.clone()))
            .insert_resource(SceneClock {
                t: 0.0,
                playing: true,
                speed: 1.0,
            })
            .add_systems(Startup, setup)
            .add_systems(Update, (controls, advance_clock, draw).chain());
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
    // World units are scene units: the whole 16:9 frame always fits.
    commands.spawn((
        Camera2d,
        VelloView,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: FRAME_WIDTH as f32,
                min_height: FRAME_HEIGHT as f32,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
    commands.spawn((Canvas, VelloScene2d::new()));
}

fn advance_clock(time: Res<Time>, tl: Res<Timeline>, mut clock: ResMut<SceneClock>) {
    if clock.playing {
        clock.t += time.delta_secs() * clock.speed;
        if clock.t >= tl.0.duration() {
            clock.t = tl.0.duration();
            clock.playing = false;
        }
    }
}

fn controls(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    tl: Res<Timeline>,
    mut clock: ResMut<SceneClock>,
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
    if mouse.pressed(MouseButton::Left)
        && let (Ok(window), Ok((cam, cam_tf))) = (windows.single(), camera.single())
        && let Some(p) = window
            .cursor_position()
            .and_then(|c| cam.viewport_to_world_2d(cam_tf, c).ok())
        && f64::from(p.y) < BAR_HIT_Y
    {
        target = Some(((f64::from(p.x) - BAR_X0) / BAR_W) as f32 * dur);
    }
    if let Some(t) = target {
        clock.t = t.clamp(0.0, dur);
        clock.playing = false;
    }
}

fn draw(
    tl: Res<Timeline>,
    clock: Res<SceneClock>,
    mut canvas: Query<&mut VelloScene2d, With<Canvas>>,
) {
    let Ok(mut scene) = canvas.single_mut() else {
        return;
    };
    scene.reset();
    encode(&mut scene, &tl.0.eval(clock.t));
    draw_bar(&mut scene, &tl.0, clock.t);
}

/// Encodes one frame in scene units (y up) into a Vello scene, in draw order.
pub fn encode(scene: &mut vello::Scene, state: &SceneState) {
    let mut order: Vec<_> = state.iter().collect();
    order.sort_by_key(|(id, m)| (m.z_index, **id));
    for (_, m) in order {
        encode_one(scene, m);
    }
}

// ponytail: opacity multiplies fill and stroke alpha separately, so overlapping fill and
// stroke show through each other while fading; use a Vello layer per mobject if that shows.
fn encode_one(scene: &mut vello::Scene, m: &VState) {
    if m.opacity <= 0.0 {
        return;
    }
    let path = bez_path(&m.path.trim(m.draw_range.clone()));
    if path.elements().is_empty() {
        return;
    }
    let color = |c: ranim_core::Color| Color::new([c.r, c.g, c.b, c.a * m.opacity]);
    let flip = Affine::FLIP_Y;
    if m.fill.a > 0.0 {
        scene.fill(Fill::NonZero, flip, color(m.fill), None, &path);
    }
    if m.stroke.color.a > 0.0 && m.stroke.width > 0.0 {
        let stroke = kurbo::Stroke::new(m.stroke.width)
            .with_join(Join::Round)
            .with_caps(Cap::Round);
        scene.stroke(&stroke, flip, color(m.stroke.color), None, &path);
    }
}

fn bez_path(p: &ranim_core::VPath) -> BezPath {
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

/// Progress bar along the bottom of the frame, with a tick per marker.
fn draw_bar(scene: &mut vello::Scene, tl: &BakedTimeline, t: f32) {
    const H: f64 = 0.06;
    let dur = f64::from(tl.duration()).max(f64::EPSILON);
    let x_at = |t: f32| BAR_X0 + BAR_W * (f64::from(t) / dur).clamp(0.0, 1.0);
    let rect = |x0: f64, x1: f64, h: f64| Rect::new(x0, -(BAR_Y + h), x1, -(BAR_Y - h));
    let grey = Color::new([1.0, 1.0, 1.0, 0.25]);
    let accent = Color::new([0.345, 0.769, 0.867, 0.9]);
    let id = Affine::IDENTITY;
    scene.fill(
        Fill::NonZero,
        id,
        grey,
        None,
        &rect(BAR_X0, BAR_X0 + BAR_W, H / 2.0),
    );
    scene.fill(
        Fill::NonZero,
        id,
        accent,
        None,
        &rect(BAR_X0, x_at(t), H / 2.0),
    );
    for &(_, m) in tl.markers() {
        let x = x_at(m);
        scene.fill(
            Fill::NonZero,
            id,
            Color::WHITE,
            None,
            &rect(x - 0.01, x + 0.01, H * 1.5),
        );
    }
}
