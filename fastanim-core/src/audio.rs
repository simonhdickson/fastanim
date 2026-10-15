//! Sound: WAV files and the clips of them a scene plays (roadmap #22).
//!
//! Baking only records when each clip starts; the preview plays them and export muxes them, so
//! every frame stays a pure function of time.

use std::path::Path;
use std::sync::Arc;

use crate::anim::{Animation, RateFn};
use crate::mobject::SceneState;

/// A loaded WAV file. Only its header is read: the length is all baking needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Audio {
    path: Arc<Path>,
    duration: f32,
}

impl Audio {
    /// Reads the WAV header at `path`.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let r = hound::WavReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self {
            path: path.into(),
            duration: r.duration() as f32 / r.spec().sample_rate as f32,
        })
    }

    /// The file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Length in seconds.
    pub fn duration(&self) -> f32 {
        self.duration
    }

    /// The whole file.
    pub fn whole(&self) -> Sound {
        self.clip(0.0, self.duration)
    }

    /// Seconds `start..end` of the file, clamped to its length.
    pub fn clip(&self, start: f32, end: f32) -> Sound {
        let start = start.clamp(0.0, self.duration);
        Sound {
            path: self.path.clone(),
            offset: start,
            duration: (end.min(self.duration) - start).max(0.0),
        }
    }
}

/// Part of an [`Audio`]. Played as an animation it lasts as long as the clip, so
/// `scene.play(sound)` waits for it to finish; it also composes with [`Parallel`](crate::Parallel)
/// and [`Sequence`](crate::Sequence). [`Scene::add_sound`](crate::Scene::add_sound) starts one
/// without waiting.
///
/// `run_time` on a sound changes how long `play` waits, not the audio.
#[derive(Debug, Clone, PartialEq)]
pub struct Sound {
    /// The file.
    pub path: Arc<Path>,
    /// Where in the file the clip starts, in seconds.
    pub offset: f32,
    /// Length in seconds.
    pub duration: f32,
}

impl Animation for Sound {
    fn plan(&mut self, _: &SceneState) {}
    fn sample(&self, _: f32, _: &mut SceneState) {}
    fn duration(&self) -> f32 {
        self.duration
    }
    fn rate_fn(&self) -> RateFn {
        RateFn::Linear
    }
    fn sounds(&self, at: f32, out: &mut Vec<SoundEvent>) {
        out.push(SoundEvent {
            at,
            sound: self.clone(),
        });
    }
    fn label(&self) -> std::borrow::Cow<'static, str> {
        "Sound".into()
    }
}

/// A sound starting at scene time `at`.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundEvent {
    /// Scene time in seconds.
    pub at: f32,
    /// What plays.
    pub sound: Sound,
}

impl SoundEvent {
    /// Scene time the sound ends.
    pub fn end(&self) -> f32 {
        self.at + self.sound.duration
    }
}
