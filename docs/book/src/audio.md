# Sound

Scenes can play WAV files. The desktop preview plays them in time with the scene, and
`render` mixes them into `mp4` and `webm` output. GIFs and frame sequences have no sound, and
the browser playground doesn't play audio yet: `audio` fails there.

## Loading

`audio(path)` loads a WAV file. A relative path is relative to the current directory.
`.duration` is the file's length in seconds.

```python
voice = audio("narration.wav")
print(voice.duration)
```

## Playing and waiting

`sound(a)` is an animation that plays all of `a`. `sound(a, start, end)` plays only seconds
`start` to `end` of it. Like any animation, it lasts as long as the clip, so `s.play` waits
for the audio to finish before the next call.

```python
s.play(sound(voice))             # all of it, then carry on
s.play(sound(voice, 1.5, 4.0))   # seconds 1.5 to 4 of the file
```

A sound combines with other animations like any other. Played together, the step lasts until
the longest one finishes. In a `sequence`, each part starts when the previous one ends.

```python
title = s.add(text("Hello"))
s.play(write(title), sound(voice, 0, 2.5))
s.play(sequence(sound(voice, 0, 1), fade_out(title)))
```

`run_time` on a sound changes how long `play` waits. It doesn't stretch the audio.

## Playing in the background

`s.add_sound(a)` and `s.add_sound(a, start, end)` start a sound without waiting:
time doesn't advance, so the scene carries on while it plays. `s.wait_for_sounds()` waits
until every sound started so far has finished.

```python
s.add_sound(audio("music.wav"))
c = s.add(circle(1))
s.play(create(c))
s.wait_for_sounds()   # until the music ends
```

A sound still playing when the scene ends is cut off there.

## Previewing

The preview plays sounds while the clock runs at normal speed. Pausing, stepping or scrubbing
silences them. When playback resumes, each sound picks up from the current time.
