//! Screenshots of the target's own screen, on request from the GUI only: never in the
//! background. Pixel-exact (no H.264, no chroma subsampling, native scale), unlike a frame
//! of the video stream, so text is read better from it.
//!
//! Windows: GDI `BitBlt` of the desktop. X11: `GetImage` on the root window. Wayland lets
//! programs capture only through the desktop portal (a permission prompt), which is not done
//! yet. The lock screen is another desktop: nothing to capture there.

use glkvm_clip::{Display, SessionState, encode_png};

/// Rectangle of `display` (index into the info's list) or of all displays together.
fn region(displays: &[Display], display: Option<u32>) -> Result<(i32, i32, u32, u32), String> {
    let picked: Vec<&Display> = match display {
        Some(i) => vec![displays.get(i as usize).ok_or("no such display")?],
        None => displays.iter().collect(),
    };
    if picked.is_empty() {
        return Err("the target reported no displays".into());
    }
    let x0 = picked.iter().map(|d| d.x).min().unwrap();
    let y0 = picked.iter().map(|d| d.y).min().unwrap();
    let x1 = picked.iter().map(|d| d.x + d.width as i32).max().unwrap();
    let y1 = picked.iter().map(|d| d.y + d.height as i32).max().unwrap();
    Ok((x0, y0, (x1 - x0) as u32, (y1 - y0) as u32))
}

/// A PNG of the screen, or why it cannot be taken.
pub fn screenshot(display: Option<u32>) -> Result<Vec<u8>, String> {
    let info = crate::info::read();
    if info.session == SessionState::Locked {
        return Err("the target is locked".into());
    }
    let (x, y, w, h) = region(&info.displays, display)?;
    let rgba = grab(x, y, w, h)?;
    encode_png(w, h, &rgba).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn grab(x: i32, y: i32, w: u32, h: u32) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SRCCOPY, SelectObject,
    };
    const CAPTUREBLT: u32 = 0x4000_0000;
    let mut bgra = vec![0u8; w as usize * h as usize * 4];
    let ok = unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, w as i32, h as i32);
        let old = SelectObject(mem, bmp);
        let mut bi: BITMAPINFO = std::mem::zeroed();
        bi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32), // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let ok = BitBlt(
            mem,
            0,
            0,
            w as i32,
            h as i32,
            screen,
            x,
            y,
            SRCCOPY | CAPTUREBLT,
        ) != 0
            && GetDIBits(
                mem,
                bmp,
                0,
                h,
                bgra.as_mut_ptr().cast(),
                &mut bi,
                DIB_RGB_COLORS,
            ) != 0;
        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
        ReleaseDC(std::ptr::null_mut(), screen);
        ok
    };
    if !ok {
        return Err("Windows refused the screen capture".into());
    }
    for px in bgra.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
        px[3] = 255;
    }
    Ok(bgra)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn grab(x: i32, y: i32, w: u32, h: u32) -> Result<Vec<u8>, String> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ConnectionExt, ImageFormat};
    if std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland") {
        return Err(
            "screen capture on Wayland needs the desktop portal, which is not supported yet".into(),
        );
    }
    let (conn, screen) = x11rb::connect(None).map_err(|e| format!("no X display: {e}"))?;
    let setup = conn.setup();
    let root = setup.roots.get(screen).ok_or("no X screen")?;
    let bpp = setup
        .pixmap_formats
        .iter()
        .find(|f| f.depth == root.root_depth)
        .map(|f| f.bits_per_pixel);
    if bpp != Some(32) {
        return Err(format!(
            "unsupported X pixel format ({bpp:?} bits per pixel)"
        ));
    }
    let reply = conn
        .get_image(
            ImageFormat::Z_PIXMAP,
            root.root,
            x as i16,
            y as i16,
            w as u16,
            h as u16,
            !0,
        )
        .map_err(|e| e.to_string())?
        .reply()
        .map_err(|e| e.to_string())?;
    let mut px = reply.data;
    // Little-endian BGRX with the usual masks.
    for p in px.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
        p[3] = 255;
    }
    Ok(px)
}

#[cfg(target_os = "macos")]
fn grab(_: i32, _: i32, _: u32, _: u32) -> Result<Vec<u8>, String> {
    Err("screen capture is not implemented on macOS targets".into())
}
