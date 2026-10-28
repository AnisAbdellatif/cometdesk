//! Zero-copy video textures on macOS: the decoder's IOSurface-backed NV12 planes are wrapped
//! as Metal textures (via `CVMetalTextureCache`) and handed to wgpu, so no pixel data is
//! copied on the CPU.
//!
//! Lifetime: each wgpu texture owns its `CVMetalTexture` (which retains the pixel buffer)
//! through a drop callback. wgpu only drops a texture after the GPU work using it has
//! finished, so the decoder can't recycle a buffer that is still being sampled.

use std::ptr::{self, NonNull};

use objc2_core_foundation::CFRetained;
use objc2_core_video::{CVMetalTexture, CVMetalTextureCache, CVMetalTextureGetTexture};
use objc2_metal::{MTLPixelFormat, MTLTextureType};
use wgpu::hal::api::Metal;

use glkvm_media::vt::VideoFrame;

pub struct TextureCache {
    cache: CFRetained<CVMetalTextureCache>,
}

/// Keeps a `CVMetalTexture` alive until wgpu releases the texture built on it.
struct Keep(#[allow(dead_code)] CFRetained<CVMetalTexture>);
// SAFETY: CVMetalTexture is a CF object with thread-safe retain/release; it is only dropped.
unsafe impl Send for Keep {}
unsafe impl Sync for Keep {}

impl TextureCache {
    /// Fails if wgpu is not running on Metal.
    pub fn new(device: &wgpu::Device) -> Option<Self> {
        // SAFETY: we only read the raw MTLDevice to create the cache.
        let hal = unsafe { device.as_hal::<Metal>() }?;
        let mtl = hal.raw_device();
        let mut cache: *mut CVMetalTextureCache = ptr::null_mut();
        // SAFETY: valid device, out pointer to a local.
        let status = unsafe {
            CVMetalTextureCache::create(None, None, mtl, None, NonNull::from(&mut cache))
        };
        if status != 0 {
            tracing::warn!(status, "CVMetalTextureCacheCreate failed");
            return None;
        }
        // SAFETY: create rule.
        let cache = unsafe { CFRetained::from_raw(NonNull::new(cache)?) };
        Some(Self { cache })
    }

    /// Wraps the Y (R8) and CbCr (RG8) planes of `frame` as wgpu textures.
    pub fn textures(
        &self,
        device: &wgpu::Device,
        frame: &VideoFrame,
    ) -> Option<(wgpu::Texture, wgpu::Texture)> {
        let (w, h) = (frame.width() as u32, frame.height() as u32);
        let y = self.plane(
            device,
            frame,
            0,
            (w, h),
            MTLPixelFormat::R8Unorm,
            wgpu::TextureFormat::R8Unorm,
        )?;
        let uv = self.plane(
            device,
            frame,
            1,
            (w.div_ceil(2), h.div_ceil(2)),
            MTLPixelFormat::RG8Unorm,
            wgpu::TextureFormat::Rg8Unorm,
        )?;
        // Lets the cache recycle textures of frames that are no longer in use.
        self.cache.flush(0);
        Some((y, uv))
    }

    fn plane(
        &self,
        device: &wgpu::Device,
        frame: &VideoFrame,
        plane: usize,
        (w, h): (u32, u32),
        mtl_format: MTLPixelFormat,
        format: wgpu::TextureFormat,
    ) -> Option<wgpu::Texture> {
        let mut cv: *mut CVMetalTexture = ptr::null_mut();
        // SAFETY: valid cache and IOSurface-backed pixel buffer; plane/size match NV12.
        let status = unsafe {
            CVMetalTextureCache::create_texture_from_image(
                None,
                &self.cache,
                frame.pixel_buffer(),
                None,
                mtl_format,
                w as usize,
                h as usize,
                plane,
                NonNull::from(&mut cv),
            )
        };
        if status != 0 {
            tracing::debug!(
                status,
                plane,
                "CVMetalTextureCacheCreateTextureFromImage failed"
            );
            return None;
        }
        // SAFETY: create rule.
        let cv = unsafe { CFRetained::from_raw(NonNull::new(cv)?) };
        let mtl = CVMetalTextureGetTexture(&cv)?;

        let hal_device = unsafe { device.as_hal::<Metal>() }?;
        let keep = Keep(cv);
        // SAFETY: `mtl` is a 2D texture of the given format/size; `keep` holds its backing
        // CVMetalTexture until wgpu destroys the texture.
        let hal_texture = unsafe {
            let _ = &*hal_device;
            wgpu::hal::metal::Device::texture_from_raw(
                mtl,
                format,
                MTLTextureType::Type2D,
                1,
                1,
                wgpu::hal::CopyExtent {
                    width: w,
                    height: h,
                    depth: 1,
                },
                Some(Box::new(move || drop(keep))),
            )
        };
        let desc = wgpu::TextureDescriptor {
            label: Some(if plane == 0 {
                "y (iosurface)"
            } else {
                "uv (iosurface)"
            }),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            // COPY_SRC lets tests read the planes back; blits from IOSurface textures are fine.
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        };
        // SAFETY: the hal texture was created for this device and matches `desc`.
        Some(unsafe {
            device.create_texture_from_hal::<Metal>(hal_texture, &desc, wgpu::TextureUses::RESOURCE)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use glkvm_media::vt::VtDecoder;
    use glkvm_media::{Codec, EncodedFrame};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    fn decode_fixture() -> VideoFrame {
        let out = Arc::new(Mutex::new(None));
        let o2 = out.clone();
        let mut d = VtDecoder::new(move |r| *o2.lock().unwrap() = Some(r.expect("decode")));
        d.decode(&EncodedFrame {
            codec: Codec::H264,
            data: Bytes::from_static(include_bytes!("../../glkvm-media/tests/data/gray64.h264")),
            keyframe: true,
            received_at: Instant::now(),
        })
        .unwrap();
        out.lock().unwrap().take().expect("frame")
    }

    fn device() -> (wgpu::Device, wgpu::Queue) {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::METAL;
        let instance = wgpu::Instance::new(desc);
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        pollster::block_on(adapter.request_device(&Default::default())).unwrap()
    }

    /// Reads texel (x, y) of a plane, `bpp` bytes per texel.
    fn read_texel(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        tex: &wgpu::Texture,
        bpp: u32,
        (x, y): (u32, u32),
    ) -> Vec<u8> {
        let row = 256; // COPY_BYTES_PER_ROW_ALIGNMENT
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(row),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([enc.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |r| r.unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        
        buf.slice(..).get_mapped_range().unwrap()[..bpp as usize].to_vec()
    }

    #[test]
    fn iosurface_planes_are_sampled_without_copy() {
        let frame = decode_fixture();
        let (device, queue) = device();
        let cache = TextureCache::new(&device).expect("Metal texture cache");
        let (y, uv) = cache.textures(&device, &frame).expect("wrap planes");
        assert_eq!((y.width(), y.height()), (64, 64));
        assert_eq!((uv.width(), uv.height()), (32, 32));
        assert_eq!(y.format(), wgpu::TextureFormat::R8Unorm);
        assert_eq!(uv.format(), wgpu::TextureFormat::Rg8Unorm);

        // Same bytes the CPU sees in the locked pixel buffer.
        let (cpu_y, cpu_uv) = frame
            .with_planes(|py, puv| {
                (
                    py.data[py.stride * 32 + 32],
                    [
                        puv.data[puv.stride * 16 + 32],
                        puv.data[puv.stride * 16 + 33],
                    ],
                )
            })
            .unwrap();
        assert_eq!(read_texel(&device, &queue, &y, 1, (32, 32)), vec![cpu_y]);
        assert_eq!(
            read_texel(&device, &queue, &uv, 2, (16, 16)),
            cpu_uv.to_vec()
        );
        // The fixture is mid-gray: Y≈64, neutral chroma.
        assert!((i32::from(cpu_y) - 64).abs() <= 3, "luma {cpu_y}");
        assert!(
            cpu_uv.iter().all(|c| (i32::from(*c) - 128).abs() <= 3),
            "chroma {cpu_uv:?}"
        );
    }

    #[test]
    fn textures_outlive_the_frame() {
        let (device, queue) = device();
        let cache = TextureCache::new(&device).unwrap();
        let frame = decode_fixture();
        let (y, _uv) = cache.textures(&device, &frame).unwrap();
        drop(frame); // the CVMetalTexture keeps the pixel buffer alive
        let texel = read_texel(&device, &queue, &y, 1, (1, 1));
        assert!((i32::from(texel[0]) - 64).abs() <= 3);
    }
}
