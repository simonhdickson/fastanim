# Epicycles: manim vs fastanim

Fourier epicycles drawing a star: `N` rotating vectors, each with its circle, and the path the
last tip traces. The same scene twice, both in Python, both calling a Python function per shape
per frame:

- `manim_epicycles.py`: manim's usual way, after 3Blue1Brown's `FourierCirclesScene`. A clock
  advanced by `dt` updaters, each vector chained to the one before it, and a `TracedPath`.
- `fastanim_epicycles.py`: fastanim's Python API. Every shape is a function of `t`.
- `fourier.py`: the star, its coefficients and the vector tips, shared by both, in plain Python.
- `epicycles.rhai`: the same scene in fastanim's Rhai, for comparison with fastanim's Python.
  It draws the same pixels. Each circle reads the one before it from the state, and the trace
  is an FFT, which keeps the setup under Rhai's operation limit.

The language is the same, and so are the maths and the per-frame Python calls. Any difference
comes from the engines:

- **Rendering.** manim rasterises every frame with Cairo on the CPU. fastanim draws on the GPU
  (Bevy) and pipes frames to ffmpeg.
- **The trace.** `TracedPath` gains a point each frame and is re-stroked whole every frame, so
  the work grows as the drawing does. Here the trace is a slice of the finished curve.
- **Seeking.** manim frames depend on the frame before (`dt`), so frame 500 needs frames 0-499.
  fastanim frames are pure in `t`: `still --at 9.5` renders one frame, and the preview scrubs.

## Running

```sh
# fastanim with video export, from a checkout (into a virtualenv)
(cd fastanim-py && maturin develop --release)
(cd fastanim-py/preview && maturin develop --release)
pip install manim

./bench.sh 10 50 100 200                  # 1080p60, 10 s per run
QUALITY=480p15 DURATION=4 ./bench.sh 50   # quicker
```

`bench.sh` runs the Rhai scene with the `fastanim` CLI: set `FASTANIM=../../target/release/fastanim`
after `cargo build --release -p fastanim-cli` if it isn't on your PATH.

Or one at a time:

```sh
N=100 manim -qh --disable_caching manim_epicycles.py Epicycles
fastanim render epicycles.rhai -o epicycles.mp4   # N is `let n` in the file
N=100 python -m fastanim render fastanim_epicycles.py -o epicycles.mp4
N=100 python -m fastanim preview fastanim_epicycles.py
```

## Results

Ryzen laptop (16 threads, Radeon 780M iGPU), Python 3.14, manim 0.22 (Cairo), 1080p60, 10 s:

| N | manim | fastanim | speedup | fastanim (Rhai) |
|---:|---:|---:|---:|---:|
| 10 | 5.4 s | 2.9 s | 1.9x | 3.0 s |
| 50 | 10.6 s | 3.0 s | 3.5x | 3.0 s |
| 100 | 16.1 s | 3.0 s | 5.3x | 3.1 s |
| 200 | 26.7 s | 3.4 s | 7.9x | 3.2 s |
| 400 | 47.2 s | 5.3 s | 9.0x | 3.4 s |
| 2000 | 210.3 s | 21.2 s | 9.9x | 6.4 s |

fastanim's floor of about 3 s is starting the GPU and encoding 600 frames. One frame at 9.5 s
(`python -m fastanim still fastanim_epicycles.py --at 9.5`) takes 0.35 s at N=200 and 0.46 s
at N=400. manim can only get there by rendering the 570 frames before it.

What growth fastanim has above N=200 is the Python updaters, 2N+1 calls per frame, and the GPU
drawing 2N+1 strokes (about 5 ms a frame at N=400 on this iGPU). The updaters run under the
GIL, so export evaluates frames of a Python scene with updaters on one thread; sixteen threads
fighting over the GIL were slower (6.8 s at N=400). Rhai has no GIL, so the Rhai scene
evaluates frames on every core: at N=5000 it renders in 15 s at ~900% CPU, against 50 s at
~140% from Python.
