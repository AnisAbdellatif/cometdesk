//! Decode thread and one-slot frame mailbox between the network and the UI thread.
//!
//! The `decode` thread feeds VideoToolbox synchronously and stores the newest frame in a
//! mailbox, waking the UI once. If rendering falls behind, older frames are replaced, never
//! queued, so the screen always shows the latest picture. While the window is hidden, frames
//! are not decoded at all and decoding resumes at the next keyframe.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use glkvm_media::EncodedFrame;
use glkvm_media::direct::MediaControl;
use glkvm_media::vt::{DecodeError, VideoFrame, VtDecoder};
use tokio::sync::mpsc;

const KEYFRAME_REQUEST_MIN_INTERVAL: Duration = Duration::from_millis(500);

pub struct Decoded {
    pub frame: VideoFrame,
    /// When the encoded frame arrived from the network.
    pub received_at: Instant,
    pub decoded_at: Instant,
}

struct Shared {
    slot: Mutex<Option<Decoded>>,
    wake_pending: AtomicBool,
    visible: AtomicBool,
    replaced: AtomicU64,
}

/// Handle to the decode thread. The thread ends when the media session is dropped.
pub struct FramePipeline {
    shared: Arc<Shared>,
    control: MediaControl,
}

impl FramePipeline {
    /// Starts decoding `video`. `wake` is called (from the decode thread) when a new frame is
    /// waiting and the UI hasn't been woken since its last [`FramePipeline::take`].
    pub fn spawn(
        mut video: mpsc::Receiver<EncodedFrame>,
        control: MediaControl,
        wake: impl Fn() + Send + 'static,
    ) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            slot: Mutex::new(None),
            wake_pending: AtomicBool::new(false),
            visible: AtomicBool::new(true),
            replaced: AtomicU64::new(0),
        });
        let thread_shared = shared.clone();
        let thread_control = control.clone();
        std::thread::Builder::new()
            .name("decode".into())
            .spawn(move || {
                let shared = thread_shared;
                let control = thread_control;
                let current = Arc::new(Mutex::new(Instant::now()));
                let mut last_key_request = Instant::now() - KEYFRAME_REQUEST_MIN_INTERVAL;
                let mut request_key = || {
                    if last_key_request.elapsed() >= KEYFRAME_REQUEST_MIN_INTERVAL {
                        last_key_request = Instant::now();
                        control.request_keyframe();
                    }
                };

                let sink_shared = shared.clone();
                let sink_current = current.clone();
                let mut decoder = VtDecoder::new(move |result| match result {
                    Ok(frame) => {
                        let received_at = *sink_current.lock().unwrap();
                        let decoded = Decoded {
                            frame,
                            received_at,
                            decoded_at: Instant::now(),
                        };
                        if sink_shared.slot.lock().unwrap().replace(decoded).is_some() {
                            sink_shared.replaced.fetch_add(1, Ordering::Relaxed);
                        }
                        if !sink_shared.wake_pending.swap(true, Ordering::AcqRel) {
                            wake();
                        }
                    }
                    Err(status) => tracing::debug!(status, "frame decode failed"),
                });

                let mut need_key = false;
                while let Some(frame) = video.blocking_recv() {
                    if !shared.visible.load(Ordering::Acquire) {
                        need_key = true;
                        continue;
                    }
                    if need_key && !frame.keyframe {
                        continue;
                    }
                    need_key = false;
                    *current.lock().unwrap() = frame.received_at;
                    match decoder.decode(&frame) {
                        Ok(()) => {}
                        Err(DecodeError::NeedKeyframe) => request_key(),
                        Err(e) => {
                            tracing::warn!(%e, "decode error, requesting keyframe");
                            need_key = true;
                            request_key();
                        }
                    }
                }
                tracing::debug!("decode thread ended");
            })?;
        Ok(Self { shared, control })
    }

    /// Takes the newest decoded frame, if any, and re-arms the wake-up.
    pub fn take(&self) -> Option<Decoded> {
        self.shared.wake_pending.store(false, Ordering::Release);
        self.shared.slot.lock().unwrap().take()
    }

    /// Hidden windows don't decode. Becoming visible asks the device for a keyframe.
    pub fn set_visible(&self, visible: bool) {
        let was = self.shared.visible.swap(visible, Ordering::AcqRel);
        if visible && !was {
            self.control.request_keyframe();
        }
    }

    pub fn is_visible(&self) -> bool {
        self.shared.visible.load(Ordering::Acquire)
    }

    /// Decoded frames that were replaced before the UI took them.
    pub fn replaced(&self) -> u64 {
        self.shared.replaced.load(Ordering::Relaxed)
    }
}
