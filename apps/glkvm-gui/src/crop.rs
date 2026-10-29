//! Cuts a part out of a PNG screenshot.

/// `region` is `[left, top, right, bottom]` as fractions (0..=1) of the image. Returns the
/// cropped size and its RGBA pixels.
pub fn crop_png(png: &[u8], region: [f32; 4]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(png));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("image too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width as usize, info.height as usize);
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => return Err(format!("unsupported screenshot format {other:?}")),
    };
    let px = |f: f32, n: usize| ((f.clamp(0.0, 1.0) * n as f32).round() as usize).min(n);
    let (x0, x1) = (px(region[0], w), px(region[2], w));
    let (y0, y1) = (px(region[1], h), px(region[3], h));
    if x1 <= x0 || y1 <= y0 {
        return Err("the region is empty".into());
    }
    let mut out = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y * w + x) * channels;
            out.extend_from_slice(&buf[i..i + 3]);
            out.push(if channels == 4 { buf[i + 3] } else { 255 });
        }
    }
    Ok(((x1 - x0) as u32, (y1 - y0) as u32, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crops_by_fraction() {
        // 4x2 image whose pixel (x, y) has red = x + 10 * y.
        let rgba: Vec<u8> = (0..2u8)
            .flat_map(|y| (0..4u8).flat_map(move |x| [x + 10 * y, 0, 0, 255]))
            .collect();
        let png = glkvm_clip::encode_png(4, 2, &rgba).unwrap();
        let (w, h, px) = crop_png(&png, [0.25, 0.5, 0.75, 1.0]).unwrap();
        assert_eq!((w, h), (2, 1));
        assert_eq!(px, [11, 0, 0, 255, 12, 0, 0, 255]);
        assert!(crop_png(&png, [0.5, 0.5, 0.5, 1.0]).is_err());
    }
}
