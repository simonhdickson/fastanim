//! The browser player (see `docs/SPEC.md` §14.4): bakes a Rhai scene and draws frames to a
//! `<canvas>` with Canvas 2D, without Bevy. `index.html` is the playground around it;
//! `build.sh` builds it into `dist/`.

use ranim_core::color::BLACK;
use ranim_core::svg::path_data;
use ranim_core::{BakedTimeline, Color, FRAME_WIDTH, to_svg};
use wasm_bindgen::prelude::*;
use web_sys::{CanvasRenderingContext2d, Path2d};

/// A baked scene.
#[wasm_bindgen]
pub struct Player(BakedTimeline);

#[wasm_bindgen]
impl Player {
    /// Runs and bakes `src`; the error is `line:col: message`.
    // ponytail: bakes on the calling thread; move into a Web Worker if long scenes freeze the
    // page (closures in the timeline can't cross to the page, so the worker would draw too).
    #[wasm_bindgen(constructor)]
    pub fn new(src: &str) -> Result<Player, JsError> {
        ranim_script::bake(src)
            .map(Player)
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Length in seconds.
    pub fn duration(&self) -> f32 {
        self.0.duration()
    }

    /// Marker names, in recording order.
    pub fn marker_names(&self) -> Vec<String> {
        self.0.markers().iter().map(|(n, _)| n.clone()).collect()
    }

    /// Marker times, matching [`Player::marker_names`].
    pub fn marker_times(&self) -> Vec<f32> {
        self.0.markers().iter().map(|&(_, t)| t).collect()
    }

    /// The frame at `t` as an SVG document.
    pub fn svg(&self, t: f32) -> String {
        to_svg(&self.0.eval(t), BLACK)
    }

    /// Draws the frame at `t`, filling the canvas width with the 16:9 frame.
    pub fn draw(&self, ctx: &CanvasRenderingContext2d, t: f32) -> Result<(), JsValue> {
        let canvas = ctx.canvas().ok_or("context has no canvas")?;
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

        let state = self.0.eval(t);
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
