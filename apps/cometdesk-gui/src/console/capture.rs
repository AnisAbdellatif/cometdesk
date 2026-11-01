//! Device snapshots, agent screenshots, region selection and screen OCR.
//! Entry points: `Console::snapshot`, `Console::exact_screenshot`, `Console::pick_region`, `Console::copy_screen_text`.

use cometdesk::api::streamer::SnapshotOptions;
use cometdesk_clip::SessionState;

use super::state::RegionPick;
use super::{CAPTURE_REPLY_TIMEOUT, Console};
use crate::app::UserEvent;
use crate::theme;

impl Console {
    /// `~/Downloads/cometdesk-<host>-<time>.<ext>`.
    fn download_path(&self, ext: &str) -> std::path::PathBuf {
        let dir = std::env::var_os("HOME")
            .map(|h| std::path::PathBuf::from(h).join("Downloads"))
            .unwrap_or_else(std::env::temp_dir);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        dir.join(format!(
            "cometdesk-{}-{stamp}.{ext}",
            self.device.host.replace(':', "_")
        ))
    }

    async fn save_notice(what: &str, path: &std::path::Path, bytes: &[u8]) -> String {
        match tokio::fs::write(path, bytes).await {
            Ok(()) => format!("{what} saved to {}", path.display()),
            Err(e) => format!("Cannot save {what}: {e}"),
        }
    }

    /// The device's snapshot (JPEG of the capture, before H.264).
    pub fn snapshot(&mut self) {
        let session = self.session.clone();
        let proxy = self.proxy.clone();
        let path = self.download_path("jpg");
        self.rt.spawn(async move {
            let msg = match session.snapshot(SnapshotOptions::default()).await {
                Ok(jpeg) => Self::save_notice("Snapshot", &path, &jpeg).await,
                Err(e) => format!("Snapshot failed: {e}"),
            };
            let _ = proxy.send_event(UserEvent::Notice(msg));
        });
    }

    /// A pixel-exact screenshot of the target's desktop (all monitors), taken by the agent.
    pub fn exact_screenshot(&mut self) {
        let Some(rx) = self.agent_capture(None) else {
            return;
        };
        let proxy = self.proxy.clone();
        let path = self.download_path("png");
        self.rt.spawn(async move {
            let msg = match tokio::time::timeout(CAPTURE_REPLY_TIMEOUT, rx).await {
                Ok(Ok(Ok(png))) => Self::save_notice("Screenshot", &path, &png).await,
                Ok(Ok(Err(e))) => format!("Screenshot failed: {e}"),
                Ok(Err(_)) => "Screenshot failed: the agent did not answer".into(),
                Err(_) => "Screenshot failed: the agent timed out".into(),
            };
            let _ = proxy.send_event(UserEvent::Notice(msg));
        });
    }

    fn agent_capture(
        &mut self,
        display: Option<u32>,
    ) -> Option<tokio::sync::oneshot::Receiver<crate::clipsync::CaptureResult>> {
        let rx = self
            .clip
            .as_ref()
            .filter(|c| c.can_insert())
            .map(|c| c.capture(display));
        if rx.is_none() {
            self.notify("No agent connected on the target".into());
        }
        rx
    }

    /// Starts picking a region of the video to copy as an image.
    pub fn pick_region(&mut self) {
        if self.video_pts.is_none() {
            return self.notify("No video to copy from yet".into());
        }
        self.release_all();
        self.region = Some(RegionPick {
            start: None,
            now: egui::Pos2::ZERO,
        });
    }

    /// Dims the video and lets the user drag a rectangle on it.
    pub(super) fn region_overlay(&mut self, ui: &mut egui::Ui) {
        let (Some(pick), Some(area)) = (self.region.as_mut(), self.video_pts) else {
            return;
        };
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.region = None;
            return;
        }
        let resp = ui.interact(area, egui::Id::unique("region_pick"), egui::Sense::drag());
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Crosshair);
        if let Some(p) = resp.interact_pointer_pos() {
            pick.now = p;
            if resp.drag_started() {
                pick.start = Some(p);
            }
        }
        let painter = ui.painter();
        painter.rect_filled(area, 0.0, egui::Color32::from_black_alpha(90));
        let msg = "Drag over the part of the screen to copy · Esc cancels";
        let font_id = egui::FontId::new(13.0, egui::FontFamily::Proportional);
        let galley = painter.layout_no_wrap(msg.to_owned(), font_id, theme::TEXT);
        let pill_size = egui::vec2(galley.size().x + 24.0, 32.0);
        let pill_rect =
            egui::Rect::from_center_size(area.center_top() + egui::vec2(0.0, 24.0), pill_size);
        painter.rect(
            pill_rect,
            egui::CornerRadius::same(8),
            theme::PANEL,
            egui::Stroke::new(1.0, theme::BORDER),
            egui::StrokeKind::Inside,
        );
        painter.galley(
            egui::pos2(
                pill_rect.min.x + 12.0,
                pill_rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            theme::TEXT,
        );
        let Some(start) = pick.start else { return };
        let sel = egui::Rect::from_two_pos(start, pick.now).intersect(area);
        painter.rect_stroke(
            sel,
            0.0,
            egui::Stroke::new(2.0, theme::OK),
            egui::StrokeKind::Inside,
        );
        if resp.drag_stopped() {
            self.region = None;
            if sel.width() >= 4.0 && sel.height() >= 4.0 {
                let to = |p: egui::Pos2| {
                    (
                        (p.x - area.min.x) / area.width(),
                        (p.y - area.min.y) / area.height(),
                    )
                };
                let (a, b) = (to(sel.min), to(sel.max));
                self.copy_screen_image([a.0, a.1, b.0, b.1]);
            }
        }
    }

    /// Copies part of the target's screen, `[left, top, right, bottom]` as fractions of the
    /// video, as an image: cut from the agent's exact screenshot of the main monitor.
    pub fn copy_screen_image(&mut self, region: [f32; 4]) {
        let Some(d) = self.agent().and_then(|(_, i)| {
            (i.session != SessionState::Locked)
                .then(|| i.displays.iter().position(|d| d.primary).unwrap_or(0) as u32)
        }) else {
            return self.notify("Needs the agent connected, with the target unlocked".into());
        };
        let Some(rx) = self.agent_capture(Some(d)) else {
            return;
        };
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            let msg = match tokio::time::timeout(CAPTURE_REPLY_TIMEOUT, rx).await {
                Ok(Ok(Ok(png))) => {
                    let image = tokio::task::spawn_blocking(move || {
                        let (w, h, rgba) = crate::crop::crop_png(&png, region)?;
                        arboard::Clipboard::new()
                            .and_then(|mut c| {
                                c.set_image(arboard::ImageData {
                                    width: w as usize,
                                    height: h as usize,
                                    bytes: rgba.into(),
                                })
                            })
                            .map(|()| (w, h))
                            .map_err(|e| e.to_string())
                    })
                    .await;
                    match image {
                        Ok(Ok((w, h))) => format!("Copied a {w}×{h} image of the screen"),
                        Ok(Err(e)) => format!("Cannot copy the screen: {e}"),
                        Err(e) => format!("Cannot copy the screen: {e}"),
                    }
                }
                Ok(Ok(Err(e))) => format!("Screenshot failed: {e}"),
                Ok(Err(_)) => "Screenshot failed: the agent did not answer".into(),
                Err(_) => "Screenshot failed: the agent timed out".into(),
            };
            let _ = proxy.send_event(UserEvent::Notice(msg));
        });
    }

    /// Recognizes the text on the target's screen and copies it. From the agent's exact
    /// screenshot of the main monitor when it is connected and the session is unlocked,
    /// else from the device snapshot.
    pub fn copy_screen_text(&mut self) {
        let primary = self.agent().and_then(|(_, i)| {
            (i.session != SessionState::Locked)
                .then(|| i.displays.iter().position(|d| d.primary).unwrap_or(0) as u32)
        });
        let exact = primary.and_then(|d| self.clip.as_ref().map(|c| c.capture(Some(d))));
        let session = self.session.clone();
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            let from_device = || async {
                session
                    .snapshot(SnapshotOptions::default())
                    .await
                    .map(|b| b.to_vec())
                    .map_err(|e| format!("Snapshot failed: {e}"))
            };
            let image = match exact {
                Some(rx) => match tokio::time::timeout(CAPTURE_REPLY_TIMEOUT, rx).await {
                    Ok(Ok(Ok(png))) => Ok(png),
                    _ => from_device().await,
                },
                None => from_device().await,
            };
            let msg = match image {
                Err(e) => e,
                Ok(img) => match tokio::task::spawn_blocking(move || crate::ocr::recognize(&img))
                    .await
                {
                    Ok(Ok(text)) if text.trim().is_empty() => "No text found on the screen".into(),
                    Ok(Ok(text)) => {
                        let n = text.chars().count();
                        match arboard::Clipboard::new().and_then(|mut c| c.set_text(text)) {
                            Ok(()) => format!("Copied {n} characters of screen text"),
                            Err(e) => format!("Cannot copy: {e}"),
                        }
                    }
                    Ok(Err(e)) => format!("Text recognition failed: {e}"),
                    Err(e) => format!("Text recognition failed: {e}"),
                },
            };
            let _ = proxy.send_event(UserEvent::Notice(msg));
        });
    }
}
