//! SVG still export, straight from a [`SceneState`] (no GPU).

use std::fmt::Write;

use crate::color::Color;
use crate::geom::VPath;
use crate::mobject::SceneState;
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
    for (_, m) in order {
        let d = path_data(&m.path.trim(m.draw_range.clone()));
        if m.opacity <= 0.0 || d.is_empty() {
            continue;
        }
        let _ = writeln!(
            out,
            r#"<path d="{d}" fill="{}" fill-opacity="{}" stroke="{}" stroke-opacity="{}" stroke-width="{}" stroke-linejoin="round" stroke-linecap="round" opacity="{}"/>"#,
            m.fill.to_hex(),
            num(m.fill.a.into()),
            m.stroke.color.to_hex(),
            num(m.stroke.color.a.into()),
            num(m.stroke.width),
            num(m.opacity.into()),
        );
    }
    out.push_str("</g>\n</svg>\n");
    out
}

fn path_data(p: &VPath) -> String {
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
