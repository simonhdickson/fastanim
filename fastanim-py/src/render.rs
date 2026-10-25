//! Exporting and previewing baked scenes (SPEC §15.2). Every build writes SVG stills and SVG
//! frame sequences; video, PNG and the preview window need the `preview` feature, on by default
//! and off in the Pyodide wheel, and go through `fastanim-bevy`.

use std::path::{Path, PathBuf};

use fastanim_core::BakedTimeline;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

use crate::scene_err;

/// Output resolution and frame rate, from a preset name.
#[derive(Debug, Clone, Copy)]
// Only video and PNG, which need the `preview` feature, use the size.
#[cfg_attr(not(feature = "preview"), allow(dead_code))]
pub(crate) struct Quality {
    width: u32,
    height: u32,
    fps: u32,
}

impl Quality {
    /// Parses a preset: `480p15`, `720p30`, `1080p60` or `4k60`.
    pub(crate) fn parse(s: &str) -> PyResult<Self> {
        let (width, height, fps) = match s {
            "480p15" => (854, 480, 15),
            "720p30" => (1280, 720, 30),
            "1080p60" => (1920, 1080, 60),
            "4k60" => (3840, 2160, 60),
            _ => {
                return Err(PyValueError::new_err(format!(
                    "unknown quality `{s}` (480p15, 720p30, 1080p60, 4k60)"
                )));
            }
        };
        Ok(Self { width, height, fps })
    }

    /// The time of frame `n`.
    pub(crate) fn frame_time(self, n: u32) -> f32 {
        n as f32 / self.fps as f32
    }
}

/// The whole scene, or from marker `section` to the next marker (or the end).
pub(crate) fn section(tl: &BakedTimeline, section: Option<&str>) -> PyResult<(f32, f32)> {
    let Some(name) = section else {
        return Ok((0.0, tl.duration()));
    };
    let start = (tl.marker(name)).ok_or_else(|| scene_err(format!("no marker `{name}`")))?;
    let end = (tl.markers().iter().map(|&(_, t)| t))
        .filter(|&t| t > start)
        .reduce(f32::min)
        .unwrap_or(tl.duration());
    Ok((start, end))
}

/// Renders `tl` from `range.0` to `range.1` and writes it to `output`, whose extension picks the
/// format: `mp4`, `webm`, `gif`, `png` or `svg`. A single frame of `png` or `svg` is written to
/// `output` itself, more to numbered files in the directory `output`, or with `animated` to one
/// SVG that plays them.
pub(crate) fn export(
    py: Python<'_>,
    tl: &BakedTimeline,
    output: PathBuf,
    quality: Quality,
    range: (f32, f32),
    animated: bool,
) -> PyResult<()> {
    let ext = (output.extension().and_then(|e| e.to_str())).unwrap_or_default();
    if !["mp4", "webm", "gif", "png", "svg"].contains(&ext) {
        return Err(PyValueError::new_err(format!(
            "unsupported output `{}`: use mp4, webm, gif, png or svg",
            output.display()
        )));
    }
    if animated && ext != "svg" {
        return Err(PyValueError::new_err(format!(
            "only svg can be animated, not `{}`",
            output.display()
        )));
    }
    // Detached: export evaluates frames on other threads, and updaters there take the GIL.
    #[cfg(feature = "preview")]
    {
        let ex = fastanim_bevy::export::Export {
            quality: fastanim_bevy::export::Quality {
                width: quality.width,
                height: quality.height,
                fps: quality.fps,
            },
            output,
            range,
            animated,
            // Python updaters run one at a time under the GIL, and more threads only fight
            // over it: on bench/epicycles one is ~20% faster than sixteen. Free-threaded
            // Python (3.14t) without the GIL runs them on every core.
            workers: (tl.has_updaters() && gil_enabled(py)).then_some(1),
            cpu: false,
        };
        py.detach(|| fastanim_bevy::export::export(tl, &ex))
            .map_err(PyRuntimeError::new_err)
    }
    #[cfg(not(feature = "preview"))]
    {
        if ext != "svg" {
            return Err(needs_preview());
        }
        let times = frame_times(range, quality.fps);
        let fps = animated.then_some(quality.fps);
        py.detach(|| write_svgs(tl, &times, &output, fps))
            .map_err(|e| PyRuntimeError::new_err(format!("{}: {e}", output.display())))
    }
}

/// Whether this interpreter runs with the GIL: always before 3.13, and on a free-threaded
/// build unless it was turned back on (`PYTHON_GIL=1`, or a module that needs it).
#[cfg(feature = "preview")]
fn gil_enabled(py: Python<'_>) -> bool {
    let is_enabled = py.import("sys").and_then(|s| s.getattr("_is_gil_enabled"));
    is_enabled
        .and_then(|f| f.call0()?.extract())
        .unwrap_or(true)
}

/// Times of the frames covering `range` at `fps`, on the global frame grid.
#[cfg(not(feature = "preview"))]
fn frame_times((start, end): (f32, f32), fps: u32) -> Vec<f32> {
    let first = (start * fps as f32).round() as u32;
    let last = (end * fps as f32).round() as u32;
    (first..=last).map(|i| i as f32 / fps as f32).collect()
}

/// As `fastanim-bevy` writes SVG: one file, one animated file at `animated` fps, or a directory
/// of `0000.svg`, `0001.svg`, ….
#[cfg(not(feature = "preview"))]
fn write_svgs(
    tl: &BakedTimeline,
    times: &[f32],
    out: &Path,
    animated: Option<u32>,
) -> std::io::Result<()> {
    let svg = |t| fastanim_core::to_svg(&tl.eval(t), tl.background());
    if let [t] = times {
        return std::fs::write(out, svg(*t));
    }
    if let Some(fps) = animated {
        let frames: Vec<_> = times.iter().map(|&t| tl.eval(t)).collect();
        let svg = fastanim_core::to_animated_svg(&frames, fps, tl.background());
        return std::fs::write(out, svg);
    }
    std::fs::create_dir_all(out)?;
    for (i, &t) in times.iter().enumerate() {
        std::fs::write(out.join(format!("{i:04}.svg")), svg(t))?;
    }
    Ok(())
}

/// What a command that renders through Bevy raises without the `preview` feature.
#[cfg(not(feature = "preview"))]
fn needs_preview() -> PyErr {
    PyRuntimeError::new_err(
        "this build of fastanim (the browser's) can't render video or PNG or open the preview \
         window; only svg output works",
    )
}

/// Opens the preview window on `tl` and blocks until it closes. With `watch`, the scene is run
/// again whenever that file changes, by `fastanim._load.load(file, name)`, and the new timeline
/// shown from the current time on; a broken scene is reported and the last good one kept.
pub(crate) fn preview(
    py: Python<'_>,
    tl: BakedTimeline,
    watch: Option<(PathBuf, String)>,
) -> PyResult<()> {
    #[cfg(feature = "preview")]
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        // winit makes one event loop per process; a second window would panic.
        static OPENED: AtomicBool = AtomicBool::new(false);
        if OPENED.swap(true, Ordering::SeqCst) {
            return Err(PyRuntimeError::new_err(
                "the preview window can only be opened once per Python process",
            ));
        }
        let reload = watch.map(|(path, name)| reloader(path, name));
        py.detach(|| fastanim_bevy::run_command(None, tl, reload))
            .map_err(PyRuntimeError::new_err)
    }
    #[cfg(not(feature = "preview"))]
    {
        let _ = (py, tl, watch);
        Err(needs_preview())
    }
}

#[cfg(feature = "preview")]
fn reloader(path: PathBuf, name: String) -> fastanim_bevy::Reload {
    let (mut files, mut seen) = watched(&path);
    Box::new(move || {
        if files.iter().map(|p| mtime(p)).eq(seen.iter().copied()) {
            return None;
        }
        let reloaded = Python::attach(|py| {
            let load = py.import("fastanim._load")?.getattr("load")?;
            crate::bake_scene(&load.call1((&path, &name))?)
        })
        .inspect(|_| eprintln!("reloaded {}", path.display()))
        .inspect_err(|e| Python::attach(|py| e.print(py)))
        .ok();
        // A failed run may not have imported every module, so keep watching the last good set.
        // Either way a broken scene isn't retried until the next save.
        if reloaded.is_some() {
            (files, seen) = watched(&path);
        } else {
            seen = files.iter().map(|p| mtime(p)).collect();
        }
        reloaded
    })
}

/// The scene file at `path` and the modules next to it that it imports, with their
/// modification times.
#[cfg(feature = "preview")]
fn watched(path: &Path) -> (Vec<PathBuf>, Vec<Option<std::time::SystemTime>>) {
    let files = Python::attach(|py| {
        let watched = py.import("fastanim._load")?.getattr("watched")?;
        watched.call1((path,))?.extract::<Vec<PathBuf>>()
    })
    .unwrap_or_else(|_| vec![path.to_owned()]);
    let mtimes = files.iter().map(|p| mtime(p)).collect();
    (files, mtimes)
}

#[cfg(feature = "preview")]
fn mtime(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// The file and name of scene function `f`, if it was defined in a file that exists.
pub(crate) fn source(f: &Bound<'_, PyAny>) -> Option<(PathBuf, String)> {
    let file: PathBuf = f
        .getattr("__code__")
        .ok()?
        .getattr("co_filename")
        .ok()?
        .extract()
        .ok()?;
    let name = f.getattr("__name__").ok()?.extract().ok()?;
    Path::is_file(&file).then_some((file, name))
}
