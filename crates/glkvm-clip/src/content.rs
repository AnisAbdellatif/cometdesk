//! Clipboard content, its wire form, and echo suppression.

use sha2::{Digest, Sha256};

use crate::{Error, MAX_ITEM, Result};

#[derive(Clone, PartialEq, Eq)]
pub enum Content {
    Text(String),
    /// 8-bit RGBA, row-major, `width * height * 4` bytes.
    Image {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
    /// Files copied in a file manager. Never sent as clipboard items: they travel as file
    /// transfers and are put on the other clipboard once they have all arrived.
    Files(Vec<std::path::PathBuf>),
}

impl std::fmt::Debug for Content {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Content::Text(t) => write!(f, "Text({} chars)", t.chars().count()),
            Content::Image { width, height, .. } => write!(f, "Image({width}x{height})"),
            Content::Files(p) => write!(f, "Files({})", p.len()),
        }
    }
}

/// Identity of clipboard content as the user sees it: text with normalized line endings,
/// image pixels (not PNG bytes, which differ between encoders).
pub type ContentKey = [u8; 32];

impl Content {
    pub fn key(&self) -> ContentKey {
        let mut h = Sha256::new();
        match self {
            Content::Text(t) => {
                h.update(b"t");
                h.update(t.replace("\r\n", "\n").as_bytes());
            }
            Content::Image {
                width,
                height,
                rgba,
            } => {
                h.update(b"i");
                h.update(width.to_be_bytes());
                h.update(height.to_be_bytes());
                h.update(rgba);
            }
            Content::Files(paths) => {
                h.update(b"f");
                for p in paths {
                    h.update(p.as_os_str().as_encoded_bytes());
                    h.update(b"\0");
                }
            }
        }
        h.finalize().into()
    }

    pub(crate) fn to_png(&self) -> Result<Vec<u8>> {
        let Content::Image {
            width,
            height,
            rgba,
        } = self
        else {
            unreachable!("to_png on something that is not an image");
        };
        encode_png(*width, *height, rgba)
    }

    pub(crate) fn from_png(data: &[u8]) -> Result<Content> {
        let mut dec = png::Decoder::new(std::io::Cursor::new(data));
        dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = dec.read_info().map_err(|e| Error::Image(e.to_string()))?;
        let (w, h) = (reader.info().width, reader.info().height);
        if (w as usize) * (h as usize) * 4 > MAX_ITEM * 4 {
            return Err(Error::Image("image too large".into()));
        }
        let size = reader
            .output_buffer_size()
            .ok_or_else(|| Error::Image("image too large".into()))?;
        let mut buf = vec![0u8; size];
        let info = reader
            .next_frame(&mut buf)
            .map_err(|e| Error::Image(e.to_string()))?;
        buf.truncate(info.buffer_size());
        let rgba = match info.color_type {
            png::ColorType::Rgba => buf,
            png::ColorType::Rgb => buf
                .chunks(3)
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect(),
            png::ColorType::GrayscaleAlpha => buf
                .chunks(2)
                .flat_map(|p| [p[0], p[0], p[0], p[1]])
                .collect(),
            png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            png::ColorType::Indexed => return Err(Error::Image("palette not expanded".into())),
        };
        Ok(Content::Image {
            width: w,
            height: h,
            rgba,
        })
    }
}

/// Echo suppression: remembers the content last seen on the local clipboard, whether the
/// user put it there or we applied it from the peer, so applying an item doesn't send it
/// back.
#[derive(Debug, Default)]
pub struct Dedup {
    last: Option<ContentKey>,
}

impl Dedup {
    /// The local clipboard holds `key`. Returns true if that is new and should be sent.
    pub fn observe_local(&mut self, key: ContentKey) -> bool {
        if self.last == Some(key) {
            return false;
        }
        self.last = Some(key);
        true
    }

    /// We are about to put `key` on the local clipboard.
    pub fn applied(&mut self, key: ContentKey) {
        self.last = Some(key);
    }

    /// Forget the last content, e.g. after reconnecting, so the current clipboard is
    /// offered again.
    pub fn reset(&mut self) {
        self.last = None;
    }
}

/// 8-bit RGBA to PNG.
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    if rgba.len() != (width as usize) * (height as usize) * 4 {
        return Err(Error::Image(
            "pixel buffer size does not match dimensions".into(),
        ));
    }
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    let mut w = enc
        .write_header()
        .map_err(|e| Error::Image(e.to_string()))?;
    w.write_image_data(rgba)
        .map_err(|e| Error::Image(e.to_string()))?;
    w.finish().map_err(|e| Error::Image(e.to_string()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_key_ignores_line_endings() {
        assert_eq!(
            Content::Text("a\r\nb".into()).key(),
            Content::Text("a\nb".into()).key()
        );
        assert_ne!(
            Content::Text("a".into()).key(),
            Content::Text("b".into()).key()
        );
    }

    #[test]
    fn png_round_trip_keeps_pixels() {
        let img = Content::Image {
            width: 4,
            height: 3,
            rgba: (0..48).map(|i| (i * 5) as u8).collect(),
        };
        let back = Content::from_png(&img.to_png().unwrap()).unwrap();
        assert_eq!(back, img);
        assert_eq!(back.key(), img.key());
    }

    #[test]
    fn rgb_png_becomes_opaque_rgba() {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, 2, 1);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[1, 2, 3, 4, 5, 6]).unwrap();
        w.finish().unwrap();
        let Content::Image { rgba, .. } = Content::from_png(&out).unwrap() else {
            panic!()
        };
        assert_eq!(rgba, vec![1, 2, 3, 255, 4, 5, 6, 255]);
    }

    #[test]
    fn dedup_suppresses_echo() {
        let mut d = Dedup::default();
        let a = Content::Text("a".into()).key();
        let b = Content::Text("b".into()).key();
        assert!(d.observe_local(a));
        assert!(!d.observe_local(a));
        d.applied(b); // from the peer
        assert!(!d.observe_local(b)); // reading it back must not send it again
        assert!(d.observe_local(a)); // user copies "a" again
        d.reset();
        assert!(d.observe_local(a));
    }
}
