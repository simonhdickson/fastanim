//! Export formats, and where an export goes: `-o` if given, else a path named after the scene.

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// What an export writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// H.264 video.
    Mp4,
    /// VP9 video, with alpha.
    Webm,
    /// An animated GIF.
    Gif,
    /// PNG frames: a folder of them, or one file for a still.
    Png,
    /// SVG frames: a folder of them, or one file for a still.
    Svg,
    /// One SVG file that plays the frames.
    AnimatedSvg,
}

impl Format {
    /// Every format, as named on the command line.
    pub const ALL: [Format; 6] = [
        Format::Mp4,
        Format::Webm,
        Format::Gif,
        Format::Png,
        Format::Svg,
        Format::AnimatedSvg,
    ];

    /// The file extension it writes.
    pub fn ext(self) -> &'static str {
        match self {
            Format::Mp4 => "mp4",
            Format::Webm => "webm",
            Format::Gif => "gif",
            Format::Png => "png",
            Format::Svg | Format::AnimatedSvg => "svg",
        }
    }

    /// Whether more than one frame goes to a folder of numbered files.
    pub fn frames(self) -> bool {
        matches!(self, Format::Png | Format::Svg)
    }

    /// The format `path`'s extension names, if any; `.svg` is SVG frames.
    pub fn from_path(path: &Path) -> Option<Format> {
        let ext = path.extension()?.to_str()?;
        Format::ALL.into_iter().find(|f| f.ext() == ext)
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Format::AnimatedSvg => "animated-svg",
            other => other.ext(),
        })
    }
}

impl FromStr for Format {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        (Format::ALL.into_iter().find(|f| f.to_string() == s)).ok_or_else(|| {
            let names: Vec<_> = Format::ALL.iter().map(Format::to_string).collect();
            format!("unknown format `{s}`: use {}", names.join(", "))
        })
    }
}

/// The format and path of an export of the scene `name`. The format is `format`, else the one
/// `output`'s extension names, else mp4 (png for a `still`), and an extension that names none
/// needs `format`; the path is `output`, else `name` for a folder of frames and `name.ext` for
/// a file.
pub fn output(
    name: &str,
    format: Option<Format>,
    output: Option<PathBuf>,
    still: bool,
) -> Result<(Format, PathBuf), String> {
    let shown = || {
        output
            .as_deref()
            .unwrap_or(Path::new(""))
            .display()
            .to_string()
    };
    let named = output.as_deref().and_then(Format::from_path);
    let unknown = output.as_deref().is_some_and(|o| o.extension().is_some()) && named.is_none();
    let format = match (format, named) {
        (Some(f), Some(n)) if f.ext() != n.ext() => {
            return Err(format!("`{}` isn't {f}", shown()));
        }
        (Some(f), _) => f,
        (None, Some(n)) => n,
        (None, None) if unknown => {
            return Err(format!(
                "unsupported output `{}`: name a format, mp4, webm, gif, png, svg or animated-svg",
                shown()
            ));
        }
        (None, None) if still => Format::Png,
        (None, None) => Format::Mp4,
    };
    if still && !matches!(format, Format::Png | Format::Svg) {
        return Err(format!("a still is png or svg, not {format}"));
    }
    let path = output.unwrap_or_else(|| {
        if format.frames() && !still {
            PathBuf::from(name)
        } else {
            PathBuf::from(format!("{name}.{}", format.ext()))
        }
    });
    Ok((format, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(
        format: Option<&str>,
        path: Option<&str>,
        still: bool,
    ) -> Result<(Format, PathBuf), String> {
        let format = format.map(|f| f.parse().unwrap());
        output("scene", format, path.map(PathBuf::from), still)
    }

    #[test]
    fn names() {
        for f in Format::ALL {
            assert_eq!(f.to_string().parse(), Ok(f));
        }
        assert!("svgz".parse::<Format>().is_err());
    }

    #[test]
    fn defaults_follow_the_scene_name() {
        let ok = |f, o, still| out(f, o, still).unwrap();
        assert_eq!(ok(None, None, false), (Format::Mp4, "scene.mp4".into()));
        assert_eq!(
            ok(Some("gif"), None, false),
            (Format::Gif, "scene.gif".into())
        );
        assert_eq!(ok(Some("png"), None, false), (Format::Png, "scene".into()));
        assert_eq!(ok(Some("svg"), None, false), (Format::Svg, "scene".into()));
        assert_eq!(
            ok(Some("animated-svg"), None, false),
            (Format::AnimatedSvg, "scene.svg".into())
        );
        assert_eq!(ok(None, None, true), (Format::Png, "scene.png".into()));
        assert_eq!(
            ok(Some("svg"), None, true),
            (Format::Svg, "scene.svg".into())
        );
    }

    #[test]
    fn output_overrides_and_names_the_format() {
        let ok = |f, o, still| out(f, o, still).unwrap();
        assert_eq!(
            ok(None, Some("x.webm"), false),
            (Format::Webm, "x.webm".into())
        );
        assert_eq!(
            ok(None, Some("f.svg"), false),
            (Format::Svg, "f.svg".into())
        );
        assert_eq!(
            ok(Some("png"), Some("frames"), false),
            (Format::Png, "frames".into())
        );
        assert_eq!(
            ok(Some("animated-svg"), Some("a.svg"), false),
            (Format::AnimatedSvg, "a.svg".into())
        );
        assert_eq!(
            ok(None, Some("dir/out"), false),
            (Format::Mp4, "dir/out".into())
        );
        assert!(out(Some("gif"), Some("x.mp4"), false).is_err());
        assert!(out(None, Some("x.avi"), false).is_err());
        assert_eq!(
            ok(Some("mp4"), Some("x.avi"), false),
            (Format::Mp4, "x.avi".into())
        );
        assert!(out(Some("mp4"), None, true).is_err());
        assert!(out(None, Some("x.gif"), true).is_err());
    }
}
