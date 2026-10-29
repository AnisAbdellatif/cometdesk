//! NV12 video renderer, independent of the window surface.
//!
//! A decoded frame becomes two textures (Y: R8, CbCr: RG8), converted to RGB in the fragment
//! shader and drawn into a rectangle of whatever render pass the caller provides. Two ways to
//! get the textures:
//!
//! - zero-copy (macOS, default): the decoder's IOSurface planes are wrapped as Metal
//!   textures, no pixel data is copied (see `zerocopy`);
//! - upload (fallback): the planes are copied into persistent textures every frame.

use cometdesk_media::vt::VideoFrame;

use crate::input::Rect;

const SHADER: &str = r#"
struct Rot {
    quarter_turns: u32,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(3) var<uniform> rot: Rot;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    // Fullscreen triangle; the viewport does the letterboxing.
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: VsOut;
    out.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    // Position in the displayed (rotated) picture, y down; map back to the source frame.
    let d = vec2<f32>(p.x, 1.0 - p.y);
    switch rot.quarter_turns {
        case 1u: { out.uv = vec2<f32>(1.0 - d.y, d.x); }
        case 2u: { out.uv = vec2<f32>(1.0 - d.x, 1.0 - d.y); }
        case 3u: { out.uv = vec2<f32>(d.y, 1.0 - d.x); }
        default: { out.uv = d; }
    }
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

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("zero-copy rendering is not available on this system")]
    ZeroCopyUnavailable,
}

#[derive(Clone)]
struct Nv12 {
    size: (u32, u32),
    y: wgpu::Texture,
    uv: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

pub struct VideoRenderer {
    rotation: wgpu::Buffer,
    quarter_turns: u32,
    device: wgpu::Device,
    queue: wgpu::Queue,
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

impl VideoRenderer {
    /// `target` is the format of the render pass color attachment (prefer a non-sRGB one:
    /// video samples are already gamma-encoded).
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: wgpu::TextureFormat,
        path: RenderPath,
    ) -> Result<Self, RenderError> {
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
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
                targets: &[Some(target.into())],
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
            RenderPath::Auto | RenderPath::ZeroCopy => crate::zerocopy::TextureCache::new(device),
        };
        #[cfg(not(target_os = "macos"))]
        let zero_copy: Option<()> = None;
        if path == RenderPath::ZeroCopy && zero_copy.is_none() {
            return Err(RenderError::ZeroCopyUnavailable);
        }

        let rotation = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rotation"),
            // 16 bytes: the smallest uniform binding every backend accepts.
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let r = Self {
            rotation,
            quarter_turns: 0,
            device: device.clone(),
            queue: queue.clone(),
            pipeline,
            layout,
            sampler,
            upload: None,
            current: None,
            #[cfg(target_os = "macos")]
            zero_copy,
            zero_copy_failed: false,
        };
        tracing::info!(path = r.path_name(), "video renderer ready");
        Ok(r)
    }

    /// Size of the current frame as displayed (after rotation).
    pub fn video_size(&self) -> Option<(u32, u32)> {
        let (w, h) = self.current.as_ref()?.size;
        Some(if self.quarter_turns % 2 == 1 {
            (h, w)
        } else {
            (w, h)
        })
    }

    /// Rotates the picture clockwise by `degrees` (0, 90, 180 or 270; others count as 0).
    /// Matches the device's `orientation` setting.
    pub fn set_rotation(&mut self, degrees: i32) {
        let turns = match degrees {
            90 => 1,
            180 => 2,
            270 => 3,
            _ => 0,
        };
        if turns != self.quarter_turns {
            self.quarter_turns = turns;
            let mut bytes = [0u8; 16];
            bytes[..4].copy_from_slice(&turns.to_le_bytes());
            self.queue.write_buffer(&self.rotation, 0, &bytes);
        }
    }

    pub fn path_name(&self) -> &'static str {
        #[cfg(target_os = "macos")]
        if self.zero_copy.is_some() && !self.zero_copy_failed {
            return "zero-copy";
        }
        "upload"
    }

    /// Forget the current frame (e.g. after disconnecting).
    pub fn clear(&mut self) {
        self.current = None;
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

    /// Draws the current frame into `rect` (physical pixels) of the pass, if there is one.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, rect: Rect) {
        let Some(nv12) = &self.current else { return };
        if rect.w < 1.0 || rect.h < 1.0 {
            return;
        }
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
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.rotation.as_entire_binding(),
                },
            ],
        })
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
