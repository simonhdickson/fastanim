//! Videos play frame by frame on a shape that moves like any other, with their sound.

use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

use fastanim_core::kurbo::Affine;
use fastanim_core::{Animation, Image, Scene, VState, Video, play_video};

/// A 10 fps video of `n` 2x1 frames, frame `i`'s pixels all `i`.
fn video(n: u8) -> Arc<Video> {
    let frames = (0..n)
        .map(|i| Image::from_rgba(2, 1, vec![i; 8]).unwrap())
        .collect();
    Arc::new(Video::from_frames(frames, 10.0).unwrap())
}

fn frame(m: &VState) -> u8 {
    m.image.as_ref().unwrap().image.pixels()[0]
}

#[test]
fn plays_frames_where_the_shape_is() {
    let v = video(20);
    assert_eq!(v.duration(), 2.0);
    let shape = VState::video(v.clone())
        .rotate(FRAC_PI_2)
        .shift((1.0, 2.0).into());
    let placed = shape.image.as_ref().unwrap().transform;
    let mut s = Scene::new();
    let id = s.add(shape);

    let mut play = play_video(id, v.clone());
    assert_eq!(play.duration(), 2.0);
    play.plan(s.state());
    let mut st = s.state().clone();
    play.sample(0.0, &mut st);
    assert_eq!(frame(&st[&id]), 0);
    play.sample(0.5, &mut st);
    assert_eq!(frame(&st[&id]), 10);
    assert_eq!(st[&id].image.as_ref().unwrap().transform, placed);
    play.sample(1.0, &mut st);
    assert_eq!(frame(&st[&id]), 19, "the last frame, not past it");

    let mut back = play_video(id, v).range(1.0, 0.0).loops(2);
    assert_eq!(back.duration(), 2.0);
    back.plan(s.state());
    back.sample(0.25, &mut st);
    assert_eq!(frame(&st[&id]), 5);
    back.sample(1.0, &mut st);
    assert_eq!(frame(&st[&id]), 0);
}

#[test]
fn sized_like_an_image_of_the_source() {
    let v = video(1);
    let m = VState::video(v.clone());
    let h = m.path.bbox().unwrap().height();
    assert!((h - v.default_height()).abs() < 1e-9);
    // Shearing and stretching distort the picture with the outline.
    let before = m.image.as_ref().unwrap().transform;
    let skew = Affine::new([2.0, 0.0, 1.0, 1.0, 0.0, 0.0]);
    let m = m.transform(skew);
    assert_eq!(m.image.as_ref().unwrap().transform, skew * before);
}

/// A real file through `ffmpeg`, when it is installed.
#[test]
fn loads_a_file_with_sound() {
    let dir = std::env::temp_dir().join(format!("fastanim-video-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let clip = dir.join("clip.mp4");
    let made = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=d=1:s=64x48:r=10",
        ])
        .args(["-f", "lavfi", "-i", "sine=d=1", "-pix_fmt", "yuv420p"])
        .arg(&clip)
        .status();
    if !made.is_ok_and(|s| s.success()) {
        eprintln!("skipped: no ffmpeg");
        return;
    }
    let v = Video::load(&clip, 24).unwrap();
    assert_eq!(v.frames().len(), 10);
    assert_eq!(v.fps(), 10.0);
    assert_eq!((v.frames()[0].width(), v.frames()[0].height()), (32, 24));
    assert_eq!(v.default_height(), 48.0 / 1080.0 * 8.0);
    let audio = v.audio().expect("the sine track");
    assert!((audio.duration() - 1.0).abs() < 0.1);

    let mut s = Scene::new();
    let id = s.add(VState::video(Arc::new(v)));
    let v = s.state()[&id]
        .image
        .as_ref()
        .unwrap()
        .video
        .clone()
        .unwrap();
    let mut sounds = Vec::new();
    play_video(id, v).loops(2).sounds(3.0, &mut sounds);
    let starts: Vec<f32> = sounds.iter().map(|e| e.at).collect();
    assert_eq!(starts, [3.0, 4.0]);

    assert!(Video::load(&dir.join("missing.mp4"), 720).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}
