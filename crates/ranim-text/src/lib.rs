//! Text and math for ranim: Typst layout into glyph paths, grouped into diffable tokens
//! (see `docs/SPEC.md` §6).
//!
//! [`math_tex`] and [`text`] typeset with the fonts bundled in `typst-assets`, so nothing needs
//! installing. Each visible glyph (and each rule, like a fraction bar) becomes one [`VState`],
//! filled white and centered on the origin.

use std::fmt;
use std::ops::Range;
use std::sync::OnceLock;

use ranim_core::color::{Color, WHITE};
use ranim_core::geom::{SubPath, line_segment};
use ranim_core::kurbo::{self, Affine, CubicBez, Point, Rect, Vec2};
use ranim_core::{FRAME_HEIGHT, FRAME_WIDTH, MobjectId, Scene, VPath, VState};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::layout::{Frame, FrameItem, Transform};
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook, TextItem};
use typst::utils::LazyHash;
use typst::visualize::{CurveItem, Geometry, Shape};
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

/// Height of one em in scene units (Typst's default 11pt text).
const EM: f64 = 0.7;
const PT: f64 = EM / 11.0;

/// What the diff compares: the glyph's text, plus its script level in math, so a superscript
/// `2` is not the same token as a baseline `2` (SPEC §6.2).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TokenKey {
    /// The characters, or `rule` for a fraction bar / radical overline.
    pub text: String,
    /// 0 for the baseline, 1 for scripts, 2 for scripts of scripts.
    pub level: u8,
}

impl fmt::Display for TokenKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)?;
        (0..self.level).try_for_each(|_| f.write_str("'"))
    }
}

/// A run of glyphs that diffs as one unit.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// What the diff compares.
    pub key: TokenKey,
    /// Indices into [`TextMobject::glyphs`].
    pub glyphs: Range<usize>,
}

/// Laid-out text: one shape per glyph, grouped into tokens.
#[derive(Debug, Clone, PartialEq)]
pub struct TextMobject {
    /// What was typeset.
    pub source: String,
    /// One filled shape per visible glyph or rule, in layout order.
    pub glyphs: Vec<VState>,
    /// Every glyph belongs to exactly one token; tokens are in glyph order.
    pub tokens: Vec<Token>,
}

/// Typesets Typst math, e.g. `"a^2 + b^2 = c^2"`. One token per glyph.
///
/// Panics with Typst's error message if `src` doesn't compile; see [`TextMobject::math`].
pub fn math_tex(src: &str) -> TextMobject {
    TextMobject::math(src).unwrap_or_else(|e| panic!("math_tex({src:?}): {e}"))
}

/// Typesets plain text (no markup). One token per word.
pub fn text(src: &str) -> TextMobject {
    TextMobject::text(src).unwrap_or_else(|e| panic!("text({src:?}): {e}"))
}

impl TextMobject {
    /// Typesets Typst math; `Err` holds Typst's error messages.
    pub fn math(src: &str) -> Result<Self, String> {
        Self::typeset(src, &format!("$ {src} $"), Tokens::Glyphs)
    }

    /// Typesets plain text; `Err` holds Typst's error messages.
    pub fn text(src: &str) -> Result<Self, String> {
        let escaped = src.replace('\\', "\\\\").replace('"', "\\\"");
        Self::typeset(src, &format!("#\"{escaped}\""), Tokens::Words)
    }

    fn typeset(source: &str, body: &str, mode: Tokens) -> Result<Self, String> {
        let page = compile(body)?;
        let mut runs = Vec::new();
        walk(&page, Affine::IDENTITY, &mut runs);

        // Typst is y-down in points; scenes are y-up in units, centered.
        let mut to_scene = Affine::scale_non_uniform(PT, -PT);
        let bbox = runs
            .iter()
            .filter_map(|r| r.path.as_ref()?.transform(to_scene).bbox())
            .reduce(|a, b| a.union(b));
        if let Some(b) = bbox {
            to_scene = Affine::translate(-b.center().to_vec2()) * to_scene;
        }

        let mut glyphs = Vec::new();
        let mut tokens: Vec<Token> = Vec::new();
        let mut word_open = false;
        for r in runs {
            let Some(path) = r.path else {
                word_open = false;
                continue;
            };
            let i = glyphs.len();
            glyphs.push(
                VState::new(path.transform(to_scene))
                    .fill(WHITE)
                    .stroke(Color::TRANSPARENT, 0.0),
            );
            match (mode, tokens.last_mut()) {
                (Tokens::Words, Some(t)) if word_open && !r.new_item => {
                    t.key.text.push_str(&r.text);
                    t.glyphs.end = i + 1;
                }
                _ => tokens.push(Token {
                    key: TokenKey {
                        text: r.text,
                        level: if mode == Tokens::Glyphs { r.level } else { 0 },
                    },
                    glyphs: i..i + 1,
                }),
            }
            word_open = true;
        }
        Ok(Self {
            source: source.to_owned(),
            glyphs,
            tokens,
        })
    }

    /// Tight bounding box of all glyphs.
    pub fn bbox(&self) -> Option<Rect> {
        self.glyphs
            .iter()
            .filter_map(|g| g.path.bbox())
            .reduce(|a, b| a.union(b))
    }

    /// Applies an affine transform to every glyph.
    pub fn transform(mut self, a: Affine) -> Self {
        self.glyphs = self.glyphs.into_iter().map(|g| g.transform(a)).collect();
        self
    }

    /// Translates by `v`.
    pub fn shift(self, v: Vec2) -> Self {
        self.transform(Affine::translate(v))
    }

    /// Moves so the bounding-box center is at `p`.
    pub fn move_to(self, p: Point) -> Self {
        let c = self.bbox().map_or(Point::ORIGIN, |b| b.center());
        self.shift(p - c)
    }

    /// Scales about the bounding-box center.
    pub fn scale(self, factor: f64) -> Self {
        let c = self.bbox().map_or(Point::ORIGIN, |b| b.center());
        self.transform(Affine::scale_about(factor, c))
    }

    /// Moves against the frame edge in direction `dir` (e.g. `UP`), leaving manim's 0.5-unit gap.
    pub fn to_edge(self, dir: Vec2) -> Self {
        const BUFF: f64 = 0.5;
        let Some(b) = self.bbox() else { return self };
        let (hw, hh) = (FRAME_WIDTH / 2.0 - BUFF, FRAME_HEIGHT / 2.0 - BUFF);
        let dx = match dir.x.total_cmp(&0.0) {
            std::cmp::Ordering::Greater => hw - b.x1,
            std::cmp::Ordering::Less => -hw - b.x0,
            std::cmp::Ordering::Equal => 0.0,
        };
        let dy = match dir.y.total_cmp(&0.0) {
            std::cmp::Ordering::Greater => hh - b.y1,
            std::cmp::Ordering::Less => -hh - b.y0,
            std::cmp::Ordering::Equal => 0.0,
        };
        self.shift(Vec2::new(dx, dy))
    }

    /// Sets every glyph's fill color.
    pub fn fill(mut self, color: Color) -> Self {
        self.glyphs = self.glyphs.into_iter().map(|g| g.fill(color)).collect();
        self
    }

    /// Adds every glyph to the scene; the ids are in glyph order.
    pub fn add_to(&self, s: &mut Scene) -> Vec<MobjectId> {
        self.glyphs.iter().map(|g| s.add(g.clone())).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tokens {
    Glyphs,
    Words,
}

/// One glyph or rule in layout order. `path` is `None` for invisible glyphs (spaces).
struct Run {
    text: String,
    level: u8,
    path: Option<VPath>,
    /// First glyph of a new text item: words never span items.
    new_item: bool,
}

fn walk(frame: &Frame, at: Affine, out: &mut Vec<Run>) {
    for (pos, item) in frame.items() {
        let here = at * Affine::translate((pos.x.to_pt(), pos.y.to_pt()));
        match item {
            FrameItem::Group(g) => walk(&g.frame, here * affine(g.transform), out),
            FrameItem::Text(t) => text_item(t, here, out),
            FrameItem::Shape(s, _) => out.push(Run {
                text: "rule".into(),
                level: 0,
                path: shape(s).map(|p| p.transform(here)),
                new_item: true,
            }),
            FrameItem::Image(..) | FrameItem::Link(..) | FrameItem::Tag(_) => {}
        }
    }
}

fn affine(t: Transform) -> Affine {
    Affine::new([
        t.sx.get(),
        t.ky.get(),
        t.kx.get(),
        t.sy.get(),
        t.tx.to_pt(),
        t.ty.to_pt(),
    ])
}

fn text_item(t: &TextItem, at: Affine, out: &mut Vec<Run>) {
    let size = t.size.to_pt();
    // Typst sets scripts at 70% and scripts-of-scripts at 50% of 11pt.
    let level = match size / 11.0 {
        s if s > 0.85 => 0,
        s if s > 0.6 => 1,
        _ => 2,
    };
    let units = size / t.font.units_per_em();
    let mut x = 0.0;
    for (i, g) in t.glyphs.iter().enumerate() {
        let origin = Vec2::new(
            x + g.x_offset.at(t.size).to_pt(),
            -g.y_offset.at(t.size).to_pt(),
        );
        x += g.x_advance.at(t.size).to_pt();
        let mut b = Outline::default();
        t.font
            .ttf()
            .outline_glyph(ttf_parser::GlyphId(g.id), &mut b);
        let place = at * Affine::translate(origin) * Affine::scale_non_uniform(units, -units);
        let path = b.finish();
        out.push(Run {
            text: t.text[g.range()].to_owned(),
            level,
            path: (!path.subpaths.is_empty()).then(|| path.transform(place)),
            new_item: i == 0,
        });
    }
}

/// Rules and boxes as filled outlines, in the shape's own coordinates.
fn shape(s: &Shape) -> Option<VPath> {
    let thickness = s.stroke.as_ref().map_or(0.0, |st| st.thickness.to_pt());
    match &s.geometry {
        Geometry::Line(to) => {
            let d = Vec2::new(to.x.to_pt(), to.y.to_pt());
            let n = Vec2::new(-d.y, d.x).normalize() * (thickness / 2.0);
            let p = Point::ORIGIN;
            Some(VPath::polyline(&[p + n, p + d + n, p + d - n, p - n], true))
        }
        Geometry::Rect(size) => {
            let (w, h) = (size.x.to_pt(), size.y.to_pt());
            let pts = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].map(Point::from);
            Some(VPath::polyline(&pts, true))
        }
        // ponytail: curves are filled even when Typst only strokes them; outline strokes
        // with kurbo if a stroked curve ever shows up in math.
        Geometry::Curve(c) => {
            let mut b = Outline::default();
            let pt = |p: &typst::layout::Point| (p.x.to_pt() as f32, p.y.to_pt() as f32);
            for item in c.0.iter() {
                use ttf_parser::OutlineBuilder;
                match item {
                    CurveItem::Move(p) => b.move_to(pt(p).0, pt(p).1),
                    CurveItem::Line(p) => b.line_to(pt(p).0, pt(p).1),
                    CurveItem::Cubic(p1, p2, p) => {
                        b.curve_to(pt(p1).0, pt(p1).1, pt(p2).0, pt(p2).1, pt(p).0, pt(p).1)
                    }
                    CurveItem::Close => b.close(),
                }
            }
            Some(b.finish()).filter(|p| !p.subpaths.is_empty())
        }
    }
}

/// Collects a glyph outline as cubics (quadratics are degree-elevated).
#[derive(Default)]
struct Outline {
    path: VPath,
    current: Vec<CubicBez>,
    start: Point,
    last: Point,
}

impl Outline {
    fn flush(&mut self, closed: bool) {
        if closed && self.last != self.start {
            self.current.push(line_segment(self.last, self.start));
        }
        if !self.current.is_empty() {
            self.path.subpaths.push(SubPath {
                segments: std::mem::take(&mut self.current),
                closed,
            });
        }
    }

    fn finish(mut self) -> VPath {
        self.flush(false);
        self.path
    }
}

fn pt(x: f32, y: f32) -> Point {
    Point::new(x.into(), y.into())
}

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.flush(false);
        self.start = pt(x, y);
        self.last = self.start;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = pt(x, y);
        self.current.push(line_segment(self.last, p));
        self.last = p;
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let q = kurbo::QuadBez::new(self.last, pt(x1, y1), pt(x, y));
        self.current.push(q.raise());
        self.last = q.p2;
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let p = pt(x, y);
        self.current
            .push(CubicBez::new(self.last, pt(x1, y1), pt(x2, y2), p));
        self.last = p;
    }
    fn close(&mut self) {
        self.flush(true);
        self.last = self.start;
    }
}

/// Fonts and the standard library, built once.
struct Shared {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| {
        let fonts: Vec<Font> = typst_assets::fonts()
            .flat_map(|data| Font::iter(Bytes::new(data)))
            .collect();
        Shared {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(FontBook::from_fonts(&fonts)),
            fonts,
        }
    })
}

/// A single in-memory file, with no access to the filesystem, packages or the clock.
struct Doc {
    source: Source,
}

impl World for Doc {
    fn library(&self) -> &LazyHash<Library> {
        &shared().library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &shared().book
    }
    fn main(&self) -> FileId {
        self.source.id()
    }
    fn source(&self, id: FileId) -> typst::diag::FileResult<Source> {
        if id == self.source.id() {
            Ok(self.source.clone())
        } else {
            Err(typst::diag::FileError::AccessDenied)
        }
    }
    fn file(&self, _: FileId) -> typst::diag::FileResult<Bytes> {
        Err(typst::diag::FileError::AccessDenied)
    }
    fn font(&self, index: usize) -> Option<Font> {
        shared().fonts.get(index).cloned()
    }
    fn today(&self, _: Option<Duration>) -> Option<Datetime> {
        None
    }
}

/// Compiles `body` onto one auto-sized page and returns its frame.
fn compile(body: &str) -> Result<Frame, String> {
    let doc = Doc {
        source: Source::detached(format!(
            "#set page(width: auto, height: auto, margin: 0pt, fill: none)\n{body}"
        )),
    };
    let out: PagedDocument = typst::compile(&doc).output.map_err(|errs| {
        errs.iter()
            .map(|e| e.message.as_str())
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    Ok(out.pages()[0].frame.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(t: &TextMobject) -> Vec<String> {
        t.tokens.iter().map(|t| t.key.to_string()).collect()
    }

    #[test]
    fn math_tokens_carry_script_level() {
        let t = math_tex("a^2 + b^2 = c^2");
        // Math letters are the Unicode math italics Typst renders them as.
        assert_eq!(keys(&t), ["𝑎", "2'", "+", "𝑏", "2'", "=", "𝑐", "2'"]);
        assert_eq!(t.glyphs.len(), 8);
        let b = t.bbox().unwrap();
        assert!(b.center().to_vec2().hypot() < 1e-9, "centered: {b:?}");
        assert!((0.3..1.2).contains(&b.height()), "height {}", b.height());
        // Scripts are raised.
        let (a, two) = (
            t.glyphs[0].path.bbox().unwrap(),
            t.glyphs[1].path.bbox().unwrap(),
        );
        assert!(two.y0 > a.y0 + 0.1, "{two:?} vs {a:?}");
    }

    #[test]
    fn fractions_have_a_rule() {
        let t = math_tex("1/2");
        // Frame order: rules come after the glyphs around them.
        assert_eq!(keys(&t), ["1", "2", "rule"]);
    }

    #[test]
    fn text_tokens_are_words() {
        let t = text("Hello, \"big\" world");
        assert_eq!(keys(&t), ["Hello,", "\"big\"", "world"]);
        assert_eq!(t.tokens.last().unwrap().glyphs.len(), 5);
        assert_eq!(t.tokens.last().unwrap().glyphs.end, t.glyphs.len());
    }

    #[test]
    fn errors_are_reported() {
        assert!(TextMobject::math("#nope").is_err());
    }

    #[test]
    fn to_edge() {
        let t = text("Title").to_edge(ranim_core::UP);
        assert!((t.bbox().unwrap().y1 - (FRAME_HEIGHT / 2.0 - 0.5)).abs() < 1e-9);
    }
}
