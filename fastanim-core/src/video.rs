//! Video files: every frame decoded once by `ffmpeg` into [`Image`]s, drawn on a shape like
//! any image so it follows every move, scale, rotate and stretch, and swapped frame by frame
//! by [`play_video`]. The audio track, if any, plays with it.

use std::fmt;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;

use serde_json::Value;

use crate::FRAME_HEIGHT;
use crate::anim::{Animation, RateFn, get};
use crate::audio::{Audio, SoundEvent};
use crate::image::Image;
use crate::mobject::{MobjectId, SceneState};

/// A decoded video: its frames, frame rate and audio track.
pub struct Video {
    frames: Vec<Image>,
    fps: f64,
    height: u32,
    audio: Option<Audio>,
}

impl Video {
    /// Video from `frames` at `fps`, with no sound.
    pub fn from_frames(frames: Vec<Image>, fps: f64) -> Result<Self, String> {
        let Some(first) = frames.first() else {
            return Err("a video needs at least one frame".into());
        };
        if !(fps > 0.0 && fps.is_finite()) {
            return Err(format!("a video can't play at {fps} fps"));
        }
        let height = first.height();
        Ok(Self {
            frames,
            fps,
            height,
            audio: None,
        })
    }

    /// Decodes the video file at `path` with `ffmpeg`, scaled down to at most `max_height`
    /// pixels tall, and extracts its audio track to a WAV in the temp directory.
    pub fn load(path: &Path, max_height: u32) -> Result<Self, String> {
        let at = |e: String| format!("{}: {e}", path.display());
        if !path.is_file() {
            return Err(at("no such file".into()));
        }
        let probe = run(Command::new("ffprobe")
            .args(["-v", "error", "-show_entries"])
            .args(["stream=codec_type,width,height,r_frame_rate", "-of", "json"])
            .arg(path))
        .map_err(at)?;
        let probe: Value = serde_json::from_slice(&probe).map_err(|e| at(e.to_string()))?;
        let streams = (probe.get("streams").and_then(Value::as_array))
            .map(Vec::as_slice)
            .unwrap_or_default();
        fn kind(s: &Value) -> Option<&str> {
            s.get("codec_type").and_then(Value::as_str)
        }
        let v = (streams.iter().find(|s| kind(s) == Some("video")))
            .ok_or_else(|| at("no video stream".into()))?;
        let dim = |k: &str| v.get(k).and_then(Value::as_u64).filter(|&n| n > 0);
        let (Some(w), Some(h)) = (dim("width"), dim("height")) else {
            return Err(at("the video stream has no size".into()));
        };
        let fps = (v.get("r_frame_rate").and_then(Value::as_str))
            .and_then(|r| {
                let (n, d) = r.split_once('/')?;
                Some(n.parse::<f64>().ok()? / d.parse::<f64>().ok()?)
            })
            .ok_or_else(|| at("the video stream has no frame rate".into()))?;

        // Even sizes, as many codecs' frames are.
        let max_height = u64::from(max_height.max(2));
        let (ow, oh) = if h > max_height {
            ((w * max_height / h).max(2) & !1, max_height & !1)
        } else {
            (w, h)
        };
        // shortcut: ignores rotation metadata (phone clips play sideways); honour it when
        // someone needs it.
        let raw = run(Command::new("ffmpeg")
            .args(["-v", "error", "-noautorotate", "-i"])
            .arg(path)
            .args(["-vf", &format!("scale={ow}:{oh}"), "-f", "rawvideo"])
            .args(["-pix_fmt", "rgba", "-"]))
        .map_err(at)?;
        let size = (ow * oh * 4) as usize;
        let frames = (raw.chunks_exact(size))
            .map(|px| Image::from_rgba(ow as u32, oh as u32, px.to_vec()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(at)?;
        let mut video = Self::from_frames(frames, fps).map_err(at)?;
        video.height = h as u32;
        if streams.iter().any(|s| kind(s) == Some("audio")) {
            video.audio = Some(extract_audio(path).map_err(at)?);
        }
        Ok(video)
    }

    /// The frames.
    pub fn frames(&self) -> &[Image] {
        &self.frames
    }

    /// Frames per second.
    pub fn fps(&self) -> f64 {
        self.fps
    }

    /// The audio track.
    pub fn audio(&self) -> Option<&Audio> {
        self.audio.as_ref()
    }

    /// Length in seconds.
    pub fn duration(&self) -> f64 {
        self.frames.len() as f64 / self.fps
    }

    /// The frame showing at `secs`, clamped to the first and last.
    pub fn frame_at(&self, secs: f64) -> &Image {
        let i = (secs * self.fps).floor().max(0.0) as usize;
        &self.frames[i.min(self.frames.len() - 1)]
    }

    /// The size an image of the source (not the scaled-down frames) would have, so a size
    /// cap doesn't shrink it on screen.
    pub fn default_height(&self) -> f64 {
        f64::from(self.height) / 1080.0 * FRAME_HEIGHT
    }
}

impl PartialEq for Video {
    fn eq(&self, o: &Self) -> bool {
        std::ptr::eq(self, o)
    }
}

impl fmt::Debug for Video {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let img = &self.frames[0];
        let (w, h, n) = (img.width(), img.height(), self.frames.len());
        write!(f, "Video({w}x{h}, {n} frames at {} fps)", self.fps)
    }
}

/// Runs `cmd`, returning its stdout, or its stderr as the error.
fn run(cmd: &mut Command) -> Result<Vec<u8>, String> {
    let name = cmd.get_program().to_string_lossy().into_owned();
    let mut child = (cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped()))
    .spawn()
    .map_err(|e| format!("could not start {name} (video needs ffmpeg installed): {e}"))?;
    // Read stderr on a thread so a chatty decoder can't fill its pipe and stall.
    let mut stderr = child.stderr.take().expect("piped stderr");
    let errs = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let mut out = Vec::new();
    (child.stdout.take().expect("piped stdout"))
        .read_to_end(&mut out)
        .map_err(|e| format!("{name}: {e}"))?;
    let status = child.wait().map_err(|e| format!("{name}: {e}"))?;
    let errs = errs.join().unwrap_or_default();
    if !status.success() {
        return Err(format!("{name} failed ({status}): {}", errs.trim()));
    }
    Ok(out)
}

/// The audio track of `path` as a WAV in the temp directory, named after the file's path,
/// size and modification time so loading it again reuses it. It stays there, as export
/// mixes sounds in by path.
fn extract_audio(path: &Path) -> Result<Audio, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let mut h = DefaultHasher::new();
    (path.canonicalize().map_err(|e| e.to_string())?).hash(&mut h);
    meta.len().hash(&mut h);
    meta.modified().ok().hash(&mut h);
    let wav = std::env::temp_dir().join(format!("fastanim-{:016x}.wav", h.finish()));
    if !wav.is_file() {
        // Written beside, then renamed, so a half-written file is never reused.
        let part = wav.with_extension("part.wav");
        run(Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-i"])
            .arg(path)
            .args(["-vn", "-acodec", "pcm_s16le"])
            .arg(&part))?;
        std::fs::rename(&part, &wav).map_err(|e| e.to_string())?;
    }
    Audio::load(&wav)
}

/// Plays a video shape's frames, and its sound; see [`play_video`].
pub struct PlayVideo {
    id: MobjectId,
    video: Arc<Video>,
    from: f64,
    to: f64,
    loops: u32,
}

/// Plays the video on shape `id` (made by [`VState::video`](crate::VState::video)) once
/// through, with its audio track.
pub fn play_video(id: MobjectId, video: Arc<Video>) -> PlayVideo {
    PlayVideo {
        id,
        from: 0.0,
        to: video.duration(),
        video,
        loops: 1,
    }
}

impl PlayVideo {
    /// Plays seconds `from` to `to` instead; backwards, and silent, if `to < from`.
    pub fn range(self, from: f64, to: f64) -> Self {
        Self { from, to, ..self }
    }

    /// Plays it `n` times over, `n` times as long.
    pub fn loops(self, n: u32) -> Self {
        Self {
            loops: n.max(1),
            ..self
        }
    }

    fn once(&self) -> f64 {
        (self.to - self.from).abs()
    }
}

impl Animation for PlayVideo {
    fn plan(&mut self, state: &SceneState) {
        let m = get(state, self.id);
        assert!(
            m.image.as_ref().and_then(|i| i.video.as_ref()) == Some(&self.video),
            "play_video needs the shape `VState::video` made"
        );
    }

    fn sample(&self, alpha: f32, state: &mut SceneState) {
        let n = f64::from(self.loops);
        let a = f64::from(alpha) * n;
        // Lands on the last frame at the end rather than wrapping back to the first.
        let u = if a >= n { 1.0 } else { a.fract() };
        let secs = self.from + (self.to - self.from) * u;
        // Only the pixels change: the placement is wherever other animations put it.
        let mut m = get(state, self.id).clone();
        if let Some(i) = &mut m.image {
            i.image = self.video.frame_at(secs).clone();
        }
        state.insert(self.id, m);
    }

    fn duration(&self) -> f32 {
        (self.once() * f64::from(self.loops)) as f32
    }

    fn rate_fn(&self) -> RateFn {
        RateFn::Linear
    }

    fn sounds(&self, at: f32, out: &mut Vec<SoundEvent>) {
        let Some(audio) = &self.video.audio else {
            return;
        };
        if self.to <= self.from {
            return;
        }
        for k in 0..self.loops {
            out.push(SoundEvent {
                at: at + (self.once() * f64::from(k)) as f32,
                sound: audio.clip(self.from as f32, self.to as f32),
            });
        }
    }

    fn label(&self) -> std::borrow::Cow<'static, str> {
        "PlayVideo".into()
    }
}
