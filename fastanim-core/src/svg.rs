//! SVG still export, straight from a [`SceneState`] (no GPU).

use std::fmt::Write;

use crate::color::Color;
use crate::geom::VPath;
use crate::image::ImageFill;
use crate::mobject::{Gradient, LineCap, LineJoin, SceneState};
use crate::{FRAME_HEIGHT, FRAME_WIDTH};

/// Renders one frame as a 16:9 SVG document over a `background` fill.
pub fn to_svg(state: &SceneState, background: Color) -> String {
    let (w, h) = (FRAME_WIDTH, FRAME_HEIGHT);
    let mut out = String::new();
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="{} {} {} {}">"#,
        num(-w / 2.0),
        num(-h / 2.0),
        num(w),
        num(h)
    );
    let _ = writeln!(
        out,
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#,
        num(-w / 2.0),
        num(-h / 2.0),
        num(w),
        num(h),
        background.to_hex()
    );
    // Scene units are y-up; SVG is y-down.
    out.push_str("<g transform=\"scale(1,-1)\">\n");
    let mut order: Vec<_> = state.iter().collect();
    order.sort_by_key(|(id, m)| (m.z_index, **id));
    let mut gradients = 0;
    for (_, m) in order {
        if m.opacity <= 0.0 {
            continue;
        }
        if let Some(i) = &m.image {
            image(&mut out, i, ImageFill::alpha(m.opacity, &m.draw_range));
        }
        let d = path_data(&m.path.trim(m.draw_range.clone()));
        if d.is_empty() {
            continue;
        }
        let mut paint = |c: Color, g: Option<&Gradient>| match g {
            Some(g) => {
                gradients += 1;
                gradient(&mut out, gradients, g);
                format!("url(#g{gradients})")
            }
            None => c.to_hex(),
        };
        let fill = paint(m.fill, m.fill_gradient.as_deref());
        let stroke = paint(m.stroke.color, m.stroke_gradient.as_deref());
        let join = match m.stroke.join {
            LineJoin::Miter => "miter",
            LineJoin::Round => "round",
            LineJoin::Bevel => "bevel",
        };
        let cap = match m.stroke.cap {
            LineCap::Butt => "butt",
            LineCap::Round => "round",
            LineCap::Square => "square",
        };
        let _ = writeln!(
            out,
            r#"<path d="{d}" fill="{fill}" fill-opacity="{}" stroke="{stroke}" stroke-opacity="{}" stroke-width="{}" stroke-linejoin="{join}" stroke-linecap="{cap}" opacity="{}"/>"#,
            num(m.fill.a.into()),
            num(m.stroke.color.a.into()),
            num(m.stroke.width),
            num(m.opacity.into()),
        );
    }
    out.push_str("</g>\n</svg>\n");
    out
}

/// A `<linearGradient>` with id `g{id}`, in scene units; its stops keep their own alpha, and the
/// path's fill or stroke opacity scales it.
fn gradient(out: &mut String, id: usize, g: &Gradient) {
    let _ = writeln!(
        out,
        r#"<defs><linearGradient id="g{id}" gradientUnits="userSpaceOnUse" x1="{}" y1="{}" x2="{}" y2="{}">"#,
        num(g.from.x),
        num(g.from.y),
        num(g.to.x),
        num(g.to.y),
    );
    for (offset, c) in g.stops(1.0) {
        let _ = writeln!(
            out,
            r#"<stop offset="{}" stop-color="{}" stop-opacity="{}"/>"#,
            num(offset.into()),
            c.to_hex(),
            num(c.a.into()),
        );
    }
    out.push_str("</linearGradient></defs>\n");
}

/// An `<image>` filling the unit square under `i`'s transform, as an embedded PNG.
fn image(out: &mut String, i: &ImageFill, alpha: f32) {
    if alpha <= 0.0 {
        return;
    }
    let [a, b, c, d, e, f] = i.transform.as_coeffs().map(num);
    let pixelated = if i.smooth {
        ""
    } else {
        r#" style="image-rendering:pixelated""#
    };
    let _ = writeln!(
        out,
        r#"<image href="data:image/png;base64,{}" width="1" height="1" preserveAspectRatio="none" transform="matrix({a} {b} {c} {d} {e} {f})" opacity="{}"{pixelated}/>"#,
        base64(&i.image.to_png()),
        num(alpha.into()),
    );
}

/// Standard base64 with padding.
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | (u32::from(b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// SVG path data (`M`, `C`, `Z`) for `p`, in scene units; also what canvas `Path2D` takes.
pub fn path_data(p: &VPath) -> String {
    let mut d = String::new();
    for sp in &p.subpaths {
        let Some(first) = sp.segments.first() else {
            continue;
        };
        let _ = write!(d, "M{} {}", num(first.p0.x), num(first.p0.y));
        for s in &sp.segments {
            let _ = write!(
                d,
                "C{} {} {} {} {} {}",
                num(s.p1.x),
                num(s.p1.y),
                num(s.p2.x),
                num(s.p2.y),
                num(s.p3.x),
                num(s.p3.y)
            );
        }
        if sp.closed {
            d.push('Z');
        }
    }
    d
}

/// Four decimals, no trailing zeros, no `-0`: stable, diffable golden files.
fn num(x: f64) -> String {
    let s = format!("{x:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_pads() {
        assert_eq!(super::base64(b""), "");
        assert_eq!(super::base64(b"f"), "Zg==");
        assert_eq!(super::base64(b"fo"), "Zm8=");
        assert_eq!(super::base64(b"foo"), "Zm9v");
        assert_eq!(super::base64(b"foobar"), "Zm9vYmFy");
    }
}
