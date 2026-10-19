//! The browser player (see `docs/SPEC.md` §14.4): bakes a Rhai scene and draws frames to a
//! canvas with Canvas 2D, without Bevy. It runs in a Web Worker (`worker.js`) on an
//! `OffscreenCanvas`, since the baked timeline holds script closures that can't be sent to the
//! page. `player.js` wraps that as a `<fastanim-player>` element, `index.html` is the playground
//! around it, and Trunk (`Trunk.toml`) builds it all into `dist/`.

use std::cell::RefCell;
use std::collections::HashMap;

use fastanim_core::color::BLACK;
use fastanim_core::svg::path_data;
use fastanim_core::{
    BakedTimeline, Color, EventKind, FRAME_WIDTH, Gradient, Image, ImageFill, LineCap, LineJoin,
    to_svg,
};
use wasm_bindgen::{Clamped, JsCast, prelude::*};
use web_sys::{
    CanvasGradient, ImageData, OffscreenCanvas, OffscreenCanvasRenderingContext2d, Path2d,
};

/// Fills the typesetting cache from a `.bundle` written by `fastanim bundle`, so the
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
    /// Each image's pixels on a canvas of their own, put there once, keyed by
    /// [`Image::key`]; the image is kept so its key isn't reused.
    images: RefCell<HashMap<usize, (Image, OffscreenCanvas)>>,
}

#[wasm_bindgen]
impl Player {
    /// Runs and bakes `src`; the error is `line:col: message`.
    #[wasm_bindgen(constructor)]
    pub fn new(src: &str) -> Result<Player, JsError> {
        fastanim_text::time_typesetting(js_sys::Date::now);
        fastanim_core::time_baking(js_sys::Date::now);
        let (t0, typeset0) = (js_sys::Date::now(), fastanim_text::typeset_ms());
        let timeline = fastanim_script::bake(src).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Player {
            timeline,
            bake_ms: js_sys::Date::now() - t0,
            typeset_ms: fastanim_text::typeset_ms() - typeset0,
            images: RefCell::default(),
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

    /// Milliseconds of the bake spent planning animations (alignment, diffing).
    pub fn plan_ms(&self) -> f64 {
        self.timeline.stats().plan_ms
    }

    /// Milliseconds of the bake spent copying the scene into each clip's keyframe.
    pub fn snapshot_ms(&self) -> f64 {
        self.timeline.stats().snapshot_ms
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

    /// Every play, updater and sound, in start order, as
    /// `{ start, end, label, kind: "play" | "updater" | "sound", lane }` objects.
    pub fn events(&self) -> Result<js_sys::Array, JsValue> {
        let out = js_sys::Array::new();
        for e in self.timeline.events() {
            let kind = match e.kind {
                EventKind::Play => "play",
                EventKind::Updater => "updater",
                EventKind::Sound => "sound",
            };
            let o = js_sys::Object::new();
            let set = |k: &str, v: JsValue| js_sys::Reflect::set(&o, &k.into(), &v);
            set("start", e.start.into())?;
            set("end", e.end.into())?;
            set("label", e.label.as_str().into())?;
            set("kind", kind.into())?;
            set("lane", (e.lane as u32).into())?;
            out.push(&o);
        }
        Ok(out)
    }

    /// The frame at `t` as an SVG document.
    pub fn svg(&self, t: f32) -> String {
        to_svg(&self.timeline.eval(t), self.timeline.background())
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
        let frame_h = fastanim_core::FRAME_HEIGHT;
        ctx.set_fill_style_str(&css(self.timeline.background().with_alpha(1.0)));
        ctx.fill_rect(-FRAME_WIDTH / 2.0, -frame_h / 2.0, FRAME_WIDTH, frame_h);
        // Vello's and SVG's.
        ctx.set_miter_limit(4.0);

        let state = self.timeline.eval(t);
        let mut order: Vec<_> = state.iter().collect();
        order.sort_by_key(|(id, m)| (m.z_index, **id));
        for (_, m) in order {
            if m.opacity <= 0.0 {
                continue;
            }
            if let Some(i) = &m.image {
                self.draw_image(ctx, i, ImageFill::alpha(m.opacity, &m.draw_range))?;
            }
            let d = path_data(&m.path.trim(m.draw_range.clone()));
            if d.is_empty() {
                continue;
            }
            let p = Path2d::new_with_path_string(&d)?;
            ctx.set_global_alpha(m.opacity.into());
            if m.fill.a > 0.0 {
                match &m.fill_gradient {
                    Some(g) => ctx.set_fill_style_canvas_gradient(&gradient(ctx, g, m.fill.a)?),
                    None => ctx.set_fill_style_str(&css(m.fill)),
                }
                ctx.fill_with_path_2d(&p);
            }
            if m.stroke.width > 0.0 && m.stroke.color.a > 0.0 {
                let c = m.stroke.color;
                match &m.stroke_gradient {
                    Some(g) => ctx.set_stroke_style_canvas_gradient(&gradient(ctx, g, c.a)?),
                    None => ctx.set_stroke_style_str(&css(c)),
                }
                ctx.set_line_width(m.stroke.width);
                ctx.set_line_join(match m.stroke.join {
                    LineJoin::Miter => "miter",
                    LineJoin::Round => "round",
                    LineJoin::Bevel => "bevel",
                });
                ctx.set_line_cap(match m.stroke.cap {
                    LineCap::Butt => "butt",
                    LineCap::Round => "round",
                    LineCap::Square => "square",
                });
                ctx.stroke_with_path(&p);
            }
        }
        Ok(())
    }
}

impl Player {
    /// Draws `i`'s pixels over the unit square its transform places.
    fn draw_image(
        &self,
        ctx: &OffscreenCanvasRenderingContext2d,
        i: &ImageFill,
        alpha: f32,
    ) -> Result<(), JsValue> {
        if alpha <= 0.0 {
            return Ok(());
        }
        let src = self.image_canvas(&i.image)?;
        ctx.save();
        ctx.set_global_alpha(alpha.into());
        ctx.set_image_smoothing_enabled(i.smooth);
        let [a, b, c, d, e, f] = i.transform.as_coeffs();
        ctx.transform(a, b, c, d, e, f)?;
        ctx.draw_image_with_offscreen_canvas_and_dw_and_dh(&src, 0.0, 0.0, 1.0, 1.0)?;
        ctx.restore();
        Ok(())
    }

    fn image_canvas(&self, img: &Image) -> Result<OffscreenCanvas, JsValue> {
        let mut images = self.images.borrow_mut();
        if let Some((_, c)) = images.get(&img.key()) {
            return Ok(c.clone());
        }
        let c = OffscreenCanvas::new(img.width(), img.height())?;
        let cx: OffscreenCanvasRenderingContext2d =
            (c.get_context("2d")?.ok_or("no 2d context")?).dyn_into()?;
        let data = ImageData::new_with_u8_clamped_array_and_sh(
            Clamped(img.pixels()),
            img.width(),
            img.height(),
        )?;
        cx.put_image_data(&data, 0.0, 0.0)?;
        images.insert(img.key(), (img.clone(), c.clone()));
        Ok(c)
    }
}

/// `g` in scene units, its alpha scaled by `alpha`.
fn gradient(
    ctx: &OffscreenCanvasRenderingContext2d,
    g: &Gradient,
    alpha: f32,
) -> Result<CanvasGradient, JsValue> {
    let cg = ctx.create_linear_gradient(g.from.x, g.from.y, g.to.x, g.to.y);
    for (offset, c) in g.stops(alpha) {
        cg.add_color_stop(offset, &css(c))?;
    }
    Ok(cg)
}

fn css(c: Color) -> String {
    let v = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round();
    format!("rgba({},{},{},{})", v(c.r), v(c.g), v(c.b), c.a)
}
