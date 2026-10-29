//! Audio from the target: Opus packets from a WebRTC source, decoded and played through the
//! default output device.
//!
//! Only the Pion source carries audio, on its own connection (see [`crate::pion::start_audio`]).
//! The player keeps latency bounded: it starts after a short prebuffer, and drops the oldest
//! samples if the output falls behind rather than letting delay build up.

use std::collections::VecDeque;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use opus_decoder::OpusDecoder;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

/// Opus always decodes to 48 kHz here; the output callback resamples to the device rate.
const RATE: u32 = 48_000;
const CHANNELS: usize = 2;
/// Longest Opus frame: 120 ms.
const MAX_FRAME: usize = (RATE as usize / 1000) * 120;
const PACKET_QUEUE: usize = 64;
/// Playback starts once this much is buffered (and again after an underrun).
const PREBUFFER: Duration = Duration::from_millis(40);
/// Above this much buffered audio the oldest is dropped, down to `PREBUFFER`.
const MAX_BUFFER: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioState {
    Connecting,
    Streaming,
    Disconnected(String),
    Closed,
}

/// Opus payloads as they arrive, one per RTP packet.
pub struct AudioSession {
    pub packets: mpsc::Receiver<Bytes>,
    pub state: watch::Receiver<AudioState>,
    task: JoinHandle<()>,
}

impl AudioSession {
    pub(crate) fn spawn<F, Fut>(task: F) -> AudioSession
    where
        F: FnOnce(mpsc::Sender<Bytes>, watch::Sender<AudioState>) -> Fut,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let (tx, packets) = mpsc::channel(PACKET_QUEUE);
        let (state_tx, state) = watch::channel(AudioState::Connecting);
        let task = tokio::spawn(task(tx, state_tx));
        AudioSession {
            packets,
            state,
            task,
        }
    }

    /// Moves the packet receiver out (to a [`Player`]), keeping the session alive.
    pub fn take_packets(mut self) -> (mpsc::Receiver<Bytes>, AudioSession) {
        let (_, closed) = mpsc::channel(1);
        let packets = std::mem::replace(&mut self.packets, closed);
        (packets, self)
    }
}

impl Drop for AudioSession {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PlayError {
    #[error("no audio output device")]
    NoDevice,
    #[error("audio output: {0}")]
    Device(String),
    #[error("unsupported output sample format {0:?}")]
    Format(cpal::SampleFormat),
}

/// Interleaved stereo at [`RATE`], shared between the decode thread and the output callback.
#[derive(Default)]
struct Ring {
    samples: VecDeque<f32>,
    playing: bool,
}

impl Ring {
    fn push(&mut self, pcm: &[f32]) {
        self.samples.extend(pcm);
        let max = RATE as usize * CHANNELS * MAX_BUFFER.as_millis() as usize / 1000;
        if self.samples.len() > max {
            let keep = RATE as usize * CHANNELS * PREBUFFER.as_millis() as usize / 1000;
            let drop = self.samples.len() - keep;
            self.samples.drain(..drop - drop % CHANNELS);
        }
    }

    fn pop_frame(&mut self) -> Option<[f32; 2]> {
        let prebuffer = RATE as usize * CHANNELS * PREBUFFER.as_millis() as usize / 1000;
        if !self.playing {
            if self.samples.len() < prebuffer {
                return None;
            }
            self.playing = true;
        }
        match (self.samples.pop_front(), self.samples.pop_front()) {
            (Some(l), Some(r)) => Some([l, r]),
            _ => {
                self.playing = false;
                None
            }
        }
    }
}

/// Decodes Opus packets and plays them. Dropping the player stops both.
pub struct Player {
    _stream: cpal::Stream,
    muted: Arc<AtomicBool>,
    _decoder: JoinHandle<()>,
}

impl Player {
    /// Starts playback of `packets` on the default output device. Needs a tokio runtime
    /// context for the decode task.
    pub fn start(mut packets: mpsc::Receiver<Bytes>) -> Result<Player, PlayError> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or(PlayError::NoDevice)?;
        let config = device
            .default_output_config()
            .map_err(|e| PlayError::Device(e.to_string()))?;
        if config.sample_format() != cpal::SampleFormat::F32 {
            return Err(PlayError::Format(config.sample_format()));
        }
        let out_channels = config.channels() as usize;
        let step = RATE as f32 / config.sample_rate() as f32;

        let ring = Arc::new(Mutex::new(Ring::default()));
        let muted = Arc::new(AtomicBool::new(false));

        let cb_ring = ring.clone();
        let cb_muted = muted.clone();
        // Linear resampler state: position between `prev` and `next` source frames.
        let (mut prev, mut next, mut pos) = ([0f32; 2], [0f32; 2], 1.0f32);
        let stream = device
            .build_output_stream(
                config.config(),
                move |out: &mut [f32], _| {
                    let silent = cb_muted.load(Ordering::Relaxed);
                    let mut ring = cb_ring.lock().unwrap();
                    for frame in out.chunks_mut(out_channels) {
                        while pos >= 1.0 {
                            pos -= 1.0;
                            prev = next;
                            match ring.pop_frame() {
                                Some(f) => next = f,
                                None => {
                                    next = [0.0; 2];
                                    pos = 1.0;
                                    break;
                                }
                            }
                        }
                        let l = prev[0] + (next[0] - prev[0]) * pos;
                        let r = prev[1] + (next[1] - prev[1]) * pos;
                        pos += step;
                        let (l, r) = if silent { (0.0, 0.0) } else { (l, r) };
                        match frame.len() {
                            1 => frame[0] = (l + r) * 0.5,
                            _ => {
                                frame[0] = l;
                                frame[1] = r;
                                frame[2..].fill(0.0);
                            }
                        }
                    }
                },
                |e| tracing::warn!(error = %e, "audio output"),
                None,
            )
            .map_err(|e| PlayError::Device(e.to_string()))?;
        stream
            .play()
            .map_err(|e| PlayError::Device(e.to_string()))?;

        let decoder = tokio::task::spawn_blocking(move || {
            let mut dec = match OpusDecoder::new(RATE, CHANNELS) {
                Ok(d) => d,
                Err(e) => return tracing::error!(error = %e, "opus decoder"),
            };
            let mut pcm = vec![0f32; MAX_FRAME * CHANNELS];
            while let Some(packet) = packets.blocking_recv() {
                match dec.decode_float(&packet, &mut pcm, false) {
                    Ok(n) => ring.lock().unwrap().push(&pcm[..n * CHANNELS]),
                    Err(e) => tracing::debug!(error = %e, "dropping undecodable opus packet"),
                }
            }
        });
        Ok(Player {
            _stream: stream,
            muted,
            _decoder: decoder,
        })
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }

    pub fn muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_prebuffers_then_plays_and_rebuffers_after_underrun() {
        let mut r = Ring::default();
        r.push(&[0.5; 100]);
        assert_eq!(r.pop_frame(), None, "below the prebuffer");
        r.push(&vec![0.25; RATE as usize * CHANNELS / 10]);
        assert_eq!(r.pop_frame(), Some([0.5, 0.5]));
        r.samples.clear();
        assert_eq!(r.pop_frame(), None);
        assert!(!r.playing, "underrun goes back to prebuffering");
    }

    #[test]
    fn decodes_a_celt_silence_frame_to_20ms_of_stereo() {
        // TOC 0xFC: CELT fullband, 20 ms, stereo, one frame; a 1-byte frame is silence.
        let mut dec = OpusDecoder::new(RATE, CHANNELS).unwrap();
        let mut pcm = vec![0f32; MAX_FRAME * CHANNELS];
        let n = dec.decode_float(&[0xFC, 0x00], &mut pcm, false).unwrap();
        assert_eq!(n, 960);
    }

    #[test]
    fn ring_caps_latency_keeping_whole_frames() {
        let mut r = Ring::default();
        r.push(&vec![0.1; RATE as usize * CHANNELS]); // one second
        let kept = r.samples.len();
        assert!(kept <= RATE as usize * CHANNELS * 150 / 1000);
        assert_eq!(kept % CHANNELS, 0);
    }
}
