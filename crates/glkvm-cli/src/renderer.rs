//! Window surface for the CLI viewer; video drawing is `glkvm_view::VideoRenderer`.
//!
//! Rendering happens only when a new frame arrives or the window changes, never continuously.

use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use glkvm_media::vt::VideoFrame;
use glkvm_view::{Rect, RenderPath, VideoRenderer};
use winit::window::Window;

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    video: VideoRenderer,
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
        tracing::info!(adapter = %adapter.get_info().name, ?format, ?present_mode, "surface ready");
        let video = VideoRenderer::new(&device, &queue, format, path)?;
        Ok(Self {
            surface,
            device,
            queue,
            config,
            video,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    pub fn surface_size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn video_size(&self) -> Option<(u32, u32)> {
        self.video.video_size()
    }

    pub fn path_name(&self) -> &'static str {
        self.video.path_name()
    }

    pub fn set_frame(&mut self, frame: &VideoFrame) -> bool {
        self.video.set_frame(frame)
    }

    /// Draws the current frame into `rect` (black elsewhere) and presents.
    pub fn render(&mut self, rect: Rect) -> Result<()> {
        let texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
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
            self.video.draw(&mut pass, rect);
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(texture);
        Ok(())
    }
}
