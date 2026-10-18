//! Shader fills: each [`ShaderFill`](fastanim_core::ShaderFill) is drawn into a texture of its
//! own on the GPU, which Vello then uses as the pixels of an image brush filling the shape.
//!
//! [`encode`](crate::encode) leaves a [`ShaderJob`] per shader fill, pointing at an empty
//! placeholder image; [`ShaderFills::prepare`] renders the jobs and registers each texture as
//! that image's override, before Vello renders the frame.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use bevy::tasks::futures_lite::future::block_on;
use bevy_vello::vello::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use bevy_vello::vello::{self, wgpu};

/// Defines the uniform `shape` and the entry points around the user's `shade`.
const PRELUDE: &str = "
struct Shape { color: vec4<f32>, size: vec2<f32>, res: vec2<f32>, time: f32 }
@group(0) @binding(0) var<uniform> shape: Shape;

@vertex
fn fastanim_vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fastanim_fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return shade(p.xy / shape.res);
}
";

/// Drawn instead of a shader that fails to compile: the solid fill.
const FALLBACK: &str = "fn shade(uv: vec2<f32>) -> vec4<f32> { return shape.color; }";

/// Textures are at most this many pixels a side.
const MAX_SIZE: u32 = 2048;

/// One shader fill to render this frame.
#[derive(Debug, Clone)]
pub struct ShaderJob {
    /// The placeholder the encoded scene draws; the texture overrides it.
    pub image: ImageData,
    /// The user's WGSL.
    pub source: Arc<str>,
    /// `Shape`'s fields, padded to its 48-byte uniform layout.
    pub uniforms: [f32; 12],
}

impl ShaderJob {
    /// A job for mobject `id`'s fill, `size` scene units across and `px_per_unit` pixels per
    /// unit, at scene time `t`.
    pub fn new(
        id: u32,
        source: Arc<str>,
        size: (f64, f64),
        px_per_unit: f64,
        color: [f32; 4],
        t: f32,
    ) -> Self {
        let px = |v: f64| ((v * px_per_unit).ceil() as u32).clamp(1, MAX_SIZE);
        let (w, h) = (px(size.0), px(size.1));
        let [r, g, b, a] = color;
        Self {
            image: ImageData {
                data: placeholder(id, w, h),
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::Alpha,
                width: w,
                height: h,
            },
            source,
            uniforms: [
                r,
                g,
                b,
                a,
                size.0 as f32,
                size.1 as f32,
                w as f32,
                h as f32,
                t,
                0.0,
                0.0,
                0.0,
            ],
        }
    }
}

/// One empty blob per mobject and size, so Vello sees the same image id from frame to frame.
fn placeholder(id: u32, w: u32, h: u32) -> Blob<u8> {
    /// Each mobject's placeholder and its size.
    type Blobs = BTreeMap<u32, (u32, u32, Blob<u8>)>;
    static BLOBS: Mutex<Blobs> = Mutex::new(BTreeMap::new());
    let mut blobs = BLOBS.lock().unwrap();
    match blobs.get(&id) {
        Some((bw, bh, b)) if (*bw, *bh) == (w, h) => b.clone(),
        _ => {
            let b = Blob::new(Arc::new(Vec::new()));
            blobs.insert(id, (w, h, b.clone()));
            b
        }
    }
}

/// Compiled shaders and the textures they last drew into, for one GPU device.
#[derive(Default)]
pub struct ShaderFills {
    /// Pipelines by WGSL source; a source that failed to compile maps to the fallback's.
    pipelines: HashMap<Arc<str>, Arc<wgpu::RenderPipeline>>,
    layout: Option<wgpu::BindGroupLayout>,
    /// Placeholders and the textures overriding them, by image id.
    textures: HashMap<u64, (ImageData, wgpu::Texture)>,
}

impl ShaderFills {
    /// Renders `jobs` and makes each one's texture stand in for its placeholder in `renderer`.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut vello::Renderer,
        jobs: &[ShaderJob],
    ) {
        // Textures nobody drew this frame are dropped, and so are their overrides.
        let old: Vec<u64> = (self.textures.keys())
            .filter(|id| !jobs.iter().any(|j| j.image.data.id() == **id))
            .copied()
            .collect();
        for id in old {
            if let Some((image, _)) = self.textures.remove(&id) {
                renderer.override_image(&image, None);
            }
        }
        if jobs.is_empty() {
            return;
        }
        let mut enc = device.create_command_encoder(&Default::default());
        for job in jobs {
            let pipeline = self.pipeline(device, &job.source);
            let (w, h) = (job.image.width, job.image.height);
            let (_, texture) = self.textures.entry(job.image.data.id()).or_insert_with(|| {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("fastanim shader fill"),
                    size: wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    // Vello's image atlas format, so the texture copies straight in.
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                (job.image.clone(), texture)
            });
            let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("fastanim shape uniforms"),
                size: 48,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bytes: Vec<u8> = job.uniforms.iter().flat_map(|f| f.to_le_bytes()).collect();
            queue.write_buffer(&uniforms, 0, &bytes);
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: self.layout.as_ref().expect("made with the pipeline"),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                }],
            });
            let view = texture.create_view(&Default::default());
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fastanim shader fill"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
            drop(pass);
            let copy = wgpu::TexelCopyTextureInfoBase {
                texture: texture.clone(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            };
            renderer.override_image(&job.image, Some(copy));
        }
        queue.submit([enc.finish()]);
    }

    /// The pipeline for `source`, compiling it on first use; on a compile error, prints it once
    /// and uses the solid-fill fallback.
    fn pipeline(&mut self, device: &wgpu::Device, source: &Arc<str>) -> Arc<wgpu::RenderPipeline> {
        if let Some(p) = self.pipelines.get(source) {
            return p.clone();
        }
        let p = match self.compile(device, source) {
            Ok(p) => Arc::new(p),
            Err(e) => {
                eprintln!("shader fill failed to compile; drawing its solid fill instead:\n{e}");
                let fallback: Arc<str> = FALLBACK.into();
                if source == &fallback {
                    panic!("fallback shader failed: {e}");
                }
                self.pipeline(device, &fallback)
            }
        };
        self.pipelines.insert(source.clone(), p.clone());
        p
    }

    fn compile(
        &mut self,
        device: &wgpu::Device,
        source: &str,
    ) -> Result<wgpu::RenderPipeline, String> {
        let layout = self.layout.get_or_insert_with(|| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("fastanim shape"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            })
        });
        // A bad shader is a validation error, which would otherwise panic in wgpu's handler.
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fastanim shader fill"),
            source: wgpu::ShaderSource::Wgsl(format!("{PRELUDE}\n{source}").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&*layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fastanim shader fill"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("fastanim_vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fastanim_fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::TextureFormat::Rgba8Unorm.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        match block_on(scope.pop()) {
            Some(e) => Err(e.to_string()),
            None => Ok(pipeline),
        }
    }
}
