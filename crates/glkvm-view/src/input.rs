//! Host input → device HID commands. Pure logic, no winit event types, so it is testable.

use glkvm::hid::{HidCommand, MouseButton, WheelAccumulator, to_abs, wheel_delta};
use winit::keyboard::KeyCode;

use crate::keymap::{KeyOptions, is_meta, is_modifier, translate};

/// Where the video is drawn inside the window, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Largest rectangle with the video's aspect ratio, centered in the window.
pub fn letterbox(window: (u32, u32), video: (u32, u32)) -> Rect {
    let (ww, wh) = (f64::from(window.0), f64::from(window.1));
    let (vw, vh) = (f64::from(video.0.max(1)), f64::from(video.1.max(1)));
    let scale = (ww / vw).min(wh / vh);
    let (w, h) = (vw * scale, vh * scale);
    Rect {
        x: (ww - w) / 2.0,
        y: (wh - h) / 2.0,
        w,
        h,
    }
}

/// Largest rectangle with the video's aspect ratio, centered in `area`.
pub fn letterbox_in(area: Rect, video: (u32, u32)) -> Rect {
    let size = (
        area.w.max(0.0).round() as u32,
        area.h.max(0.0).round() as u32,
    );
    let r = letterbox(size, video);
    Rect {
        x: area.x + r.x,
        y: area.y + r.y,
        ..r
    }
}

/// Normalized (0..=1) position inside `rect`, or `None` outside it.
pub fn normalize(pos: (f64, f64), rect: Rect) -> Option<(f32, f32)> {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return None;
    }
    let nx = (pos.0 - rect.x) / rect.w;
    let ny = (pos.1 - rect.y) / rect.h;
    ((0.0..=1.0).contains(&nx) && (0.0..=1.0).contains(&ny)).then_some((nx as f32, ny as f32))
}

#[derive(Debug, Clone, Copy)]
pub struct ScrollOptions {
    /// Device wheel units per step (the device's `scroll_rate`).
    pub rate: i32,
    pub reverse_x: bool,
    pub reverse_y: bool,
}

impl Default for ScrollOptions {
    fn default() -> Self {
        Self {
            rate: 1,
            reverse_x: false,
            reverse_y: false,
        }
    }
}

impl ScrollOptions {
    /// From the device's `scroll_rate` and `reverse_scrolling` (STANDARD|VERTICAL|HORIZONTAL|BOTH).
    pub fn from_config(rate: Option<u32>, reverse: Option<&str>) -> Self {
        let r = reverse.unwrap_or("STANDARD");
        Self {
            rate: rate.map_or(1, |r| r.clamp(1, 127) as i32),
            reverse_x: matches!(r, "HORIZONTAL" | "BOTH"),
            reverse_y: matches!(r, "VERTICAL" | "BOTH"),
        }
    }
}

pub struct InputMapper {
    pub keys: KeyOptions,
    pub scroll: ScrollOptions,
    wheel: WheelAccumulator,
    rect: Rect,
    held_keys: Vec<&'static str>,
    /// Non-modifier keys pressed while Cmd was down. macOS drops their key-up while Cmd is
    /// held, so they are released when Cmd goes up.
    under_meta: Vec<&'static str>,
    held_buttons: Vec<MouseButton>,
    inside: bool,
}

impl InputMapper {
    pub fn new(keys: KeyOptions, scroll: ScrollOptions) -> Self {
        Self {
            keys,
            scroll,
            wheel: WheelAccumulator::default(),
            rect: Rect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
            held_keys: Vec::new(),
            under_meta: Vec::new(),
            held_buttons: Vec::new(),
            inside: false,
        }
    }

    pub fn set_layout(&mut self, window: (u32, u32), video: (u32, u32)) {
        self.rect = letterbox(window, video);
    }

    /// Uses an explicit video rectangle (e.g. letterboxed below a toolbar).
    pub fn set_rect(&mut self, rect: Rect) {
        self.rect = rect;
    }

    pub fn rect(&self) -> Rect {
        self.rect
    }

    /// Whether the last cursor position was inside the video.
    pub fn cursor_inside(&self) -> bool {
        self.inside
    }

    pub fn key(&mut self, code: KeyCode, pressed: bool, repeat: bool, out: &mut Vec<HidCommand>) {
        // The target repeats held keys itself, like any USB keyboard.
        if repeat {
            return;
        }
        let Some(name) = translate(code, self.keys) else {
            tracing::debug!(?code, "key not supported by device");
            return;
        };
        // macOS reports Caps Lock as a state toggle (down on enable, up on disable). The
        // target needs a full press for every toggle.
        if code == KeyCode::CapsLock {
            out.push(HidCommand::Key {
                code: name,
                pressed: true,
            });
            out.push(HidCommand::Key {
                code: name,
                pressed: false,
            });
            return;
        }
        if pressed {
            if self.held_keys.contains(&name) {
                return;
            }
            self.held_keys.push(name);
            if !is_modifier(name) && self.held_keys.iter().any(|k| is_meta(k)) {
                self.under_meta.push(name);
            }
            out.push(HidCommand::Key {
                code: name,
                pressed: true,
            });
        } else {
            if !self.held_keys.contains(&name) {
                return;
            }
            self.held_keys.retain(|k| *k != name);
            self.under_meta.retain(|k| *k != name);
            out.push(HidCommand::Key {
                code: name,
                pressed: false,
            });
            if is_meta(name) && !self.held_keys.iter().any(|k| is_meta(k)) {
                for k in self.under_meta.drain(..) {
                    self.held_keys.retain(|h| *h != k);
                    out.push(HidCommand::Key {
                        code: k,
                        pressed: false,
                    });
                }
            }
        }
    }

    /// Cursor position in physical window pixels.
    pub fn cursor(&mut self, pos: (f64, f64), out: &mut Vec<HidCommand>) {
        match normalize(pos, self.rect) {
            Some((nx, ny)) => {
                self.inside = true;
                out.push(HidCommand::MoveAbs {
                    x: to_abs(nx),
                    y: to_abs(ny),
                });
            }
            None => self.inside = false,
        }
    }

    pub fn cursor_left(&mut self) {
        self.inside = false;
    }

    pub fn button(&mut self, button: MouseButton, pressed: bool, out: &mut Vec<HidCommand>) {
        if pressed {
            // A click in the black bars would land at the last in-video position.
            if !self.inside || self.held_buttons.contains(&button) {
                return;
            }
            self.held_buttons.push(button);
        } else {
            if !self.held_buttons.contains(&button) {
                return;
            }
            self.held_buttons.retain(|b| *b != button);
        }
        out.push(HidCommand::Button { button, pressed });
    }

    /// Mouse wheel notches (winit `LineDelta`): positive = up / left.
    pub fn wheel_lines(&mut self, dx: f64, dy: f64, out: &mut Vec<HidCommand>) {
        let steps = self.wheel.push_lines(dx, dy);
        self.emit_wheel(steps, out);
    }

    /// Precise scrolling in logical pixels (winit `PixelDelta` / scale factor).
    pub fn wheel_pixels(&mut self, dx: f64, dy: f64, out: &mut Vec<HidCommand>) {
        let steps = self.wheel.push_pixels(dx, dy);
        self.emit_wheel(steps, out);
    }

    fn emit_wheel(&mut self, (sx, sy): (i32, i32), out: &mut Vec<HidCommand>) {
        if !self.inside || (sx == 0 && sy == 0) {
            return;
        }
        let sx = if self.scroll.reverse_x { -sx } else { sx };
        let sy = if self.scroll.reverse_y { -sy } else { sy };
        out.push(HidCommand::Wheel {
            dx: wheel_delta(sx, self.scroll.rate),
            dy: wheel_delta(sy, self.scroll.rate),
        });
    }

    /// Window lost focus: nothing may stay pressed on the target.
    pub fn focus_lost(&mut self, out: &mut Vec<HidCommand>) {
        self.held_keys.clear();
        self.under_meta.clear();
        self.held_buttons.clear();
        self.wheel.reset();
        out.push(HidCommand::ReleaseAll);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use KeyCode::*;

    fn mapper() -> InputMapper {
        let mut m = InputMapper::new(KeyOptions::default(), ScrollOptions::default());
        m.set_layout((1920, 1080), (1920, 1080));
        m
    }

    fn key(code: &'static str, pressed: bool) -> HidCommand {
        HidCommand::Key { code, pressed }
    }

    #[test]
    fn letterbox_wide_window() {
        let r = letterbox((2000, 1000), (1920, 1080));
        assert!((r.h - 1000.0).abs() < 1e-9);
        assert!((r.w - 1777.777).abs() < 0.01);
        assert!((r.x - 111.111).abs() < 0.01);
        assert_eq!(r.y, 0.0);
    }

    #[test]
    fn letterbox_tall_window() {
        let r = letterbox((1000, 1000), (1600, 900));
        assert_eq!((r.w, r.x), (1000.0, 0.0));
        assert!((r.h - 562.5).abs() < 1e-9);
        assert!((r.y - 218.75).abs() < 1e-9);
    }

    #[test]
    fn letterbox_in_offsets_by_area() {
        let area = Rect {
            x: 0.0,
            y: 40.0,
            w: 1000.0,
            h: 1000.0,
        };
        let r = letterbox_in(area, (1600, 900));
        assert_eq!((r.x, r.w), (0.0, 1000.0));
        assert!((r.y - (40.0 + 218.75)).abs() < 1e-9);
    }

    #[test]
    fn normalize_inside_and_outside() {
        let r = Rect {
            x: 100.0,
            y: 0.0,
            w: 800.0,
            h: 600.0,
        };
        assert_eq!(normalize((100.0, 0.0), r), Some((0.0, 0.0)));
        assert_eq!(normalize((900.0, 600.0), r), Some((1.0, 1.0)));
        assert_eq!(normalize((500.0, 300.0), r), Some((0.5, 0.5)));
        assert_eq!(normalize((99.0, 10.0), r), None);
        assert_eq!(normalize((500.0, 601.0), r), None);
        assert_eq!(
            normalize(
                (0.0, 0.0),
                Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0
                }
            ),
            None
        );
    }

    #[test]
    fn cursor_maps_corners_to_device_range() {
        let mut m = mapper();
        let mut out = vec![];
        m.cursor((0.0, 0.0), &mut out);
        m.cursor((1920.0, 1080.0), &mut out);
        assert_eq!(
            out,
            vec![
                HidCommand::MoveAbs {
                    x: -32768,
                    y: -32768
                },
                HidCommand::MoveAbs { x: 32767, y: 32767 }
            ]
        );
    }

    #[test]
    fn cursor_in_black_bars_is_dropped_and_blocks_clicks() {
        let mut m = InputMapper::new(KeyOptions::default(), ScrollOptions::default());
        m.set_layout((2000, 1000), (1000, 1000)); // bars at x < 500 and x > 1500
        let mut out = vec![];
        m.cursor((100.0, 500.0), &mut out);
        m.button(MouseButton::Left, true, &mut out);
        assert!(out.is_empty());
        m.cursor((1000.0, 500.0), &mut out);
        m.button(MouseButton::Left, true, &mut out);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn release_goes_out_even_after_leaving_the_video() {
        let mut m = mapper();
        let mut out = vec![];
        m.cursor((10.0, 10.0), &mut out);
        m.button(MouseButton::Left, true, &mut out);
        m.cursor_left();
        m.button(MouseButton::Left, false, &mut out);
        assert_eq!(
            out.last(),
            Some(&HidCommand::Button {
                button: MouseButton::Left,
                pressed: false
            })
        );
        // An unmatched release is not sent.
        out.clear();
        m.button(MouseButton::Right, false, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn autorepeat_is_dropped() {
        let mut m = mapper();
        let mut out = vec![];
        m.key(KeyA, true, false, &mut out);
        m.key(KeyA, true, true, &mut out);
        m.key(KeyA, true, true, &mut out);
        m.key(KeyA, false, false, &mut out);
        assert_eq!(out, vec![key("KeyA", true), key("KeyA", false)]);
    }

    #[test]
    fn caps_lock_toggle_is_a_full_press_each_time() {
        let mut m = mapper();
        let mut out = vec![];
        m.key(CapsLock, true, false, &mut out); // enabled
        m.key(CapsLock, false, false, &mut out); // disabled
        assert_eq!(
            out,
            vec![
                key("CapsLock", true),
                key("CapsLock", false),
                key("CapsLock", true),
                key("CapsLock", false)
            ]
        );
    }

    #[test]
    fn lone_cmd_tap_is_sent() {
        let mut m = mapper();
        let mut out = vec![];
        m.key(SuperLeft, true, false, &mut out);
        m.key(SuperLeft, false, false, &mut out);
        assert_eq!(out, vec![key("MetaLeft", true), key("MetaLeft", false)]);
    }

    #[test]
    fn keys_pressed_under_cmd_are_released_with_cmd() {
        // macOS swallows the key-up of C in Cmd+C.
        let mut m = mapper();
        let mut out = vec![];
        m.key(SuperLeft, true, false, &mut out);
        m.key(KeyC, true, false, &mut out);
        m.key(SuperLeft, false, false, &mut out);
        assert_eq!(
            out,
            vec![
                key("MetaLeft", true),
                key("KeyC", true),
                key("MetaLeft", false),
                key("KeyC", false)
            ]
        );
        // A late key-up (if macOS does deliver it) is ignored.
        out.clear();
        m.key(KeyC, false, false, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn normal_release_under_cmd_is_not_doubled() {
        let mut m = mapper();
        let mut out = vec![];
        m.key(SuperLeft, true, false, &mut out);
        m.key(KeyC, true, false, &mut out);
        m.key(KeyC, false, false, &mut out);
        m.key(SuperLeft, false, false, &mut out);
        assert_eq!(out.iter().filter(|c| **c == key("KeyC", false)).count(), 1);
    }

    #[test]
    fn wheel_respects_rate_reverse_and_position() {
        let mut m = mapper();
        m.scroll = ScrollOptions::from_config(Some(5), Some("VERTICAL"));
        let mut out = vec![];
        m.wheel_lines(0.0, 1.0, &mut out);
        assert!(out.is_empty(), "cursor not inside yet");
        m.cursor((5.0, 5.0), &mut out);
        out.clear();
        m.wheel_lines(1.0, 1.0, &mut out);
        assert_eq!(out, vec![HidCommand::Wheel { dx: 5, dy: -5 }]);
    }

    #[test]
    fn scroll_options_from_config() {
        let s = ScrollOptions::from_config(None, None);
        assert_eq!((s.rate, s.reverse_x, s.reverse_y), (1, false, false));
        let s = ScrollOptions::from_config(Some(500), Some("BOTH"));
        assert_eq!((s.rate, s.reverse_x, s.reverse_y), (127, true, true));
    }

    #[test]
    fn focus_loss_releases_everything() {
        let mut m = mapper();
        let mut out = vec![];
        m.cursor((5.0, 5.0), &mut out);
        m.key(ShiftLeft, true, false, &mut out);
        m.button(MouseButton::Left, true, &mut out);
        out.clear();
        m.focus_lost(&mut out);
        assert_eq!(out, vec![HidCommand::ReleaseAll]);
        // State is cleared: pressing again works.
        out.clear();
        m.key(ShiftLeft, true, false, &mut out);
        assert_eq!(out, vec![key("ShiftLeft", true)]);
    }
}
