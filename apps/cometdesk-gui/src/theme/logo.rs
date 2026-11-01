//! Embedded application logo PNG, egui texture loader, and window icon RGBA decoder.
//!
//! Entry points: `LOGO_PNG`, `logo_texture`, `logo_rgba`.

pub const LOGO_PNG: &[u8] = include_bytes!("../../assets/logo-256.png");

/// The logo as a texture (decoded from the embedded PNG).
pub fn logo_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(LOGO_PNG))
        .read_info()
        .ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [info.width as usize, info.height as usize],
        &buf[..info.buffer_size()],
    );
    Some(ctx.load_texture("logo", image, egui::TextureOptions::LINEAR))
}

/// RGBA pixels of the logo, for the window icon.
#[cfg(not(target_os = "macos"))]
pub fn logo_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(LOGO_PNG))
        .read_info()
        .ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    Some((buf, info.width, info.height))
}
