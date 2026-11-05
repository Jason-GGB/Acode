//! Global cursor position in physical pixels, origin at the top-left.

/// Current cursor position, or nothing when the platform cannot read it.
pub fn cursor_pos() -> Option<(i32, i32)> {
    #[cfg(windows)]
    {
        return cursor_pos_windows();
    }
    #[cfg(target_os = "macos")]
    {
        return cursor_pos_macos();
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        None
    }
}

#[cfg(windows)]
fn cursor_pos_windows() -> Option<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut pt = POINT::default();
    unsafe { GetCursorPos(&mut pt).ok().map(|_| (pt.x, pt.y)) }
}

#[cfg(target_os = "macos")]
fn cursor_pos_macos() -> Option<(i32, i32)> {
    use core_graphics::display::CGDisplay;
    use objc::{class, msg_send, sel, sel_impl};

    #[repr(C)]
    struct NsPoint {
        x: f64,
        y: f64,
    }

    let point: NsPoint = unsafe { msg_send![class!(NSEvent), mouseLocation] };
    let display = CGDisplay::main();
    let height = display.bounds().size.height;
    if height <= 0.0 {
        return None;
    }
    let scale = display.pixels_high() as f64 / height;
    let x = (point.x * scale).round() as i32;
    let y = ((height - point.y) * scale).round() as i32;
    Some((x, y))
}
