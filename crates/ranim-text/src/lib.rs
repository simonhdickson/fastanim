//! Text and math for ranim: Typst layout into glyph paths, grouped into diffable tokens
//! (see `docs/SPEC.md` §6).
//!
//! [`math_tex`], [`text`] and [`code`] typeset with the fonts bundled in `typst-assets`, so
//! nothing needs installing. Each visible glyph (and each rule, like a fraction bar) becomes one
//! [`VState`], filled white (or by syntax highlighting) and centered on the origin. [`list`]
//! lays out boxed values for algorithm animations.

use std::collections::HashMap;
use std::fmt;
use std::ops::Range;
use std::sync::{Mutex, OnceLock};

use ranim_core::color::{Color, WHITE};
use ranim_core::geom::{SubPath, line_segment};
use ranim_core::kurbo::{self, Affine, CubicBez, Point, Rect, Vec2};
use ranim_core::{Group, Scene, TransformDiff, VPath, VState};
use ranim_core::{Interpolate, Layout, Op};
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
    /// Token ranges per line, for diffing line by line first (SPEC §5.6). Empty means one
    /// line; only [`code`] has several.
    pub lines: Vec<Range<usize>>,
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

/// Typesets source code highlighted as `lang` (any language Typst's `raw` knows, e.g.
/// `"rust"`). One token per identifier, number or punctuation character, grouped into lines.
///
/// Panics with Typst's error message if it doesn't compile; see [`TextMobject::code`].
pub fn code(src: &str, lang: &str) -> TextMobject {
    TextMobject::code(src, lang).unwrap_or_else(|e| panic!("code({src:?}): {e}"))
}

/// Side of one [`list`] cell, in scene units.
pub const CELL: f64 = 1.0;

/// Boxed values in a row, centered on the origin: the `ListMobject` of SPEC §7.1. Each token
/// is one cell, its box then its text, keyed by the text. Animate with [`transform_list`].
pub fn list<T: fmt::Display>(values: &[T]) -> TextMobject {
    let mid = (values.len() as f64 - 1.0) / 2.0;
    let (mut glyphs, mut tokens) = (Vec::new(), Vec::new());
    for (i, v) in values.iter().enumerate() {
        let at = Vec2::new((i as f64 - mid) * CELL, 0.0);
        let label = text(&v.to_string());
        let start = glyphs.len();
        glyphs.push(VState::square(CELL * 0.9).shift(at));
        glyphs.extend(label.glyphs.into_iter().map(|g| g.shift(at)));
        tokens.push(Token {
            key: TokenKey {
                text: label.source,
                level: 0,
            },
            glyphs: start..glyphs.len(),
        });
    }
    TextMobject {
        source: (tokens.iter().map(|t| t.key.text.as_str()))
            .collect::<Vec<_>>()
            .join(", "),
        glyphs,
        tokens,
        lines: Vec::new(),
    }
}

static CLOCK: OnceLock<fn() -> f64> = OnceLock::new();
static TYPESET_MS: Mutex<f64> = Mutex::new(0.0);

/// Starts timing typesetting with `now`, a clock in milliseconds (`std::time` has none on
/// `wasm32-unknown-unknown`). Only the first call takes effect.
pub fn time_typesetting(now: fn() -> f64) {
    let _ = CLOCK.set(now);
}

/// Total milliseconds spent typesetting (cache misses only) since [`time_typesetting`].
pub fn typeset_ms() -> f64 {
    *TYPESET_MS.lock().unwrap()
}

impl TextMobject {
    /// Typesets Typst math; `Err` holds Typst's error messages.
    pub fn math(src: &str) -> Result<Self, String> {
        Self::typeset(src, &format!("$ {src} $"), Tokens::Glyphs)
    }

    /// Typesets plain text; `Err` holds Typst's error messages.
    pub fn text(src: &str) -> Result<Self, String> {
        Self::typeset(src, &format!("#{}", typst_str(src)), Tokens::Words)
    }

    /// Typesets highlighted source code; `Err` holds Typst's error messages.
    pub fn code(src: &str, lang: &str) -> Result<Self, String> {
        let body = format!(
            "#raw(block: true, lang: {}, {})",
            typst_str(lang),
            typst_str(src)
        );
        Self::typeset(src, &body, Tokens::Code)
    }

    /// Typesetting is a pure function of the Typst body, which encodes kind, source and
    /// language, so results are memoized on it (SPEC §14.5): re-running a script only
    /// typesets the snippets it changed.
    fn typeset(source: &str, body: &str, mode: Tokens) -> Result<Self, String> {
        // ponytail: unbounded and never evicted; fine for a session's worth of snippets, add
        // an LRU if a long-lived playground grows it too far.
        static CACHE: OnceLock<Mutex<HashMap<String, TextMobject>>> = OnceLock::new();
        let cache = CACHE.get_or_init(Mutex::default);
        if let Some(t) = cache.lock().unwrap().get(body) {
            return Ok(t.clone());
        }
        let start = CLOCK.get().map(|now| now());
        let t = Self::typeset_uncached(source, body, mode);
        if let (Some(start), Some(now)) = (start, CLOCK.get()) {
            *TYPESET_MS.lock().unwrap() += now() - start;
        }
        let t = t?;
        cache.lock().unwrap().insert(body.to_owned(), t.clone());
        Ok(t)
    }

    fn typeset_uncached(source: &str, body: &str, mode: Tokens) -> Result<Self, String> {
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
        let mut lines = Vec::new();
        let (mut line_start, mut line_y) = (0, None);
        let mut word_open = false;
        for r in runs {
            let Some(path) = r.path else {
                word_open = false;
                continue;
            };
            // Code lines: the baseline moves down (Typst is y-down).
            if mode == Tokens::Code && line_y.is_some_and(|y| r.y > y + 1.0) {
                lines.push(line_start..tokens.len());
                line_start = tokens.len();
                word_open = false;
            }
            line_y = Some(r.y);
            let i = glyphs.len();
            let fill = match mode {
                // Highlighting themes are for light backgrounds: plain black text turns
                // white, and colors are lightened.
                Tokens::Code if r.fill == Color::rgb(0.0, 0.0, 0.0) => WHITE,
                Tokens::Code => Color::lerp(&r.fill, &WHITE, 0.35),
                _ => WHITE,
            };
            glyphs.push(
                VState::new(path.transform(to_scene))
                    .fill(fill)
                    .stroke(Color::TRANSPARENT, 0.0),
            );
            let ident = |s: &str| s.chars().all(|c| c.is_alphanumeric() || c == '_');
            let joins = |t: &Token| match mode {
                Tokens::Glyphs => false,
                Tokens::Words => !r.new_item,
                Tokens::Code => !r.new_item && ident(&t.key.text) && ident(&r.text),
            };
            match tokens.last_mut() {
                Some(t) if word_open && joins(t) => {
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
        if mode == Tokens::Code && line_start < tokens.len() {
            lines.push(line_start..tokens.len());
        }
        Ok(Self {
            source: source.to_owned(),
            glyphs,
            tokens,
            lines,
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

    /// Sets every glyph's fill color.
    pub fn fill(mut self, color: Color) -> Self {
        self.glyphs = self.glyphs.into_iter().map(|g| g.fill(color)).collect();
        self
    }

    /// Adds every glyph to the scene as a group of tokens; the ids are in glyph order.
    pub fn add_to(&self, s: &mut Scene) -> Group<TokenKey> {
        Group::add(s, &self.layout())
    }

    /// Glyphs, tokens and lines as a [`Layout`].
    pub fn layout(&self) -> Layout<TokenKey> {
        Layout {
            states: self.glyphs.clone(),
            parts: self.parts(),
            lines: self.lines.clone(),
        }
    }

    /// Tokens as [`Group`] parts.
    pub fn parts(&self) -> Vec<(TokenKey, Range<usize>)> {
        (self.tokens.iter())
            .map(|t| (t.key.clone(), t.glyphs.clone()))
            .collect()
    }
}

impl ranim_core::Position for TextMobject {
    fn bbox(&self) -> Option<Rect> {
        TextMobject::bbox(self)
    }
    fn transform(self, a: Affine) -> Self {
        TextMobject::transform(self, a)
    }
}

/// Morphs text already in the scene into `to`, diffing tokens (SPEC §7): unchanged tokens
/// slide, moved ones arc, and only real changes fade or morph. Code is diffed line by line
/// first, then token by token within changed lines. Updates `from` to the new text; play the
/// result next.
///
/// ```
/// use ranim_core::{AnimationExt, Scene};
/// use ranim_text::{math_tex, transform_diff};
///
/// let mut s = Scene::new();
/// let mut eq = math_tex("a^2 + b^2 = c^2").add_to(&mut s);
/// let d = transform_diff(&mut s, &mut eq, &math_tex("a^2 = c^2 - b^2"));
/// s.play(d.run_time(1.5));
/// ```
pub fn transform_diff(
    s: &mut Scene,
    from: &mut Group<TokenKey>,
    to: &TextMobject,
) -> TransformDiff {
    from.transform_diff(s, &to.layout(), operator_class)
}

/// [`transform_diff`] for [`list`]s: values that change cell travel along arcs rather than
/// slide, so swapping two values is two moves passing on opposite sides.
///
/// ```
/// use ranim_core::{Op, Scene};
/// use ranim_text::{list, transform_list};
///
/// let mut s = Scene::new();
/// let mut l = list(&[5, 3, 8]).add_to(&mut s);
/// let d = transform_list(&mut s, &mut l, &list(&[3, 5, 8]));
/// assert_eq!(d.ops().iter().filter(|op| matches!(op, Op::Move { .. })).count(), 2);
/// s.play(d);
/// ```
pub fn transform_list(
    s: &mut Scene,
    from: &mut Group<TokenKey>,
    to: &TextMobject,
) -> TransformDiff {
    let to = to.layout();
    let ops = (from.diff(s.state(), &to, |_| None::<()>).into_iter())
        .map(|op| match op {
            Op::Equal { a, b } if a != b => Op::Move { a, b },
            op => op,
        })
        .collect();
    from.transform_ops(s, &to, ops)
}

/// `s` as a Typst string literal.
fn typst_str(s: &str) -> String {
    let escaped = (s.replace('\\', "\\\\").replace('"', "\\\""))
        .replace('\r', "")
        .replace('\n', "\\n")
        .replace('\t', "\\t");
    format!("\"{escaped}\"")
}

/// Lets non-adjacent operators replace each other (`+` → `−`), and relations likewise.
fn operator_class(k: &TokenKey) -> Option<u8> {
    match k.text.as_str() {
        "+" | "-" | "−" | "±" | "∓" | "×" | "÷" | "·" | "∗" => Some(0),
        "=" | "≠" | "<" | ">" | "≤" | "≥" | "≈" | "≡" => Some(1),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tokens {
    Glyphs,
    Words,
    /// Identifier runs and single punctuation characters, in lines.
    Code,
}

/// One glyph or rule in layout order. `path` is `None` for invisible glyphs (spaces).
struct Run {
    text: String,
    level: u8,
    path: Option<VPath>,
    /// First glyph of a new text item: words never span items.
    new_item: bool,
    /// Text color.
    fill: Color,
    /// Baseline, in Typst's y-down points.
    y: f64,
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
                fill: WHITE,
                y: here.translation().y,
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
    let fill = match &t.fill {
        typst::visualize::Paint::Solid(c) => {
            let c = c.to_rgb();
            Color::rgba(c.red, c.green, c.blue, c.alpha)
        }
        _ => WHITE,
    };
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
            fill,
            y: place.translation().y,
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
    use ranim_core::{FRAME_HEIGHT, Position};

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
    fn typesetting_is_memoized() {
        use std::time::Instant;
        let src = "sum_(k=1)^n k^3 = (n(n+1)/2)^2";
        let t0 = Instant::now();
        let a = math_tex(src);
        let first = t0.elapsed();
        let t0 = Instant::now();
        let b = math_tex(src);
        let second = t0.elapsed();
        assert_eq!(a, b);
        assert!(second * 10 < first, "{second:?} vs {first:?}");
    }

    #[test]
    fn typesetting_is_timed() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static TICKS: AtomicU64 = AtomicU64::new(0);
        time_typesetting(|| TICKS.fetch_add(1, Ordering::Relaxed) as f64);
        let before = typeset_ms();
        math_tex("q_17 + w^9");
        assert!(typeset_ms() > before);
        let before = typeset_ms();
        math_tex("q_17 + w^9");
        assert_eq!(typeset_ms(), before, "cache hits aren't typesetting");
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
    fn code_tokens_and_lines() {
        let t = code("fn main() {\n    let x_1 = f(\"hi\");\n}", "rust");
        let lines: Vec<Vec<String>> = (t.lines.iter())
            .map(|l| {
                keys(&TextMobject {
                    tokens: t.tokens[l.clone()].to_vec(),
                    ..t.clone()
                })
            })
            .collect();
        assert_eq!(
            lines,
            [
                vec!["fn", "main", "(", ")", "{"],
                vec!["let", "x_1", "=", "f", "(", "\"", "hi", "\"", ")", ";"],
                vec!["}"],
            ]
        );
        // Highlighted: keywords differ from plain punctuation, which is white.
        assert_ne!(t.glyphs[0].fill, WHITE);
        assert_eq!(t.glyphs[t.tokens[2].glyphs.start].fill, WHITE);
    }

    #[test]
    fn list_cells() {
        let l = list(&[5, 12]);
        assert_eq!(keys(&l), ["5", "12"]);
        assert_eq!(l.tokens[1].glyphs, 2..5, "box then two digits");
        assert!((l.glyphs[0].path.center().x + CELL / 2.0).abs() < 1e-9);
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
