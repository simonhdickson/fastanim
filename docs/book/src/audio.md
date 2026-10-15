# Sound

Scenes can play WAV files. The desktop preview plays them in time with the scene, and
`fastanim render` mixes them into `mp4` and `webm` output. GIFs and frame sequences have no
sound, and the browser playground doesn't play audio yet: `audio` fails there.

## Loading

`audio(path)` loads a WAV file. A relative path is relative to the script's directory.
`.duration` is the file's length in seconds.

```rhai
let voice = audio("narration.wav");
print(voice.duration);
```

## Playing and waiting

`sound(a)` is an animation that plays all of `a`. `sound(a, start, end)` plays only seconds
`start` to `end` of it. Like any animation, it lasts as long as the clip, so `scene.play` waits
for the audio to finish before the next call.

```rhai
scene.play(sound(voice));                 // all of it, then carry on
scene.play(sound(voice, 1.5, 4.0));       // seconds 1.5 to 4 of the file
```

A sound combines with other animations like any other. Played together, the step lasts until
the longest one finishes. In a `sequence`, each part starts when the previous one ends.

```rhai
let title = scene.add(text("Hello"));
scene.play([write(title), sound(voice, 0.0, 2.5)]);
scene.play(sequence([sound(voice, 0.0, 1.0), fade_out(title)]));
```

`run_time` on a sound changes how long `play` waits. It doesn't stretch the audio.

## Playing in the background

`scene.add_sound(a)` and `scene.add_sound(a, start, end)` start a sound without waiting:
time doesn't advance, so the scene carries on while it plays. `scene.wait_for_sounds()` waits
until every sound started so far has finished.

```rhai
scene.add_sound(audio("music.wav"));
let c = scene.add(circle(1.0));
scene.play(create(c));
scene.wait_for_sounds();                  // until the music ends
```

A sound still playing when the scene ends is cut off there.

## Previewing

The preview plays sounds while the clock runs at normal speed. Pausing, stepping or scrubbing
silences them. When playback resumes, each sound picks up from the current time.
