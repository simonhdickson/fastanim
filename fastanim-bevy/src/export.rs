//! Headless export (SPEC §8.5): frames are rendered offscreen with Vello, read back to the CPU
//! and piped as raw RGBA into `ffmpeg`. Time comes from the frame index only, so output is
//! deterministic.
//!
//! This drives Vello on its own wgpu device rather than a windowless Bevy app: the encoding is
//! the same [`encode`] the preview uses, and nothing else in Bevy is needed to make a frame.

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use bevy::tasks::futures_lite::future::block_on;
use bevy_vello::vello::kurbo::Affine;
use bevy_vello::vello::peniko::Color;
use bevy_vello::vello::util::RenderContext;
use bevy_vello::vello::{self, AaConfig, AaSupport, RenderParams, RendererOptions, wgpu};
use fastanim_core::{BakedTimeline, FRAME_HEIGHT, FRAME_WIDTH, SceneState, to_svg};

use crate::encode;

/// Output resolution and frame rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quality {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Frames per second.
    pub fps: u32,
}

impl Default for Quality {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 60,
        }
    }
}

impl FromStr for Quality {
    type Err = String;

    /// Parses a preset: `480p15`, `720p30`, `1080p60` or `4k60`.
    fn from_str(s: &str) -> Result<Self, String> {
        let (width, height, fps) = match s {
            "480p15" => (854, 480, 15),
            "720p30" => (1280, 720, 30),
            "1080p60" => (1920, 1080, 60),
            "4k60" => (3840, 2160, 60),
            _ => {
                return Err(format!(
                    "unknown quality `{s}` (480p15, 720p30, 1080p60, 4k60)"
                ));
            }
        };
        Ok(Self { width, height, fps })
    }
}

/// Times of the frames covering `start..=end` seconds at `fps`, on the global frame grid.
pub fn frame_times(start: f32, end: f32, fps: u32) -> impl Iterator<Item = f32> {
    let first = (start * fps as f32).round() as u32;
    let last = (end * fps as f32).round() as u32;
    (first..=last).map(move |i| i as f32 / fps as f32)
}

/// GPU conversion of the rendered frame to yuv420p, so readback and ffmpeg's pipe carry 1.5
/// bytes a pixel instead of 4.
struct Yuv {
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    out: wgpu::Buffer,
    /// Output bytes, before padding `out` to whole u32s.
    len: usize,
}

/// Renders scene states to RGBA or yuv420p pixels on an offscreen GPU texture.
pub struct FrameRenderer {
    ctx: RenderContext,
    dev: usize,
    renderer: vello::Renderer,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    buffer: wgpu::Buffer,
    padded_row: u32,
    width: u32,
    height: u32,
    yuv: Option<Yuv>,
    frame: vello::Scene,
    scene: vello::Scene,
}

impl FrameRenderer {
    /// Opens a GPU device and allocates a `width × height` target. With `yuv`, frames come back
    /// as planar yuv420p (BT.601 limited range) instead of RGBA.
    pub fn new(width: u32, height: u32, yuv: bool) -> Result<Self, String> {
        let mut ctx = RenderContext::new();
        let dev = block_on(ctx.device(None)).ok_or("no compatible GPU adapter found")?;
        let device = &ctx.devices[dev].device;
        let renderer = vello::Renderer::new(
            device,
            RendererOptions {
                antialiasing_support: AaSupport::area_only(),
                ..Default::default()
            },
        )
        .map_err(|e| format!("vello: {e}"))?;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fastanim export"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let padded_row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let yuv = yuv.then(|| {
            let (w, h) = (width as usize, height as usize);
            let len = w * h + 2 * w.div_ceil(2) * h.div_ceil(2);
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("fastanim yuv420p"),
                source: wgpu::ShaderSource::Wgsl(include_str!("yuv420p.wgsl").into()),
            });
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("fastanim yuv420p"),
                layout: None,
                module: &module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
            let out = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("fastanim yuv420p"),
                size: len.next_multiple_of(4) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("fastanim yuv420p"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: out.as_entire_binding(),
                    },
                ],
            });
            Yuv {
                pipeline,
                bind_group,
                out,
                len,
            }
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fastanim readback"),
            size: yuv
                .as_ref()
                .map_or(u64::from(padded_row * height), |y| y.out.size()),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            ctx,
            dev,
            renderer,
            texture,
            view,
            buffer,
            padded_row,
            width,
            height,
            yuv,
            frame: vello::Scene::new(),
            scene: vello::Scene::new(),
        })
    }

    /// Renders one frame on black and returns tightly packed RGBA rows, top to bottom, or its
    /// yuv420p planes if made with `yuv`.
    // shortcut: one blocking readback per frame; double-buffer if GPU+readback, not eval, limits export.
    pub fn render(&mut self, state: &SceneState) -> Result<Vec<u8>, String> {
        let (w, h) = (f64::from(self.width), f64::from(self.height));
        let scale = (w / FRAME_WIDTH).min(h / FRAME_HEIGHT);
        self.frame.reset();
        encode(&mut self.frame, state);
        self.scene.reset();
        let to_pixels = Affine::translate((w / 2.0, h / 2.0)) * Affine::scale(scale);
        self.scene.append(&self.frame, Some(to_pixels));

        let handle = &self.ctx.devices[self.dev];
        let (device, queue) = (&handle.device, &handle.queue);
        self.renderer
            .render_to_texture(
                device,
                queue,
                &self.scene,
                &self.view,
                &RenderParams {
                    base_color: Color::BLACK,
                    width: self.width,
                    height: self.height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .map_err(|e| format!("vello: {e}"))?;

        let mut enc = device.create_command_encoder(&Default::default());
        if let Some(yuv) = &self.yuv {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&yuv.pipeline);
            pass.set_bind_group(0, &yuv.bind_group, &[]);
            pass.dispatch_workgroups((yuv.out.size() / 4).div_ceil(64) as u32, 1, 1);
            drop(pass);
            enc.copy_buffer_to_buffer(&yuv.out, 0, &self.buffer, 0, None);
        } else {
            enc.copy_texture_to_buffer(
                self.texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &self.buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(self.padded_row),
                        rows_per_image: None,
                    },
                },
                self.texture.size(),
            );
        }
        queue.submit([enc.finish()]);

        let slice = self.buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| drop(tx.send(r)));
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| format!("readback: {e}"))?;
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("readback: {e}"))?;
        if let Some(yuv) = &self.yuv {
            let pixels = slice.get_mapped_range()[..yuv.len].to_vec();
            self.buffer.unmap();
            return Ok(pixels);
        }
        let row = self.width as usize * 4;
        // Row-wise memcpy: a byte iterator over mapped memory is several ms per 1080p frame.
        let mut pixels = Vec::with_capacity(row * self.height as usize);
        for r in slice.get_mapped_range().chunks(self.padded_row as usize) {
            pixels.extend_from_slice(&r[..row]);
        }
        self.buffer.unmap();
        Ok(pixels)
    }
}

/// What to export.
#[derive(Debug, Clone)]
pub struct Export {
    /// Resolution and frame rate.
    pub quality: Quality,
    /// Output file; its extension picks the format: `mp4`, `webm`, `gif`, `png` or `svg`.
    /// For `png` and `svg` with more than one frame it names a directory of numbered frames.
    pub output: PathBuf,
    /// First and last time to render, in seconds.
    pub range: (f32, f32),
    /// Encode on the GPU with VAAPI; mp4 only.
    pub hw: bool,
    /// Convert frames to yuv420p on the GPU before readback; mp4 and webm only.
    pub gpu_yuv: bool,
}

/// Renders `tl` over `ex.range` and writes it to `ex.output`.
pub fn export(tl: &BakedTimeline, ex: &Export) -> Result<(), String> {
    let times: Vec<f32> = frame_times(ex.range.0, ex.range.1, ex.quality.fps).collect();
    let ext = ex
        .output
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    let still = times.len() == 1;
    if ext == "svg" {
        // SVG bypasses the GPU and ffmpeg entirely.
        return write_svgs(tl, &times, &ex.output, still).map_err(|e| e.to_string());
    }
    if ex.hw && ext != "mp4" {
        return Err("--hw only supports mp4 output".into());
    }
    if ex.gpu_yuv && !matches!(ext, "mp4" | "webm") {
        return Err("--gpu-yuv only supports mp4 and webm output".into());
    }
    // shortcut: first render node only; add a device option if multi-GPU machines need it.
    let device: &[&str] = if ex.hw {
        &["-vaapi_device", "/dev/dri/renderD128"]
    } else {
        &[]
    };
    let codec: &[&str] = match ext {
        // Convert to nv12 on the CPU: scale_vaapi's conversion shifts colours from libx264's.
        // QP 18 matches libx264 CRF 18 in SSIM, at about 1.7x the file size.
        "mp4" if ex.hw => &[
            "-vf",
            "format=nv12,hwupload",
            "-c:v",
            "h264_vaapi",
            "-rc_mode",
            "CQP",
            "-qp",
            "18",
        ],
        "mp4" => &["-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18"],
        "webm" => &["-c:v", "libvpx-vp9", "-b:v", "0", "-crf", "30"],
        "gif" => &["-vf", "split[a][b];[a]palettegen[p];[b][p]paletteuse"],
        "png" if still => &["-frames:v", "1", "-update", "1"],
        "png" => &[],
        _ => return Err(format!("unsupported output `{}`", ex.output.display())),
    };
    let target = if ext == "png" && !still {
        std::fs::create_dir_all(&ex.output).map_err(|e| e.to_string())?;
        ex.output.join("%04d.png")
    } else {
        ex.output.clone()
    };

    let mut renderer = FrameRenderer::new(ex.quality.width, ex.quality.height, ex.gpu_yuv)?;
    let q = ex.quality;
    let mut ffmpeg = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "rawvideo"])
        .args(["-pix_fmt", if ex.gpu_yuv { "yuv420p" } else { "rgba" }])
        .args(device)
        .args(["-s", &format!("{}x{}", q.width, q.height)])
        .args(["-r", &q.fps.to_string(), "-i", "-"])
        .args(codec)
        .arg(&target)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start ffmpeg: {e}"))?;
    let mut stdin = ffmpeg.stdin.take().expect("piped stdin");
    let start = Instant::now();
    // Pipeline: `eval` (CPU-bound, pure) fans out across cores, the GPU renders on this thread in
    // frame order, and a writer thread feeds ffmpeg. Bounded channels cap frames in flight.
    let workers = thread::available_parallelism().map_or(1, |n| n.get());
    let (times, total) = (&times, times.len());
    let rendered = thread::scope(|s| {
        let states: Vec<_> = (0..workers)
            .map(|k| {
                let (tx, rx) = mpsc::sync_channel(2);
                s.spawn(move || {
                    for &t in times.iter().skip(k).step_by(workers) {
                        // The receiver hangs up on a render error; stop quietly.
                        if tx.send(tl.eval(t)).is_err() {
                            break;
                        }
                    }
                });
                rx
            })
            .collect();
        let (px_tx, px_rx) = mpsc::sync_channel::<Vec<u8>>(4);
        let writer = s.spawn(move || {
            for (i, pixels) in px_rx.into_iter().enumerate() {
                stdin
                    .write_all(&pixels)
                    .map_err(|e| format!("ffmpeg: {e}"))?;
                progress(i + 1, total, start);
            }
            Ok::<_, String>(())
        });
        for i in 0..total {
            let state = states[i % workers].recv().map_err(|e| e.to_string())?;
            if px_tx.send(renderer.render(&state)?).is_err() {
                break; // writer failed; its error is reported below
            }
        }
        drop(px_tx);
        writer.join().expect("writer thread panicked")
    });
    rendered?;
    let status = ffmpeg.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("ffmpeg failed: {status}"));
    }
    Ok(())
}

fn write_svgs(tl: &BakedTimeline, times: &[f32], out: &Path, still: bool) -> io::Result<()> {
    let bg = fastanim_core::color::BLACK;
    if still {
        return std::fs::write(out, to_svg(&tl.eval(times[0]), bg));
    }
    std::fs::create_dir_all(out)?;
    let start = Instant::now();
    for (i, &t) in times.iter().enumerate() {
        std::fs::write(out.join(format!("{i:04}.svg")), to_svg(&tl.eval(t), bg))?;
        progress(i + 1, times.len(), start);
    }
    Ok(())
}

/// Redraws `[####----] done/total  eta` on stderr after each frame, if stderr is a terminal and
/// there's more than one frame.
fn progress(done: usize, total: usize, start: Instant) {
    if total < 2 || !io::stderr().is_terminal() {
        return;
    }
    const WIDTH: usize = 30;
    let filled = WIDTH * done / total;
    let eta = start.elapsed().as_secs_f32() * (total - done) as f32 / done as f32;
    eprint!(
        "\r[{}{}] {done}/{total}  eta {eta:.0}s ",
        "#".repeat(filled),
        "-".repeat(WIDTH - filled)
    );
    if done == total {
        eprintln!();
    }
}
