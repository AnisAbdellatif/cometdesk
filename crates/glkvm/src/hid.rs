//! HID input over the `/api/ws` binary protocol.
//!
//! Every binary message is `[event type][payload]`:
//!
//! | type | payload |
//! |---|---|
//! | 1 key | `[flags][key name, ASCII, ≤ 32 bytes]`, flags bit0 = pressed, bit1 = finish |
//! | 2 mouse button | `[flags][button name]`, flags bit0 = pressed |
//! | 3 absolute move | `[x i16 BE][y i16 BE]`, range −32768..=32767 |
//! | 4 relative move | `[squash][dx i8][dy i8]...` |
//! | 5 wheel | `[squash][dx i8][dy i8]...` |
//!
//! Key names are W3C `KeyboardEvent.code` values (`KeyA`, `ShiftLeft`, ...).

use std::time::{Duration, Instant};

pub const EV_KEY: u8 = 1;
pub const EV_MOUSE_BUTTON: u8 = 2;
pub const EV_MOUSE_MOVE: u8 = 3;
pub const EV_MOUSE_RELATIVE: u8 = 4;
pub const EV_MOUSE_WHEEL: u8 = 5;

const FLAG_PRESSED: u8 = 0b01;
const FLAG_FINISH: u8 = 0b10;
const MAX_NAME: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

impl MouseButton {
    /// Wire name. kvmd calls back/forward "up"/"down".
    pub fn name(self) -> &'static str {
        match self {
            MouseButton::Left => "left",
            MouseButton::Right => "right",
            MouseButton::Middle => "middle",
            MouseButton::Back => "up",
            MouseButton::Forward => "down",
        }
    }
}

fn named(ev: u8, flags: u8, name: &str) -> Vec<u8> {
    debug_assert!(
        name.is_ascii() && name.len() <= MAX_NAME,
        "bad HID name {name:?}"
    );
    let mut v = Vec::with_capacity(2 + name.len());
    v.push(ev);
    v.push(flags);
    v.extend_from_slice(&name.as_bytes()[..name.len().min(MAX_NAME)]);
    v
}

pub fn encode_key(code: &str, pressed: bool) -> Vec<u8> {
    named(EV_KEY, if pressed { FLAG_PRESSED } else { 0 }, code)
}

/// Key event with the `finish` flag (used by kvmd for the last key of a shortcut).
pub fn encode_key_finish(code: &str, pressed: bool) -> Vec<u8> {
    named(
        EV_KEY,
        FLAG_FINISH | if pressed { FLAG_PRESSED } else { 0 },
        code,
    )
}

pub fn encode_button(button: MouseButton, pressed: bool) -> Vec<u8> {
    named(
        EV_MOUSE_BUTTON,
        if pressed { FLAG_PRESSED } else { 0 },
        button.name(),
    )
}

pub fn encode_move(x: i16, y: i16) -> Vec<u8> {
    let mut v = Vec::with_capacity(5);
    v.push(EV_MOUSE_MOVE);
    v.extend_from_slice(&x.to_be_bytes());
    v.extend_from_slice(&y.to_be_bytes());
    v
}

pub fn encode_wheel(dx: i8, dy: i8) -> Vec<u8> {
    vec![EV_MOUSE_WHEEL, 0, dx as u8, dy as u8]
}

pub fn encode_relative(dx: i8, dy: i8) -> Vec<u8> {
    vec![EV_MOUSE_RELATIVE, 0, dx as u8, dy as u8]
}

/// Maps a normalized coordinate (0.0 = left/top edge, 1.0 = right/bottom edge) to the
/// device's absolute range. Values outside 0..=1 are clamped.
pub fn to_abs(n: f32) -> i16 {
    let n = if n.is_nan() { 0.5 } else { n.clamp(0.0, 1.0) };
    (f64::from(n) * 65535.0 - 32768.0).round() as i16
}

/// One input action for the device.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HidCommand {
    Key {
        code: &'static str,
        pressed: bool,
    },
    Button {
        button: MouseButton,
        pressed: bool,
    },
    /// Absolute position in device units (see [`to_abs`]).
    MoveAbs {
        x: i16,
        y: i16,
    },
    Wheel {
        dx: i8,
        dy: i8,
    },
    /// Release every key and button believed to be held.
    ReleaseAll,
}

/// Turns [`HidCommand`]s into wire frames while keeping the stream cheap for the device:
///
/// - absolute moves are rate-limited with a leading edge: a move is sent immediately if the
///   last one is older than `min_move_interval`, otherwise only the newest pending position
///   is sent when the interval expires;
/// - any other event first flushes the pending move, so a click lands where the pointer is;
/// - held keys and buttons are tracked so they can be released on focus loss/disconnect.
#[derive(Debug)]
pub struct Coalescer {
    min_move_interval: Duration,
    pending_move: Option<(i16, i16)>,
    last_move: Option<(i16, i16)>,
    last_move_sent: Option<Instant>,
    held_keys: Vec<&'static str>,
    held_buttons: Vec<MouseButton>,
}

/// 120 Hz: faster than any display the target is shown on, slow enough for kvmd.
pub const DEFAULT_MOVE_INTERVAL: Duration = Duration::from_micros(8_333);

impl Default for Coalescer {
    fn default() -> Self {
        Self::new(DEFAULT_MOVE_INTERVAL)
    }
}

impl Coalescer {
    pub fn new(min_move_interval: Duration) -> Self {
        Self {
            min_move_interval,
            pending_move: None,
            last_move: None,
            last_move_sent: None,
            held_keys: Vec::new(),
            held_buttons: Vec::new(),
        }
    }

    pub fn push(&mut self, cmd: HidCommand, now: Instant, out: &mut Vec<Vec<u8>>) {
        match cmd {
            HidCommand::MoveAbs { x, y } => {
                if self.last_move == Some((x, y)) && self.pending_move.is_none() {
                    return;
                }
                let due = self
                    .last_move_sent
                    .is_none_or(|t| now.saturating_duration_since(t) >= self.min_move_interval);
                if due {
                    self.pending_move = None;
                    self.send_move(x, y, now, out);
                } else {
                    self.pending_move = Some((x, y));
                }
            }
            HidCommand::Key { code, pressed } => {
                self.flush_move(now, out);
                if pressed {
                    if !self.held_keys.contains(&code) {
                        self.held_keys.push(code);
                    }
                } else {
                    self.held_keys.retain(|k| *k != code);
                }
                out.push(encode_key(code, pressed));
            }
            HidCommand::Button { button, pressed } => {
                self.flush_move(now, out);
                if pressed {
                    if !self.held_buttons.contains(&button) {
                        self.held_buttons.push(button);
                    }
                } else {
                    self.held_buttons.retain(|b| *b != button);
                }
                out.push(encode_button(button, pressed));
            }
            HidCommand::Wheel { dx, dy } => {
                self.flush_move(now, out);
                if dx != 0 || dy != 0 {
                    out.push(encode_wheel(dx, dy));
                }
            }
            HidCommand::ReleaseAll => {
                self.flush_move(now, out);
                for code in self.held_keys.drain(..).rev() {
                    out.push(encode_key(code, false));
                }
                for button in self.held_buttons.drain(..).rev() {
                    out.push(encode_button(button, false));
                }
            }
        }
    }

    /// Sends the pending move if its interval has expired.
    pub fn poll(&mut self, now: Instant, out: &mut Vec<Vec<u8>>) {
        if let Some(deadline) = self.deadline()
            && now >= deadline
        {
            self.flush_move(now, out);
        }
    }

    /// When [`Coalescer::poll`] should be called next, if a move is pending.
    pub fn deadline(&self) -> Option<Instant> {
        self.pending_move?;
        Some(
            self.last_move_sent
                .map_or_else(Instant::now, |t| t + self.min_move_interval),
        )
    }

    /// Forget held state without sending anything (the connection is gone).
    pub fn reset(&mut self) {
        self.pending_move = None;
        self.last_move = None;
        self.last_move_sent = None;
        self.held_keys.clear();
        self.held_buttons.clear();
    }

    pub fn held_keys(&self) -> &[&'static str] {
        &self.held_keys
    }

    fn flush_move(&mut self, now: Instant, out: &mut Vec<Vec<u8>>) {
        if let Some((x, y)) = self.pending_move.take() {
            self.send_move(x, y, now, out);
        }
    }

    fn send_move(&mut self, x: i16, y: i16, now: Instant, out: &mut Vec<Vec<u8>>) {
        out.push(encode_move(x, y));
        self.last_move = Some((x, y));
        self.last_move_sent = Some(now);
    }
}

/// Turns host scroll deltas into whole wheel steps.
///
/// Mouse wheels report lines (one notch = 1.0); trackpads report many small pixel deltas.
/// Both are accumulated and only whole steps are emitted. A direction change drops the
/// remainder so reversing feels immediate.
#[derive(Debug, Clone)]
pub struct WheelAccumulator {
    /// Logical pixels per step for precise (trackpad) scrolling.
    pub pixels_per_step: f64,
    acc: [f64; 2],
}

impl Default for WheelAccumulator {
    fn default() -> Self {
        Self {
            pixels_per_step: 40.0,
            acc: [0.0; 2],
        }
    }
}

impl WheelAccumulator {
    /// Positive = up/left content movement as winit reports it. Returns whole steps (x, y).
    pub fn push_lines(&mut self, dx: f64, dy: f64) -> (i32, i32) {
        (self.axis(0, dx), self.axis(1, dy))
    }

    pub fn push_pixels(&mut self, dx: f64, dy: f64) -> (i32, i32) {
        let s = self.pixels_per_step;
        (self.axis(0, dx / s), self.axis(1, dy / s))
    }

    pub fn reset(&mut self) {
        self.acc = [0.0; 2];
    }

    fn axis(&mut self, i: usize, d: f64) -> i32 {
        if d == 0.0 || !d.is_finite() {
            return 0;
        }
        if self.acc[i] != 0.0 && self.acc[i].signum() != d.signum() {
            self.acc[i] = 0.0;
        }
        self.acc[i] += d;
        let steps = self.acc[i].trunc();
        self.acc[i] -= steps;
        steps as i32
    }
}

/// Converts wheel steps into a device delta: `steps × rate`, clamped to i8.
pub fn wheel_delta(steps: i32, rate: i32) -> i8 {
    (steps.saturating_mul(rate)).clamp(-127, 127) as i8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_frames() {
        assert_eq!(encode_key("KeyA", true), b"\x01\x01KeyA");
        assert_eq!(encode_key("ShiftLeft", false), b"\x01\x00ShiftLeft");
        assert_eq!(encode_key_finish("Delete", true), b"\x01\x03Delete");
    }

    #[test]
    fn button_frames() {
        assert_eq!(encode_button(MouseButton::Left, true), b"\x02\x01left");
        assert_eq!(encode_button(MouseButton::Back, false), b"\x02\x00up");
        assert_eq!(encode_button(MouseButton::Forward, true), b"\x02\x01down");
    }

    #[test]
    fn move_and_wheel_frames() {
        assert_eq!(encode_move(-32768, 32767), [3, 0x80, 0x00, 0x7f, 0xff]);
        assert_eq!(encode_move(1, -2), [3, 0, 1, 0xff, 0xfe]);
        assert_eq!(encode_wheel(0, -5), [5, 0, 0, 0xfb]);
        assert_eq!(encode_relative(127, -127), [4, 0, 0x7f, 0x81]);
    }

    #[test]
    fn abs_mapping_covers_full_range() {
        assert_eq!(to_abs(0.0), -32768);
        assert_eq!(to_abs(1.0), 32767);
        assert_eq!(to_abs(-3.0), -32768);
        assert_eq!(to_abs(9.0), 32767);
        assert!(to_abs(0.5).abs() <= 1);
        assert_eq!(to_abs(f32::NAN), to_abs(0.5));
    }

    #[test]
    fn abs_mapping_is_monotonic() {
        let mut prev = i16::MIN;
        for i in 0..=1000 {
            let v = to_abs(i as f32 / 1000.0);
            assert!(v >= prev);
            prev = v;
        }
    }

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn first_move_is_sent_immediately() {
        let mut c = Coalescer::default();
        let mut out = vec![];
        c.push(HidCommand::MoveAbs { x: 1, y: 2 }, t0(), &mut out);
        assert_eq!(out, vec![encode_move(1, 2)]);
        assert!(c.deadline().is_none());
    }

    #[test]
    fn moves_within_interval_collapse_to_latest() {
        let mut c = Coalescer::new(Duration::from_millis(8));
        let start = t0();
        let mut out = vec![];
        c.push(HidCommand::MoveAbs { x: 0, y: 0 }, start, &mut out);
        for i in 1..=10 {
            c.push(
                HidCommand::MoveAbs { x: i, y: i },
                start + Duration::from_millis(1),
                &mut out,
            );
        }
        assert_eq!(out.len(), 1);
        assert_eq!(c.deadline(), Some(start + Duration::from_millis(8)));
        c.poll(start + Duration::from_millis(7), &mut out);
        assert_eq!(out.len(), 1);
        c.poll(start + Duration::from_millis(8), &mut out);
        assert_eq!(out, vec![encode_move(0, 0), encode_move(10, 10)]);
        assert!(c.deadline().is_none());
    }

    #[test]
    fn duplicate_position_is_not_resent() {
        let mut c = Coalescer::new(Duration::ZERO);
        let mut out = vec![];
        c.push(HidCommand::MoveAbs { x: 5, y: 5 }, t0(), &mut out);
        c.push(HidCommand::MoveAbs { x: 5, y: 5 }, t0(), &mut out);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn click_flushes_pending_move_first() {
        let mut c = Coalescer::new(Duration::from_secs(1));
        let start = t0();
        let mut out = vec![];
        c.push(HidCommand::MoveAbs { x: 0, y: 0 }, start, &mut out);
        c.push(HidCommand::MoveAbs { x: 9, y: 9 }, start, &mut out);
        c.push(
            HidCommand::Button {
                button: MouseButton::Left,
                pressed: true,
            },
            start,
            &mut out,
        );
        assert_eq!(
            out,
            vec![
                encode_move(0, 0),
                encode_move(9, 9),
                encode_button(MouseButton::Left, true)
            ]
        );
    }

    #[test]
    fn release_all_releases_held_keys_and_buttons() {
        let mut c = Coalescer::default();
        let now = t0();
        let mut out = vec![];
        c.push(
            HidCommand::Key {
                code: "ShiftLeft",
                pressed: true,
            },
            now,
            &mut out,
        );
        c.push(
            HidCommand::Key {
                code: "KeyA",
                pressed: true,
            },
            now,
            &mut out,
        );
        c.push(
            HidCommand::Key {
                code: "KeyA",
                pressed: false,
            },
            now,
            &mut out,
        );
        c.push(
            HidCommand::Key {
                code: "ControlLeft",
                pressed: true,
            },
            now,
            &mut out,
        );
        c.push(
            HidCommand::Button {
                button: MouseButton::Right,
                pressed: true,
            },
            now,
            &mut out,
        );
        out.clear();
        c.push(HidCommand::ReleaseAll, now, &mut out);
        assert_eq!(
            out,
            vec![
                encode_key("ControlLeft", false),
                encode_key("ShiftLeft", false),
                encode_button(MouseButton::Right, false),
            ]
        );
        out.clear();
        c.push(HidCommand::ReleaseAll, now, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn repeated_press_tracked_once() {
        let mut c = Coalescer::default();
        let mut out = vec![];
        c.push(
            HidCommand::Key {
                code: "KeyB",
                pressed: true,
            },
            t0(),
            &mut out,
        );
        c.push(
            HidCommand::Key {
                code: "KeyB",
                pressed: true,
            },
            t0(),
            &mut out,
        );
        assert_eq!(c.held_keys(), &["KeyB"]);
    }

    #[test]
    fn zero_wheel_is_dropped() {
        let mut c = Coalescer::default();
        let mut out = vec![];
        c.push(HidCommand::Wheel { dx: 0, dy: 0 }, t0(), &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn wheel_lines_are_whole_steps() {
        let mut w = WheelAccumulator::default();
        assert_eq!(w.push_lines(0.0, 1.0), (0, 1));
        assert_eq!(w.push_lines(0.0, 0.4), (0, 0));
        assert_eq!(w.push_lines(0.0, 0.7), (0, 1));
        assert_eq!(w.push_lines(0.0, -3.0), (0, -3));
    }

    #[test]
    fn trackpad_pixels_accumulate() {
        let mut w = WheelAccumulator {
            pixels_per_step: 40.0,
            ..Default::default()
        };
        let mut total = 0;
        for _ in 0..20 {
            total += w.push_pixels(0.0, 5.0).1;
        }
        assert_eq!(total, 2); // 100 px / 40
    }

    #[test]
    fn direction_change_drops_remainder() {
        let mut w = WheelAccumulator {
            pixels_per_step: 40.0,
            ..Default::default()
        };
        assert_eq!(w.push_pixels(0.0, 39.0).1, 0);
        // Without the reset this would cancel out to 0 steps.
        assert_eq!(w.push_pixels(0.0, -40.0).1, -1);
    }

    #[test]
    fn wheel_delta_scales_and_clamps() {
        assert_eq!(wheel_delta(1, 5), 5);
        assert_eq!(wheel_delta(-2, 5), -10);
        assert_eq!(wheel_delta(100, 5), 127);
        assert_eq!(wheel_delta(-100, 5), -127);
    }
}
