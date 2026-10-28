//! wgpu renderer for NV12 video frames.
//!
//! A decoded NV12 frame becomes two textures (Y: R8, CbCr: RG8), converted to RGB in the
//! fragment shader and drawn into the letterbox rectangle. Two ways to get the textures:
//!
//! - zero-copy (macOS, default): the decoder's IOSurface planes are wrapped as Metal
//!   textures, no pixel data is copied (see `zerocopy`);
//! - upload (fallback): the planes are copied into persistent textures every frame.
//!
//! Rendering happens only when a new frame arrives or the window changes, never continuously.

use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use glkvm_media::vt::VideoFrame;
use winit::window::Window;

use crate::input::Rect;

const SHADER: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    // Fullscreen triangle; the viewport does the letterboxing.
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: VsOut;
    out.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(p.x, 1.0 - p.y);
    return out;
}

@group(0) @binding(0) var tex_y: texture_2d<f32>;
@group(0) @binding(1) var tex_uv: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    // BT.709, video (limited) range.
    let y = (textureSample(tex_y, samp, in.uv).r - 16.0 / 255.0) * (255.0 / 219.0);
    let c = (textureSample(tex_uv, samp, in.uv).rg - 128.0 / 255.0) * (255.0 / 224.0);
    let rgb = vec3<f32>(
        y + 1.5748 * c.y,
        y - 0.1873 * c.x - 0.4681 * c.y,
        y + 1.8556 * c.x,
    );
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderPath {
    /// Zero-copy if available, otherwise upload.
    Auto,
    Upload,
    ZeroCopy,
}

#[derive(Clone)]
struct Nv12 {
    size: (u32, u32),
    y: wgpu::Texture,
    uv: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Persistent textures for the upload path.
    upload: Option<Nv12>,
    /// What the next draw samples from.
    current: Option<Nv12>,
    #[cfg(target_os = "macos")]
    zero_copy: Option<crate::zerocopy::TextureCache>,
    zero_copy_failed: bool,
}

impl Renderer {
    pub async fn new(window: Arc<Window>, path: RenderPath) -> Result<Self> {
        let size = window.inner_size();
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::PRIMARY;
        let instance = wgpu::Instance::new(desc);
        let surface = instance.create_surface(window).context("create surface")?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .context("no GPU adapter")?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("glkvm"),
                ..Default::default()
            })
            .await
            .context("request device")?;

        let caps = surface.get_capabilities(&adapter);
        // Non-sRGB target: video samples are already gamma-encoded.
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .ok_or_else(|| anyhow!("surface has no formats"))?;
        let present_mode = if caps.present_modes.contains(&wgpu::PresentMode::Immediate) {
            wgpu::PresentMode::Immediate
        } else {
            wgpu::PresentMode::Fifo
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            desired_maximum_frame_latency: 1,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);
        tracing::info!(adapter = %adapter.get_info().name, ?format, ?present_mode, "renderer ready");

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("nv12"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nv12"),
            entries: &[
                tex_entry(0),
                tex_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("nv12"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("nv12"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nv12"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        #[cfg(target_os = "macos")]
        let zero_copy = match path {
            RenderPath::Upload => None,
            RenderPath::Auto | RenderPath::ZeroCopy => crate::zerocopy::TextureCache::new(&device),
        };
        #[cfg(not(target_os = "macos"))]
        let zero_copy: Option<()> = None;
        if path == RenderPath::ZeroCopy && zero_copy.is_none() {
            return Err(anyhow!(
                "zero-copy rendering is not available on this system"
            ));
        }

        Ok(Self {
            surface,
            #[cfg(target_os = "macos")]
            zero_copy,
            device,
            queue,
            config,
            pipeline,
            layout,
            sampler,
            upload: None,
            current: None,
            zero_copy_failed: false,
        })
        .inspect(|r: &Self| tracing::info!(path = r.path_name(), "render path"))
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    pub fn surface_size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Video size of the current frame.
    pub fn video_size(&self) -> Option<(u32, u32)> {
        self.current.as_ref().map(|t| t.size)
    }

    pub fn path_name(&self) -> &'static str {
        #[cfg(target_os = "macos")]
        if self.zero_copy.is_some() && !self.zero_copy_failed {
            return "zero-copy";
        }
        "upload"
    }

    /// Makes `frame` the one drawn next. Returns false if it couldn't be used.
    pub fn set_frame(&mut self, frame: &VideoFrame) -> bool {
        #[cfg(target_os = "macos")]
        if let Some(cache) = self.zero_copy.as_ref().filter(|_| !self.zero_copy_failed) {
            if let Some((y, uv)) = cache.textures(&self.device, frame) {
                let size = (frame.width() as u32, frame.height() as u32);
                let bind_group = self.bind(&y, &uv);
                self.current = Some(Nv12 {
                    size,
                    y,
                    uv,
                    bind_group,
                });
                return true;
            }
            tracing::warn!("zero-copy failed, falling back to upload");
            self.zero_copy_failed = true;
        }
        if !self.upload(frame) {
            return false;
        }
        self.current = self.upload.clone();
        true
    }

    /// Copies a decoded frame into the persistent upload textures.
    fn upload(&mut self, frame: &VideoFrame) -> bool {
        let size = (frame.width() as u32, frame.height() as u32);
        if self.upload.as_ref().is_none_or(|t| t.size != size) {
            self.upload = Some(self.create_textures(size));
        }
        let nv12 = self.upload.as_ref().expect("textures created above");
        let queue = &self.queue;
        frame
            .with_planes(|y, uv| {
                write_plane(
                    queue,
                    &nv12.y,
                    y.data,
                    y.stride,
                    y.width as u32,
                    y.height as u32,
                );
                write_plane(
                    queue,
                    &nv12.uv,
                    uv.data,
                    uv.stride,
                    uv.width as u32,
                    uv.height as u32,
                );
            })
            .is_some()
    }

    fn create_textures(&self, (w, h): (u32, u32)) -> Nv12 {
        let make = |label, size: (u32, u32), format| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let y = make("y", (w, h), wgpu::TextureFormat::R8Unorm);
        let uv = make(
            "uv",
            (w.div_ceil(2), h.div_ceil(2)),
            wgpu::TextureFormat::Rg8Unorm,
        );
        let bind_group = self.bind(&y, &uv);
        tracing::info!(w, h, "upload textures created");
        Nv12 {
            size: (w, h),
            y,
            uv,
            bind_group,
        }
    }

    fn bind(&self, y: &wgpu::Texture, uv: &wgpu::Texture) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("nv12"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &y.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &uv.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    /// Draws the current frame into `rect` (black elsewhere) and presents.
    pub fn render(&mut self, rect: Rect) -> Result<()> {
        let texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(anyhow!("surface validation error"));
            }
        };
        let view = texture.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("video"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            if let Some(nv12) = &self.current
                && rect.w >= 1.0
                && rect.h >= 1.0
            {
                pass.set_viewport(
                    rect.x as f32,
                    rect.y as f32,
                    rect.w as f32,
                    rect.h as f32,
                    0.0,
                    1.0,
                );
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &nv12.bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(texture);
        Ok(())
    }
}

fn write_plane(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    data: &[u8],
    stride: usize,
    w: u32,
    h: u32,
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(stride as u32),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
}
