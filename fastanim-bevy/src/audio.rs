//! Sound in the preview: the scene's sounds follow the clock. They play only while the clock
//! runs at normal speed; pausing, seeking or a reload stops them, and they pick up again
//! mid-clip from the new time.
//!
//! The output device lives on its own thread, since it needn't be `Send`; this side only
//! decides what to start and stop.

use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

use bevy::prelude::*;
use fastanim_core::{BakedTimeline, SoundEvent};

use crate::{SceneClock, Timeline};

/// A frame's clock may drift this far from real time before it counts as a seek.
const SLACK: f32 = 0.05;

/// What the output thread is told.
#[derive(Debug, Clone, PartialEq)]
enum Cmd {
    /// Plays `take` seconds of `path`, starting `skip` seconds in.
    Play {
        path: Arc<Path>,
        skip: f32,
        take: f32,
    },
    /// Silences everything playing.
    Stop,
}

/// Which sounds are playing, and what changes the clock calls for.
#[derive(Default)]
struct Mixer {
    /// The timeline the playing sounds came from.
    timeline: Option<Arc<BakedTimeline>>,
    /// Indices into its sounds.
    playing: Vec<usize>,
    /// Clock time last frame, if it was running.
    last: Option<f32>,
}

impl Mixer {
    /// The commands that bring playback in line with the clock at `t`, `dt` seconds after the
    /// last call; `running` if it advances at normal speed.
    fn update(&mut self, tl: &Arc<BakedTimeline>, t: f32, dt: f32, running: bool) -> Vec<Cmd> {
        let mut out = Vec::new();
        let same = self
            .timeline
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, tl));
        let smooth = self.last.is_some_and(|l| (t - (l + dt)).abs() <= SLACK);
        if !(running && same && smooth) && !self.playing.is_empty() {
            out.push(Cmd::Stop);
            self.playing.clear();
        }
        self.timeline = Some(tl.clone());
        self.last = running.then_some(t);
        if !running {
            return out;
        }
        let end = |e: &SoundEvent| e.end().min(tl.duration());
        let sounds = tl.sounds();
        self.playing.retain(|&i| t < end(&sounds[i]));
        for (i, e) in sounds.iter().enumerate() {
            if e.at <= t && t < end(e) && !self.playing.contains(&i) {
                out.push(Cmd::Play {
                    path: e.sound.path.clone(),
                    skip: e.sound.offset + (t - e.at),
                    take: end(e) - t,
                });
                self.playing.push(i);
            }
        }
        out
    }
}

/// The preview's sound: a [`Mixer`] and, once a sound first plays, the output thread.
#[derive(Resource, Default)]
pub(crate) struct AudioOut {
    mixer: Mixer,
    /// `None` until first needed; then the thread, or `None` inside if there's no device.
    out: Option<Option<Sender<Cmd>>>,
}

pub(crate) fn sync_audio(
    time: Res<Time>,
    tl: Res<Timeline>,
    clock: Res<SceneClock>,
    mut audio: ResMut<AudioOut>,
) {
    let running = clock.playing && clock.speed == 1.0;
    let cmds = audio
        .mixer
        .update(&tl.0, clock.t, time.delta_secs(), running);
    if cmds.is_empty() {
        return;
    }
    if let Some(tx) = audio.out.get_or_insert_with(spawn_output) {
        for c in cmds {
            // The thread only exits if it panicked; the preview carries on silently.
            let _ = tx.send(c);
        }
    }
}

/// Opens the default output device on a thread of its own, or reports why it couldn't.
fn spawn_output() -> Option<Sender<Cmd>> {
    let (tx, rx) = mpsc::channel::<Cmd>();
    let (ready_tx, ready_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut stream = match rodio::OutputStreamBuilder::open_default_stream() {
            Ok(s) => s,
            Err(e) => {
                let _ = ready_tx.send(Err(e.to_string()));
                return;
            }
        };
        stream.log_on_drop(false);
        let _ = ready_tx.send(Ok(()));
        let mut files: HashMap<Arc<Path>, Option<Arc<[u8]>>> = HashMap::new();
        // Set to stop everything started since the last `Stop`. Sources go straight into the
        // mixer: rodio 0.21's `Sink` skips resampling at the start of a sound, which blips.
        let mut stop = Arc::new(AtomicBool::new(false));
        for cmd in rx {
            let (path, skip, take) = match cmd {
                Cmd::Stop => {
                    stop.store(true, Ordering::Relaxed);
                    stop = Arc::new(AtomicBool::new(false));
                    continue;
                }
                Cmd::Play { path, skip, take } => (path, skip, take),
            };
            let bytes = files.entry(path.clone()).or_insert_with(|| {
                (std::fs::read(&path))
                    .inspect_err(|e| eprintln!("error: {}: {e}", path.display()))
                    .ok()
                    .map(Arc::from)
            });
            let Some(bytes) = bytes.clone() else {
                continue;
            };
            match rodio::Decoder::new(Cursor::new(bytes)) {
                Ok(d) => {
                    use rodio::Source;
                    let stop = stop.clone();
                    stream.mixer().add(
                        d.skip_duration(Duration::from_secs_f32(skip))
                            .take_duration(Duration::from_secs_f32(take))
                            .stoppable()
                            .periodic_access(Duration::from_millis(5), move |s| {
                                if stop.load(Ordering::Relaxed) {
                                    s.stop();
                                }
                            }),
                    );
                }
                Err(e) => eprintln!("error: {}: {e}", path.display()),
            }
        }
    });
    match ready_rx.recv() {
        Ok(Ok(())) => Some(tx),
        Ok(Err(e)) => {
            eprintln!("note: no audio output ({e}); previewing without sound");
            None
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fastanim_core::{Scene, Sound};

    fn timeline() -> Arc<BakedTimeline> {
        let sound = |duration| Sound {
            path: Path::new("a.wav").into(),
            offset: 0.5,
            duration,
        };
        let mut s = Scene::new();
        s.wait(1.0);
        s.add_sound(sound(2.0));
        s.play(sound(1.0));
        s.wait(3.0);
        Arc::new(s.bake())
    }

    fn play(skip: f32, take: f32) -> Cmd {
        Cmd::Play {
            path: Path::new("a.wav").into(),
            skip,
            take,
        }
    }

    const NOTHING: [Cmd; 0] = [];

    #[test]
    fn sounds_follow_the_clock() {
        let tl = timeline();
        let mut m = Mixer::default();
        assert_eq!(m.update(&tl, 0.0, 0.0, true), NOTHING);
        assert_eq!(m.update(&tl, 0.5, 0.5, true), NOTHING);
        // Both start at 1; playing on doesn't restart them.
        assert_eq!(
            m.update(&tl, 1.0, 0.5, true),
            [play(0.5, 2.0), play(0.5, 1.0)]
        );
        assert_eq!(m.update(&tl, 1.5, 0.5, true), NOTHING);
        // Pausing stops them; resuming mid-clip skips in.
        assert_eq!(m.update(&tl, 1.5, 0.0, false), [Cmd::Stop]);
        assert_eq!(m.update(&tl, 1.5, 0.0, false), NOTHING);
        assert_eq!(
            m.update(&tl, 1.5, 0.0, true),
            [play(1.0, 1.5), play(1.0, 0.5)]
        );
        // A seek restarts whatever is under the new time.
        assert_eq!(m.update(&tl, 2.5, 0.01, true), [Cmd::Stop, play(2.0, 0.5)]);
        // So does a reload, even to the same time.
        let new = timeline();
        assert_eq!(m.update(&new, 2.5, 0.0, true), [Cmd::Stop, play(2.0, 0.5)]);
        assert_eq!(m.update(&new, 3.5, 1.0, true), NOTHING);
    }
}
