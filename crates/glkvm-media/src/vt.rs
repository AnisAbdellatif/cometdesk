//! Hardware H.264/H.265 decoding with VideoToolbox.
//!
//! Decoding is synchronous (no `EnableAsynchronousDecompression`): the output callback
//! runs before `decode` returns, which keeps latency minimal and lets us hand frames to
//! the renderer immediately. Output is IOSurface-backed, Metal-compatible NV12
//! (`420v`), so a zero-copy renderer can be added without touching the decoder.

use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::Mutex;

use objc2_core_foundation::{CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_core_media::{
    CMBlockBuffer, CMFormatDescription, CMSampleBuffer, CMTime,
    CMVideoFormatDescriptionCreateFromH264ParameterSets,
    CMVideoFormatDescriptionCreateFromHEVCParameterSets, kCMBlockBufferAssureMemoryNowFlag,
};
use objc2_core_video::{
    CVImageBuffer, CVPixelBuffer, CVPixelBufferGetBaseAddressOfPlane,
    CVPixelBufferGetBytesPerRowOfPlane, CVPixelBufferGetHeight, CVPixelBufferGetHeightOfPlane,
    CVPixelBufferGetWidth, CVPixelBufferGetWidthOfPlane, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress, kCVPixelBufferIOSurfacePropertiesKey,
    kCVPixelBufferMetalCompatibilityKey, kCVPixelBufferPixelFormatTypeKey,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
};
use objc2_video_toolbox::{
    VTDecodeFrameFlags, VTDecodeInfoFlags, VTDecompressionOutputCallbackRecord,
    VTDecompressionSession, VTSessionSetProperty, kVTDecompressionPropertyKey_RealTime,
    kVTVideoDecoderSpecification_EnableHardwareAcceleratedVideoDecoder,
};

use crate::annexb::split_access_unit;
use crate::{Codec, EncodedFrame};

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("waiting for a keyframe with parameter sets")]
    NeedKeyframe,
    #[error("VideoToolbox error {0}")]
    Os(i32),
}

/// A decoded NV12 frame. Holds a retained `CVPixelBuffer`.
pub struct VideoFrame {
    buf: CFRetained<CVPixelBuffer>,
}

// SAFETY: CVPixelBuffer retain/release are thread-safe, and VideoToolbox never writes to
// a buffer after delivering it. Plane access goes through a read-only lock.
unsafe impl Send for VideoFrame {}

/// One plane of a locked frame.
pub struct Plane<'a> {
    pub data: &'a [u8],
    pub stride: usize,
    pub width: usize,
    pub height: usize,
}

impl VideoFrame {
    pub fn width(&self) -> usize {
        CVPixelBufferGetWidth(&self.buf)
    }

    pub fn height(&self) -> usize {
        CVPixelBufferGetHeight(&self.buf)
    }

    /// The underlying pixel buffer (for zero-copy paths).
    pub fn pixel_buffer(&self) -> &CVPixelBuffer {
        &self.buf
    }

    /// Locks the buffer read-only and passes the Y and interleaved CbCr planes.
    pub fn with_planes<R>(&self, f: impl FnOnce(Plane<'_>, Plane<'_>) -> R) -> Option<R> {
        let flags = CVPixelBufferLockFlags::ReadOnly;
        // SAFETY: valid buffer; unlocked below with the same flags.
        if unsafe { CVPixelBufferLockBaseAddress(&self.buf, flags) } != 0 {
            return None;
        }
        let plane = |i: usize| -> Option<Plane<'_>> {
            let base = CVPixelBufferGetBaseAddressOfPlane(&self.buf, i);
            if base.is_null() {
                return None;
            }
            let stride = CVPixelBufferGetBytesPerRowOfPlane(&self.buf, i);
            let height = CVPixelBufferGetHeightOfPlane(&self.buf, i);
            let width = CVPixelBufferGetWidthOfPlane(&self.buf, i);
            // SAFETY: the buffer is locked; the plane spans stride × height bytes.
            let data = unsafe { std::slice::from_raw_parts(base.cast::<u8>(), stride * height) };
            Some(Plane {
                data,
                stride,
                width,
                height,
            })
        };
        let out = match (plane(0), plane(1)) {
            (Some(y), Some(uv)) => Some(f(y, uv)),
            _ => None,
        };
        // SAFETY: matches the lock above.
        unsafe { CVPixelBufferUnlockBaseAddress(&self.buf, flags) };
        out
    }
}

type Sink = Mutex<Box<dyn FnMut(Result<VideoFrame, i32>) + Send>>;

pub struct VtDecoder {
    session: Option<CFRetained<VTDecompressionSession>>,
    format: Option<CFRetained<CMFormatDescription>>,
    codec: Option<Codec>,
    param_sets: Vec<Vec<u8>>,
    avcc: Vec<u8>,
    /// Boxed so its address is stable: VideoToolbox holds it as the callback refcon.
    sink: Box<Sink>,
}

// SAFETY: the session is only used from the thread that owns the decoder; the sink is
// Send and guarded by a mutex.
unsafe impl Send for VtDecoder {}

impl VtDecoder {
    /// `sink` receives every decoded frame (or a per-frame decode error status).
    pub fn new(sink: impl FnMut(Result<VideoFrame, i32>) + Send + 'static) -> Self {
        Self {
            session: None,
            format: None,
            codec: None,
            param_sets: Vec::new(),
            avcc: Vec::with_capacity(512 * 1024),
            sink: Box::new(Mutex::new(Box::new(sink))),
        }
    }

    /// Decodes one access unit. On `Err`, request a keyframe from the source.
    pub fn decode(&mut self, frame: &EncodedFrame) -> Result<(), DecodeError> {
        let split = split_access_unit(frame.codec, &frame.data, std::mem::take(&mut self.avcc));

        if !split.param_sets.is_empty()
            && (self.codec != Some(frame.codec)
                || split.param_sets.len() != self.param_sets.len()
                || split
                    .param_sets
                    .iter()
                    .zip(&self.param_sets)
                    .any(|(a, b)| *a != b.as_slice()))
        {
            self.param_sets = split.param_sets.iter().map(|p| p.to_vec()).collect();
            self.codec = Some(frame.codec);
            self.rebuild()?;
        }

        let result = if split.avcc.is_empty() {
            Ok(())
        } else if self.session.is_none() {
            Err(DecodeError::NeedKeyframe)
        } else {
            self.decode_avcc(&split.avcc)
        };
        self.avcc = split.avcc;
        result
    }

    fn rebuild(&mut self) -> Result<(), DecodeError> {
        self.invalidate();
        let codec = self.codec.expect("codec set before rebuild");
        let mut ptrs: Vec<NonNull<u8>> = Vec::with_capacity(self.param_sets.len());
        let mut sizes: Vec<usize> = Vec::with_capacity(self.param_sets.len());
        for p in &self.param_sets {
            ptrs.push(NonNull::new(p.as_ptr().cast_mut()).ok_or(DecodeError::NeedKeyframe)?);
            sizes.push(p.len());
        }
        let mut fmt: *const CMFormatDescription = ptr::null();
        // SAFETY: pointers/sizes describe live parameter-set buffers for this call.
        let status = unsafe {
            match codec {
                Codec::H264 => CMVideoFormatDescriptionCreateFromH264ParameterSets(
                    None,
                    ptrs.len(),
                    NonNull::new_unchecked(ptrs.as_mut_ptr()),
                    NonNull::new_unchecked(sizes.as_mut_ptr()),
                    4,
                    NonNull::from(&mut fmt),
                ),
                Codec::H265 => CMVideoFormatDescriptionCreateFromHEVCParameterSets(
                    None,
                    ptrs.len(),
                    NonNull::new_unchecked(ptrs.as_mut_ptr()),
                    NonNull::new_unchecked(sizes.as_mut_ptr()),
                    4,
                    None,
                    NonNull::from(&mut fmt),
                ),
            }
        };
        if status != 0 {
            return Err(DecodeError::Os(status));
        }
        // SAFETY: create rule: we own the returned reference.
        let format = unsafe {
            CFRetained::from_raw(NonNull::new(fmt.cast_mut()).ok_or(DecodeError::Os(status))?)
        };

        let true_ = CFBoolean::new(true);
        // SAFETY: framework-provided constant keys.
        let spec = unsafe {
            CFDictionary::<CFString, CFType>::from_slices(
                &[kVTVideoDecoderSpecification_EnableHardwareAcceleratedVideoDecoder],
                &[true_.as_ref()],
            )
        };
        let pixel_format =
            CFNumber::new_i32(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange as i32);
        let empty = CFDictionary::<CFString, CFType>::from_slices(&[], &[]);
        // SAFETY: framework-provided constant keys.
        let attrs = unsafe {
            CFDictionary::<CFString, CFType>::from_slices(
                &[
                    kCVPixelBufferPixelFormatTypeKey,
                    kCVPixelBufferIOSurfacePropertiesKey,
                    kCVPixelBufferMetalCompatibilityKey,
                ],
                &[pixel_format.as_ref(), empty.as_ref(), true_.as_ref()],
            )
        };
        let callback = VTDecompressionOutputCallbackRecord {
            decompressionOutputCallback: Some(output_callback),
            decompressionOutputRefCon: (&*self.sink as *const Sink).cast_mut().cast(),
        };
        let mut session: *mut VTDecompressionSession = ptr::null_mut();
        // SAFETY: all inputs are valid; the refcon outlives the session (invalidated in
        // `invalidate`/`Drop` before the sink is freed).
        let status = unsafe {
            VTDecompressionSession::create(
                None,
                &format,
                Some(spec.as_opaque()),
                Some(attrs.as_opaque()),
                &callback,
                NonNull::from(&mut session),
            )
        };
        if status != 0 {
            return Err(DecodeError::Os(status));
        }
        // SAFETY: create rule.
        let session =
            unsafe { CFRetained::from_raw(NonNull::new(session).ok_or(DecodeError::Os(status))?) };
        // SAFETY: valid session and constant key; RealTime trades power for latency.
        unsafe {
            VTSessionSetProperty(
                &session,
                kVTDecompressionPropertyKey_RealTime,
                Some(true_.as_ref()),
            );
        }
        tracing::info!(?codec, "VideoToolbox session created");
        self.format = Some(format);
        self.session = Some(session);
        Ok(())
    }

    fn decode_avcc(&mut self, avcc: &[u8]) -> Result<(), DecodeError> {
        let (Some(session), Some(format)) = (&self.session, &self.format) else {
            return Err(DecodeError::NeedKeyframe);
        };
        let mut block: *mut CMBlockBuffer = ptr::null_mut();
        // SAFETY: CoreMedia allocates `avcc.len()` bytes; we copy into them below.
        let status = unsafe {
            CMBlockBuffer::create_with_memory_block(
                None,
                ptr::null_mut(),
                avcc.len(),
                None,
                ptr::null(),
                0,
                avcc.len(),
                kCMBlockBufferAssureMemoryNowFlag,
                NonNull::from(&mut block),
            )
        };
        if status != 0 {
            return Err(DecodeError::Os(status));
        }
        // SAFETY: create rule.
        let block =
            unsafe { CFRetained::from_raw(NonNull::new(block).ok_or(DecodeError::Os(status))?) };
        // SAFETY: the block holds exactly avcc.len() bytes.
        let status = unsafe {
            CMBlockBuffer::replace_data_bytes(
                NonNull::new_unchecked(avcc.as_ptr().cast_mut().cast()),
                &block,
                0,
                avcc.len(),
            )
        };
        if status != 0 {
            return Err(DecodeError::Os(status));
        }

        let mut sample: *mut CMSampleBuffer = ptr::null_mut();
        let size = avcc.len();
        // SAFETY: one sample of `size` bytes, no timing info.
        let status = unsafe {
            CMSampleBuffer::create_ready(
                None,
                Some(&block),
                Some(format),
                1,
                0,
                ptr::null(),
                1,
                &size,
                NonNull::from(&mut sample),
            )
        };
        if status != 0 {
            return Err(DecodeError::Os(status));
        }
        // SAFETY: create rule.
        let sample =
            unsafe { CFRetained::from_raw(NonNull::new(sample).ok_or(DecodeError::Os(status))?) };

        let mut info = VTDecodeInfoFlags::empty();
        // SAFETY: synchronous decode of a valid sample; callback fires before return.
        let status = unsafe {
            session.decode_frame(
                &sample,
                VTDecodeFrameFlags::Frame_1xRealTimePlayback,
                ptr::null_mut(),
                &mut info,
            )
        };
        if status != 0 {
            return Err(DecodeError::Os(status));
        }
        Ok(())
    }

    fn invalidate(&mut self) {
        if let Some(session) = self.session.take() {
            // SAFETY: valid session; after this no callbacks reference the sink.
            unsafe {
                session.wait_for_asynchronous_frames();
                session.invalidate();
            }
        }
        self.format = None;
    }
}

impl Drop for VtDecoder {
    fn drop(&mut self) {
        self.invalidate();
    }
}

unsafe extern "C-unwind" fn output_callback(
    refcon: *mut c_void,
    _source_refcon: *mut c_void,
    status: i32,
    _flags: VTDecodeInfoFlags,
    image: *mut CVImageBuffer,
    _pts: CMTime,
    _duration: CMTime,
) {
    // SAFETY: refcon is the boxed sink registered in `rebuild`, alive while the session is.
    let sink = unsafe { &*refcon.cast::<Sink>() };
    let result = match NonNull::new(image) {
        // SAFETY: VideoToolbox passes a valid image buffer; retain to own it.
        Some(img) if status == 0 => Ok(VideoFrame {
            buf: unsafe { CFRetained::retain(img) },
        }),
        _ => Err(if status == 0 { -1 } else { status }),
    };
    if let Ok(mut f) = sink.lock() {
        f(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use std::sync::Arc;
    use std::time::Instant;

    #[test]
    fn delta_frame_without_parameter_sets_needs_keyframe() {
        let mut d = VtDecoder::new(|_| {});
        let frame = EncodedFrame {
            codec: Codec::H264,
            data: Bytes::from_static(&[0, 0, 0, 1, 0x41, 0x9a, 0x02]),
            keyframe: false,
            received_at: Instant::now(),
        };
        assert!(matches!(d.decode(&frame), Err(DecodeError::NeedKeyframe)));
    }

    fn fixture(data: &'static [u8], keyframe: bool) -> EncodedFrame {
        EncodedFrame {
            codec: Codec::H264,
            data: Bytes::from_static(data),
            keyframe,
            received_at: Instant::now(),
        }
    }

    const KEY: &[u8] = include_bytes!("../tests/data/gray64.h264");
    const DELTA: &[u8] = include_bytes!("../tests/data/gray64_p.h264");

    /// 64x64 Constrained Baseline frames (like the device's 42E01F stream), generated by
    /// `tools/gen_h264_fixtures.swift`. The keyframe is gray (Y=64), the delta brighter (Y=128).
    #[test]
    fn decodes_keyframe_then_delta_synchronously() {
        let frames = Arc::new(Mutex::new(Vec::new()));
        let f2 = frames.clone();
        let mut d = VtDecoder::new(move |r: Result<VideoFrame, i32>| {
            let frame = r.expect("decoded frame");
            let luma = frame.with_planes(|y, uv| {
                assert_eq!((y.width, y.height), (64, 64));
                assert_eq!((uv.width, uv.height), (32, 32));
                y.data[y.stride * 32 + 32]
            });
            f2.lock()
                .unwrap()
                .push((frame.width(), frame.height(), luma.unwrap()));
        });
        d.decode(&fixture(KEY, true)).expect("decode key");
        // Synchronous decode: the frame is out before decode() returns.
        assert_eq!(frames.lock().unwrap().len(), 1);
        d.decode(&fixture(DELTA, false)).expect("decode delta");
        let got = frames.lock().unwrap();
        assert_eq!(got.len(), 2);
        let (w, h, y0) = got[0];
        let (_, _, y1) = got[1];
        assert_eq!((w, h), (64, 64));
        assert!((y0 as i32 - 64).abs() <= 3, "key luma {y0}");
        assert!((y1 as i32 - 128).abs() <= 3, "delta luma {y1}");
    }

    #[test]
    fn same_parameter_sets_reuse_the_session() {
        let count = Arc::new(Mutex::new(0));
        let c2 = count.clone();
        let mut d = VtDecoder::new(move |r| {
            r.unwrap();
            *c2.lock().unwrap() += 1;
        });
        d.decode(&fixture(KEY, true)).unwrap();
        let first = d.session.as_ref().map(CFRetained::as_ptr);
        d.decode(&fixture(KEY, true)).unwrap();
        let second = d.session.as_ref().map(CFRetained::as_ptr);
        assert_eq!(first, second);
        assert_eq!(*count.lock().unwrap(), 2);
    }
}
