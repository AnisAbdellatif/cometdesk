//! Window surface: video drawn first, egui on top, in one submission.
//!
//! egui only re-runs on input or when it asks for a repaint; a new video frame re-draws the
//! last egui output, so 60 fps video doesn't mean 60 egui layouts per second.

use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use glkvm_view::{Rect, RenderPath, VideoRenderer};
use winit::window::Window;

pub struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub video: VideoRenderer,
    egui: egui_wgpu::Renderer,
    paint_jobs: Vec<egui::ClippedPrimitive>,
    pixels_per_point: f32,
    free_after_render: Vec<egui::TextureId>,
}

impl Gpu {
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
                label: Some("glkvm-gui"),
                ..Default::default()
            })
            .await
            .context("request device")?;
        let caps = surface.get_capabilities(&adapter);
        // Non-sRGB target: video samples are already gamma-encoded; egui handles either.
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
        let video = VideoRenderer::new(&device, &queue, format, path)?;
        let egui = egui_wgpu::Renderer::new(&device, format, egui_wgpu::RendererOptions::default());
        tracing::info!(adapter = %adapter.get_info().name, ?format, ?present_mode, "gpu ready");
        Ok(Self {
            surface,
            device,
            queue,
            config,
            video,
            egui,
            paint_jobs: Vec::new(),
            pixels_per_point: 1.0,
            free_after_render: Vec::new(),
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    pub fn max_texture_side(&self) -> usize {
        self.device.limits().max_texture_dimension_2d as usize
    }

    /// Takes the result of an egui pass; drawn by every following [`Gpu::render`].
    pub fn set_egui(&mut self, ctx: &egui::Context, mut output: egui::FullOutput) {
        // TexturesDelta asserts on drop that it was applied, so empty it as we apply it.
        let delta = &mut output.textures_delta;
        for (id, deltas) in std::mem::take(&mut delta.set) {
            for d in &deltas {
                self.egui.update_texture(&self.device, &self.queue, id, d);
            }
        }
        self.free_after_render
            .extend(std::mem::take(&mut delta.free));
        self.pixels_per_point = output.pixels_per_point;
        self.paint_jobs =
            ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
    }

    /// Draws the video into `video_rect` (physical pixels) and egui on top, then presents.
    pub fn render(&mut self, video_rect: Option<Rect>) -> Result<()> {
        let frame = match self.surface.get_current_texture() {
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
        let view = frame.texture.create_view(&Default::default());
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: self.pixels_per_point,
        };
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let mut extra = self.egui.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &self.paint_jobs,
            &screen,
        );
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("frame"),
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
                })
                .forget_lifetime();
            if let Some(rect) = video_rect {
                self.video.draw(&mut pass, rect);
                // egui sets its own viewport per primitive; reset to the full target first.
                pass.set_viewport(
                    0.0,
                    0.0,
                    self.config.width as f32,
                    self.config.height as f32,
                    0.0,
                    1.0,
                );
            }
            if !self.paint_jobs.is_empty() {
                self.egui.render(&mut pass, &self.paint_jobs, &screen);
            }
        }
        extra.push(encoder.finish());
        self.queue.submit(extra);
        self.queue.present(frame);
        for id in self.free_after_render.drain(..) {
            self.egui.free_texture(&id);
        }
        Ok(())
    }
}
