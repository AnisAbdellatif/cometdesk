//! macOS AppKit window-delegate fullscreen presentation options and Dock icon helper.
//!
//! Entry points: `hide_menu_bar_in_fullscreen`, `set_dock_icon`.

use winit::window::Window;

use crate::theme;

/// In fullscreen macOS slides its menu bar down at the top edge, over the toolbar that appears
/// there too. The options macOS applies for a native fullscreen window come from the window
/// delegate (winit's, which asks for an auto-hiding menu bar; setting them on the app instead
/// is overwritten), so replace its answer with one that hides the menu bar.
pub(super) fn hide_menu_bar_in_fullscreen(window: &Window) {
    use objc2::ffi::class_replaceMethod;
    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2::{msg_send, sel};
    use objc2_app_kit::NSApplicationPresentationOptions as Opts;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    unsafe extern "C" fn options(_: *mut AnyObject, _: Sel, _: *mut AnyObject, _: usize) -> usize {
        (Opts::FullScreen | Opts::HideDock | Opts::HideMenuBar).bits()
    }

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(h) = handle.as_raw() else {
        return;
    };
    // SAFETY: `ns_view` is the live NSView of `window`; it, its window and the delegate are
    // only messaged here, on the main thread, and the replacement matches the delegate
    // method's signature (`NSApplicationPresentationOptions` and `NSUInteger` are both usize).
    unsafe {
        let view = h.ns_view.as_ptr().cast::<AnyObject>();
        let ns_window: *mut AnyObject = msg_send![view, window];
        if ns_window.is_null() {
            return;
        }
        let delegate: *mut AnyObject = msg_send![ns_window, delegate];
        if delegate.is_null() {
            return;
        }
        let class: *const AnyClass = (*delegate).class();
        let imp: Imp = std::mem::transmute(
            options as unsafe extern "C" fn(*mut AnyObject, Sel, *mut AnyObject, usize) -> usize,
        );
        class_replaceMethod(
            class.cast_mut(),
            sel!(window:willUseFullScreenPresentationOptions:),
            imp,
            c"Q@:@Q".as_ptr(),
        );
    }
}

/// Shows the logo in the Dock when running outside an app bundle (a bundle supplies its own).
pub(super) fn set_dock_icon() {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};

    let (Some(nsimage), Some(nsdata), Some(nsapp)) = (
        AnyClass::get(c"NSImage"),
        AnyClass::get(c"NSData"),
        AnyClass::get(c"NSApplication"),
    ) else {
        return;
    };
    let png = theme::LOGO_PNG;
    // SAFETY: plain AppKit calls on the main thread (`resumed` runs there); `dataWithBytes`
    // copies the bytes, and the returned objects are autoreleased or owned by AppKit.
    unsafe {
        let data: *mut AnyObject = msg_send![nsdata, dataWithBytes: png.as_ptr().cast::<std::ffi::c_void>(), length: png.len()];
        let image: *mut AnyObject = msg_send![nsimage, alloc];
        let image: *mut AnyObject = msg_send![image, initWithData: data];
        if image.is_null() {
            return;
        }
        let app: *mut AnyObject = msg_send![nsapp, sharedApplication];
        let _: () = msg_send![app, setApplicationIconImage: image];
    }
}
