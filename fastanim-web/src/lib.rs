//! The browser player (see `docs/SPEC.md` §14.4): bakes a Rhai scene and draws frames to a
//! canvas with Canvas 2D, without Bevy. It runs in a Web Worker (`worker.js`) on an
//! `OffscreenCanvas`, since the baked timeline holds script closures that can't be sent to the
//! page. `player.js` wraps that as a `<fastanim-player>` element, `index.html` is the playground
//! around it, and Trunk (`Trunk.toml`) builds it all into `dist/`.

use fastanim_core::color::BLACK;
use fastanim_core::svg::path_data;
use fastanim_core::{BakedTimeline, Color, FRAME_WIDTH, to_svg};
use wasm_bindgen::prelude::*;
use web_sys::{OffscreenCanvasRenderingContext2d, Path2d};

/// Fills the typesetting cache from a `.bundle` written by `fastanim run --bundle`, so the
/// text in it is never typeset here (SPEC §14.5); returns how many snippets it held.
#[wasm_bindgen]
pub fn load_bundle(bundle: &str) -> Result<usize, JsError> {
    fastanim_text::import_bundle(bundle).map_err(|e| JsError::new(&e))
}

/// Everything typeset so far, bundled or not, as a bundle for [`load_bundle`]; the worker keeps
/// it in IndexedDB so text typeset on one visit isn't typeset again on the next (SPEC §14.5).
#[wasm_bindgen]
pub fn save_bundle() -> String {
    fastanim_text::export_bundle()
}

/// A baked scene.
#[wasm_bindgen]
pub struct Player {
    timeline: BakedTimeline,
    bake_ms: f64,
    typeset_ms: f64,
}

#[wasm_bindgen]
impl Player {
    /// Runs and bakes `src`; the error is `line:col: message`.
    #[wasm_bindgen(constructor)]
    pub fn new(src: &str) -> Result<Player, JsError> {
        fastanim_text::time_typesetting(js_sys::Date::now);
        let (t0, typeset0) = (js_sys::Date::now(), fastanim_text::typeset_ms());
        let timeline = fastanim_script::bake(src).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Player {
            timeline,
            bake_ms: js_sys::Date::now() - t0,
            typeset_ms: fastanim_text::typeset_ms() - typeset0,
        })
    }

    /// Milliseconds the bake took, typesetting included.
    pub fn bake_ms(&self) -> f64 {
        self.bake_ms
    }

    /// Milliseconds of the bake spent typesetting text not already in the cache (SPEC §14.5).
    pub fn typeset_ms(&self) -> f64 {
        self.typeset_ms
    }

    /// Length in seconds.
    pub fn duration(&self) -> f32 {
        self.timeline.duration()
    }

    /// Marker names, in recording order.
    pub fn marker_names(&self) -> Vec<String> {
        self.timeline
            .markers()
            .iter()
            .map(|(n, _)| n.clone())
            .collect()
    }

    /// Marker times, matching [`Player::marker_names`].
    pub fn marker_times(&self) -> Vec<f32> {
        self.timeline.markers().iter().map(|&(_, t)| t).collect()
    }

    /// The frame at `t` as an SVG document.
    pub fn svg(&self, t: f32) -> String {
        to_svg(&self.timeline.eval(t), BLACK)
    }

    /// Draws the frame at `t`, filling the canvas width with the 16:9 frame.
    pub fn draw(&self, ctx: &OffscreenCanvasRenderingContext2d, t: f32) -> Result<(), JsValue> {
        let canvas = ctx.canvas();
        let (w, h) = (f64::from(canvas.width()), f64::from(canvas.height()));
        ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)?;
        ctx.set_global_alpha(1.0);
        ctx.set_fill_style_str(&css(BLACK));
        ctx.fill_rect(0.0, 0.0, w, h);
        // Scene units are y-up and centered.
        let k = w / FRAME_WIDTH;
        ctx.set_transform(k, 0.0, 0.0, -k, w / 2.0, h / 2.0)?;
        ctx.set_line_join("round");
        ctx.set_line_cap("round");

        let state = self.timeline.eval(t);
        let mut order: Vec<_> = state.iter().collect();
        order.sort_by_key(|(id, m)| (m.z_index, **id));
        for (_, m) in order {
            let d = path_data(&m.path.trim(m.draw_range.clone()));
            if m.opacity <= 0.0 || d.is_empty() {
                continue;
            }
            let p = Path2d::new_with_path_string(&d)?;
            ctx.set_global_alpha(m.opacity.into());
            if m.fill.a > 0.0 {
                ctx.set_fill_style_str(&css(m.fill));
                ctx.fill_with_path_2d(&p);
            }
            if m.stroke.width > 0.0 && m.stroke.color.a > 0.0 {
                ctx.set_stroke_style_str(&css(m.stroke.color));
                ctx.set_line_width(m.stroke.width);
                ctx.stroke_with_path(&p);
            }
        }
        Ok(())
    }
}

fn css(c: Color) -> String {
    let v = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round();
    format!("rgba({},{},{},{})", v(c.r), v(c.g), v(c.b), c.a)
}
