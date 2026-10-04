//! Pre-typeset text (SPEC §14.5): the typesetting cache written out natively by
//! `fastanim run --bundle` and read back by the web player, so published scenes never run Typst
//! in the browser.
//!
//! The format is whitespace-separated text. Strings are a byte length, one space, then the
//! bytes, so they can hold anything. Numbers use Rust's shortest round-trip form, so a bundled
//! result is bit-identical to typesetting it again:
//!
//! ```text
//! fastanim-bundle 1
//! <body> <source> <glyph count>
//!   per glyph: <r g b a> <subpath count>, per subpath: <closed 0|1> <segments> <x0 y0> <6 per segment>
//! <token count> per token: <level> <glyph start> <glyph end> <text>
//! <line count> per line: <token start> <token end>
//! ```

use std::fmt::Write;
use std::str::FromStr;

use fastanim_core::VPath;
use fastanim_core::color::Color;
use fastanim_core::geom::SubPath;
use fastanim_core::kurbo::{CubicBez, Point};

use crate::{TextMobject, Token, TokenKey, cache, glyph};

const MAGIC: &str = "fastanim-bundle 1";

/// Everything typeset so far in this process, as a bundle.
pub fn export_bundle() -> String {
    let cache = cache().lock().unwrap();
    let mut entries: Vec<_> = cache.iter().collect();
    entries.sort_by_key(|(body, _)| *body);
    let mut out = format!("{MAGIC}\n");
    for (body, t) in entries {
        write_entry(&mut out, body, t);
    }
    out
}

/// Fills the typesetting cache from a bundle; returns how many snippets it held.
pub fn import_bundle(bundle: &str) -> Result<usize, String> {
    let entries = read(bundle)?;
    let n = entries.len();
    cache().lock().unwrap().extend(entries);
    Ok(n)
}

fn write_entry(out: &mut String, body: &str, t: &TextMobject) {
    let s = |out: &mut String, s: &str| {
        let _ = write!(out, "{} {s} ", s.len());
    };
    s(out, body);
    s(out, &t.source);
    let _ = writeln!(out, "{}", t.glyphs.len());
    for g in &t.glyphs {
        let Color { r, g: gr, b, a } = g.fill;
        let _ = write!(out, "{r} {gr} {b} {a} {}", g.path.subpaths.len());
        for sp in &g.path.subpaths {
            let _ = write!(out, " {} {}", u8::from(sp.closed), sp.segments.len());
            if let Some(first) = sp.segments.first() {
                let _ = write!(out, " {} {}", first.p0.x, first.p0.y);
            }
            for c in &sp.segments {
                for p in [c.p1, c.p2, c.p3] {
                    let _ = write!(out, " {} {}", p.x, p.y);
                }
            }
        }
        out.push('\n');
    }
    let _ = write!(out, "{}", t.tokens.len());
    for tok in &t.tokens {
        let _ = write!(
            out,
            "\n{} {} {} ",
            tok.key.level, tok.glyphs.start, tok.glyphs.end
        );
        s(out, &tok.key.text);
    }
    let _ = write!(out, "\n{}", t.lines.len());
    for l in &t.lines {
        let _ = write!(out, " {} {}", l.start, l.end);
    }
    out.push('\n');
}

fn read(bundle: &str) -> Result<Vec<(String, TextMobject)>, String> {
    let rest =
        (bundle.strip_prefix(MAGIC)).ok_or("not a fastanim bundle (or from another version)")?;
    let mut r = Reader { s: rest };
    let mut out = Vec::new();
    while !r.done() {
        let body = r.string()?;
        let source = r.string()?;
        let glyphs = (0..r.num::<usize>()?)
            .map(|_| {
                let fill = Color::rgba(r.num()?, r.num()?, r.num()?, r.num()?);
                let subpaths = (0..r.num::<usize>()?)
                    .map(|_| r.subpath())
                    .collect::<Result<_, _>>()?;
                Ok(glyph(VPath { subpaths }, fill))
            })
            .collect::<Result<_, String>>()?;
        let tokens = (0..r.num::<usize>()?)
            .map(|_| {
                let level = r.num()?;
                let glyphs = r.num()?..r.num()?;
                let text = r.string()?;
                Ok(Token {
                    key: TokenKey { text, level },
                    glyphs,
                })
            })
            .collect::<Result<_, String>>()?;
        let lines = (0..r.num::<usize>()?)
            .map(|_| Ok(r.num()?..r.num()?))
            .collect::<Result<_, String>>()?;
        out.push((
            body,
            TextMobject {
                source,
                glyphs,
                tokens,
                lines,
            },
        ));
    }
    Ok(out)
}

struct Reader<'a> {
    s: &'a str,
}

impl Reader<'_> {
    fn done(&mut self) -> bool {
        self.s = self.s.trim_start();
        self.s.is_empty()
    }

    fn word(&mut self) -> Result<&str, String> {
        if self.done() {
            return Err("bundle ends early".into());
        }
        let end = self.s.find(char::is_whitespace).unwrap_or(self.s.len());
        let (w, rest) = self.s.split_at(end);
        self.s = rest;
        Ok(w)
    }

    fn num<T: FromStr>(&mut self) -> Result<T, String> {
        let w = self.word()?;
        w.parse().map_err(|_| format!("bad number {w:?} in bundle"))
    }

    fn string(&mut self) -> Result<String, String> {
        let len: usize = self.num()?;
        let s = (self.s.strip_prefix(' ')).and_then(|s| Some((s.get(..len)?, s.get(len..)?)));
        let (s, rest) = s.ok_or("bad string in bundle")?;
        self.s = rest;
        Ok(s.to_owned())
    }

    fn point(&mut self) -> Result<Point, String> {
        Ok(Point::new(self.num()?, self.num()?))
    }

    fn subpath(&mut self) -> Result<SubPath, String> {
        let closed = self.num::<u8>()? == 1;
        let n: usize = self.num()?;
        let mut last = if n > 0 { self.point()? } else { Point::ORIGIN };
        let segments = (0..n)
            .map(|_| {
                let c = CubicBez::new(last, self.point()?, self.point()?, self.point()?);
                last = c.p3;
                Ok(c)
            })
            .collect::<Result<_, String>>()?;
        Ok(SubPath { segments, closed })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{code, math_tex, text};

    #[test]
    fn round_trips_exactly() {
        let ts = [
            ("m", math_tex("sqrt(x^2) + 1/2")),
            ("t", text("Hello, wörld")),
            ("c", code("fn f() {\n    1\n}", "rust")),
        ];
        let mut out = format!("{MAGIC}\n");
        for (body, t) in &ts {
            write_entry(&mut out, body, t);
        }
        let back = read(&out).unwrap();
        assert_eq!(back.len(), ts.len());
        for ((body, t), (b, u)) in ts.iter().zip(&back) {
            assert_eq!(body, b);
            assert_eq!(t, u, "{body}");
        }
    }

    #[test]
    fn strings_hold_anything() {
        let t = TextMobject {
            source: "two\nlines and  spaces ".into(),
            glyphs: Vec::new(),
            tokens: vec![Token {
                key: TokenKey {
                    text: " 3 ".into(),
                    level: 2,
                },
                glyphs: 0..0,
            }],
            lines: Vec::new(),
        };
        let mut out = format!("{MAGIC}\n");
        write_entry(&mut out, "#raw(\"x\")\n", &t);
        assert_eq!(read(&out).unwrap(), [("#raw(\"x\")\n".to_owned(), t)]);
    }

    #[test]
    fn bad_bundles_are_errors() {
        assert!(read("nope").is_err());
        assert!(read(&format!("{MAGIC}\n5 ab")).is_err());
        assert!(read(&format!("{MAGIC}\n1 a 1 b x")).is_err());
    }

    #[test]
    fn imports_fill_the_cache() {
        let body = "#\"bundled only\"";
        let t = TextMobject {
            source: "bundled only".into(),
            glyphs: Vec::new(),
            tokens: Vec::new(),
            lines: Vec::new(),
        };
        let mut out = format!("{MAGIC}\n");
        write_entry(&mut out, body, &t);
        assert_eq!(import_bundle(&out), Ok(1));
        // The real typesetter would have produced glyphs; the bundle's answer wins.
        assert_eq!(text("bundled only"), t);
        assert!(export_bundle().contains("bundled only"));
    }
}
