//! Window surface: video drawn first, egui on top, in one submission.
//!
//! egui only re-runs on input or when it asks for a repaint; a new video frame re-draws the
//! last egui output, so 60 fps video doesn't mean 60 egui layouts per second.

use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use cometdesk_view::{Rect, RenderPath, VideoRenderer};
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
    /// egui's vertex/index/uniform buffers need uploading (output or size changed). Video
    /// frames re-draw the same egui output many times; uploading it each time cost CPU.
    egui_buffers_dirty: bool,
    /// Automation asked for the next presented frame (see `app/control.rs`).
    capture: Option<std::sync::mpsc::Sender<(u32, u32, Vec<u8>)>>,
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
                label: Some("cometdesk-gui"),
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | (caps.usages & wgpu::TextureUsages::COPY_SRC),
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
            egui_buffers_dirty: true,
            capture: None,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
        self.egui_buffers_dirty = true;
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
        self.egui_buffers_dirty = true;
        self.paint_jobs =
            ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
    }

    /// Sends the next drawn frame (video and UI) as `(width, height, RGBA)`.
    pub fn request_capture(&mut self, tx: std::sync::mpsc::Sender<(u32, u32, Vec<u8>)>) {
        self.capture = Some(tx);
    }

    /// Reads the frame just drawn into `frame` back to the CPU.
    fn read_back(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        frame: &wgpu::Texture,
    ) -> Option<wgpu::Buffer> {
        if !frame.usage().contains(wgpu::TextureUsages::COPY_SRC) {
            return None;
        }
        let (w, h) = (self.config.width, self.config.height);
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture"),
            size: u64::from(row) * u64::from(h),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            frame.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        Some(buffer)
    }

    /// Waits for `buffer` (from [`Self::read_back`]) and returns tightly packed RGBA.
    fn finish_capture(&self, buffer: &wgpu::Buffer) -> Option<Vec<u8>> {
        let (w, h) = (self.config.width as usize, self.config.height as usize);
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        let data = buffer.slice(..).get_mapped_range().ok()?;
        let bgra = matches!(
            self.config.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        );
        let mut out = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            out.extend_from_slice(&data[y * row..y * row + w * 4]);
        }
        if bgra {
            out.as_chunks_mut::<4>()
                .0
                .iter_mut()
                .for_each(|p| p.swap(0, 2));
        }
        Some(out)
    }

    /// Draws the video into `video_rect` (physical pixels) and egui on top, then presents.
    pub fn render(&mut self, video_rect: Option<Rect>) -> Result<()> {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => {
                self.free_textures();
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
        let mut extra = Vec::new();
        if self.egui_buffers_dirty {
            self.egui_buffers_dirty = false;
            extra = self.egui.update_buffers(
                &self.device,
                &self.queue,
                &mut encoder,
                &self.paint_jobs,
                &screen,
            );
        }
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
        let capture = self.capture.take().and_then(|tx| {
            // A surface that cannot be read back leaves the request to time out.
            self.read_back(&mut encoder, &frame.texture)
                .map(|b| (tx, b))
        });
        extra.push(encoder.finish());
        self.queue.submit(extra);
        if let Some((tx, buffer)) = capture
            && let Some(rgba) = self.finish_capture(&buffer)
        {
            let _ = tx.send((self.config.width, self.config.height, rgba));
        }
        self.queue.present(frame);
        self.free_textures();
        Ok(())
    }

    /// Releases egui textures queued for freeing after the frame that stopped using them.
    fn free_textures(&mut self) {
        for id in self.free_after_render.drain(..) {
            self.egui.free_texture(&id);
        }
    }
}
