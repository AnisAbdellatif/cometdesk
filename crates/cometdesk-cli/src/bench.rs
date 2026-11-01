//! `--bench` CPU and frame-timing measurement for the CLI viewer.
//!
//! Entry point: `Bench` (`Bench::new`, `Bench::restart`, `Bench::frame`).

use std::time::{Duration, Instant};

use cometdesk_media::direct::MediaStats;

/// CPU time used by this process so far (user + system, all threads).
fn process_cpu_time() -> Duration {
    // SAFETY: getrusage writes into the zeroed struct we pass.
    let ru = unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        ru
    };
    let tv = |t: libc::timeval| Duration::new(t.tv_sec as u64, t.tv_usec as u32 * 1000);
    tv(ru.ru_utime) + tv(ru.ru_stime)
}

const BENCH_WARMUP: Duration = Duration::from_secs(3);

/// `--bench`: measures process CPU and per-frame main-thread time over a fixed window.
pub(crate) struct Bench {
    duration: Duration,
    first_frame: Option<Instant>,
    start: Option<(Instant, Duration, MediaStats)>,
    frames: u32,
    frame_work: Duration,
    net_to_screen: Duration,
}

impl Bench {
    pub(crate) fn new(duration: Duration) -> Self {
        Self {
            duration,
            first_frame: None,
            start: None,
            frames: 0,
            frame_work: Duration::ZERO,
            net_to_screen: Duration::ZERO,
        }
    }

    /// The window was hidden: decoding paused, so start over (with a fresh warm-up).
    pub(crate) fn restart(&mut self) {
        if self.start.is_some() {
            tracing::warn!("bench: window hidden, restarting measurement");
        }
        *self = Self::new(self.duration);
    }

    /// Records one presented frame. Returns the report when the window is over.
    pub(crate) fn frame(
        &mut self,
        now: Instant,
        work: Duration,
        net_to_screen: Duration,
        media: &MediaStats,
    ) -> Option<String> {
        let first = *self.first_frame.get_or_insert(now);
        if now - first < BENCH_WARMUP {
            return None;
        }
        let (start, cpu0, start_media) = *self
            .start
            .get_or_insert_with(|| (now, process_cpu_time(), *media));
        self.frames += 1;
        self.frame_work += work;
        self.net_to_screen += net_to_screen;
        let wall = now - start;
        if wall < self.duration {
            return None;
        }
        let cpu = process_cpu_time() - cpu0;
        let n = f64::from(self.frames.max(1));
        let codec = media.codec.map_or("unknown", cometdesk_media::Codec::label);
        let keys = media.keyframes_total - start_media.keyframes_total;
        let key_bytes = media.keyframe_bytes_total - start_media.keyframe_bytes_total;
        let deltas = media.delta_frames_total - start_media.delta_frames_total;
        let delta_bytes = media.delta_bytes_total - start_media.delta_bytes_total;
        let avg_key = key_bytes.checked_div(keys).unwrap_or(0);
        let avg_delta = delta_bytes.checked_div(deltas).unwrap_or(0);
        let kbps = (key_bytes + delta_bytes) as f64 * 8.0 / 1000.0 / wall.as_secs_f64();
        Some(format!(
            "codec={codec} kbps={kbps:.0} keyframes={keys} avg_keyframe={avg_key}B avg_delta={avg_delta}B frames={} fps={:.1} cpu={:.1}% of one core ({:.2} ms CPU/frame) \
             main-thread work/frame={:.3} ms net->screen={:.2} ms",
            self.frames,
            n / wall.as_secs_f64(),
            cpu.as_secs_f64() / wall.as_secs_f64() * 100.0,
            cpu.as_secs_f64() * 1000.0 / n,
            self.frame_work.as_secs_f64() * 1000.0 / n,
            self.net_to_screen.as_secs_f64() * 1000.0 / n,
        ))
    }
}
