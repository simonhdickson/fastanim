# Export performance notes

Findings from profiling `fastanim render` (2026-10-12), kept so the experiments don't need
repeating. None of the speedups below are merged: default export is already fast enough
(a 4k60 scene renders in ~30s, against ~1m30s for a similar scene in manim).

The code for `--hw` and `--gpu-yuv` lives on the branch `experiment/gpu-yuv-hw`
(commit `600d81c`, one commit on top of `c7b03dd`).

## Setup

`fastanim-script/scenes/growing_wave.rhai` at 1080p60: 1261 frames, 16-core laptop with an
AMD Phoenix iGPU (Radeon 780M), ffmpeg via VAAPI. Times are wall clock for the whole render.

## Where the time goes

The export pipeline is: `eval` on every core, Vello render and readback on the main thread, a
writer thread piping raw RGBA into ffmpeg.

- `eval` is not the limit. Its workers sit at ~5% CPU each, waiting on full channels.
- ffmpeg is. With the default `libx264 -crf 18` it uses ~550% CPU and tops out near 220 fps
  even when fed from a file, which is about the pipeline's speed.
- Behind that, the main thread spends ~4 of ~7 s blocked sending frames to the writer: 8.3 MB of
  RGBA per 1080p frame, ~10 GB per render, through the pipe and ffmpeg's RGBA→yuv420p
  conversion.
- The GPU is not the limit either: `gpu_busy_percent` stays at 12–32%.

## What was tried

| Flags | Time | CPU |
|---|---|---|
| default | 7.1 s | 625% |
| double-buffered readback | 6.8 s | 650% |
| 1 MB pipe buffer (`F_SETPIPE_SZ`) | 6.6 s | 750% |
| `--hw` | 6.1 s | 440% |
| `--gpu-yuv` | 4.8 s | 630% |
| `--hw --gpu-yuv` | 3.7 s | 240% |

- **Double-buffered readback** (3 readback buffers in flight, wait per submission index):
  output byte-identical, ~2% faster. Readback was never the bottleneck here. Merged since
  (`FrameRenderer::submit`/`finish`) for GPU-heavy scenes; on `bench/epicycles` at N=400 it
  made no measurable difference either, as the GIL-bound Python updaters were the limit.
- **Bigger pipe buffer:** small win, needs `libc` and `unsafe`. Not kept.
- **`--hw`:** `h264_vaapi` instead of `libx264`, with
  `-vaapi_device /dev/dri/renderD128 -vf format=nv12,hwupload -rc_mode CQP -qp 18`.
  - QP 18 matches CRF 18 in SSIM (0.99996 vs source for both), at ~1.8× the file size.
  - Convert to nv12 on the CPU (`format=nv12`). `scale_vaapi=format=nv12` is faster but shifts
    colours (SSIM drops to 31 dB).
  - Alone it barely helps, because the RGBA feed is still the limit.
  - The device path is hardcoded, which is wrong on multi-GPU machines. NVENC is untested.
- **`--gpu-yuv`:** a compute shader (`fastanim-bevy/src/yuv420p.wgsl` on the branch) packs the
  Vello frame into planar yuv420p before readback, so 3.1 MB a frame instead of 8.3 MB, with
  no per-row copy. ffmpeg then reads `-pix_fmt yuv420p`.
  - It matches swscale's default: BT.601 limited range, chroma the mean of each 2×2 block.
  - It differs from the default encode by 57 dB PSNR per plane, far below encoder noise.
  - Cost: the colour conversion now exists twice. If the codec args move to BT.709 or tag
    colour metadata, the shader has to follow.
  - The branch has a test checking every output byte against a CPU conversion.

## If export speed matters again

1. Cherry-pick `--gpu-yuv` from the branch. It works on any GPU and gives most of the gain.
2. Add `--hw` on top for the remaining ~1.3×, ideally with a device option and NVENC.
