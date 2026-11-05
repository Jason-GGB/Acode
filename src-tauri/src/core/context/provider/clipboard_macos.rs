//! Selected text via Command-C, then the previous pasteboard is put back.
//!
//! Terminals are skipped: Command-C there is an interrupt, not a copy.

use std::thread;
use std::time::{Duration, Instant};

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, KeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};

use crate::core::context::image_capture::read_clipboard_image_data_url;
use crate::core::context::models::{CaptureError, CaptureSource, WindowInfo};
use crate::core::context::provider::{CaptureProvider, CaptureResult, PartialCapture};

const CAPTURE_TIMEOUT: Duration = Duration::from_millis(180);
const POLL_INTERVAL: Duration = Duration::from_millis(15);

pub struct ClipboardProvider;

impl ClipboardProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ClipboardProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Screen bounds of a selection. macOS capture places the overlay from the cursor.
pub struct SelectionGeometry {
    pub text_bounds: Option<(i32, i32, i32, i32)>,
    pub editor_left: Option<i32>,
}

/// Selection bounds are not read on macOS; the overlay uses the cursor.
pub fn selected_text_geometry(_hwnd: isize) -> Option<SelectionGeometry> {
    None
}

impl CaptureProvider for ClipboardProvider {
    fn capture(&self, window: &WindowInfo) -> CaptureResult {
        if window.pid == std::process::id() || window.is_terminal() {
            return CaptureResult::Empty;
        }
        match capture_selection() {
            Ok(Some(text)) => CaptureResult::Success(PartialCapture {
                selected_text: Some(text),
                selected_files: Vec::new(),
                selected_images: Vec::new(),
                source: CaptureSource::Clipboard,
            }),
            Ok(None) => CaptureResult::Empty,
            Err(error) => {
                tracing::warn!(provider = "clipboard", error = %error, "context provider failed");
                CaptureResult::Empty
            }
        }
    }
}

fn capture_selection() -> Result<Option<String>, CaptureError> {
    if pasteboard_holds_unreadable_types() {
        return Ok(None);
    }
    let previous = read_text();
    let before = pasteboard_change_count();
    post_command_c().map_err(|_| CaptureError::Clipboard("command-c was not posted".into()))?;

    let started = Instant::now();
    while started.elapsed() < CAPTURE_TIMEOUT {
        if pasteboard_change_count() != before {
            let captured = read_text().filter(|text| !text.trim().is_empty());
            restore_text(previous);
            return Ok(captured);
        }
        thread::sleep(POLL_INTERVAL);
    }
    Ok(None)
}

fn read_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

fn restore_text(previous: Option<String>) {
    let Some(previous) = previous else {
        return;
    };
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(previous);
    }
}

fn pasteboard_holds_unreadable_types() -> bool {
    let count = pasteboard_type_count();
    count > 0 && read_text().is_none() && read_clipboard_image_data_url().is_none()
}

fn pasteboard_change_count() -> isize {
    unsafe {
        let pasteboard: *mut Object = msg_send![class!(NSPasteboard), generalPasteboard];
        if pasteboard.is_null() {
            return 0;
        }
        msg_send![pasteboard, changeCount]
    }
}

fn pasteboard_type_count() -> usize {
    unsafe {
        let pasteboard: *mut Object = msg_send![class!(NSPasteboard), generalPasteboard];
        if pasteboard.is_null() {
            return 0;
        }
        let types: *mut Object = msg_send![pasteboard, types];
        if types.is_null() {
            return 0;
        }
        msg_send![types, count]
    }
}

fn post_command_c() -> Result<(), ()> {
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)?;
    let down = CGEvent::new_keyboard_event(source.clone(), KeyCode::ANSI_C, true)?;
    down.set_flags(CGEventFlags::CGEventFlagCommand);
    down.post(CGEventTapLocation::HID);
    let up = CGEvent::new_keyboard_event(source, KeyCode::ANSI_C, false)?;
    up.set_flags(CGEventFlags::CGEventFlagCommand);
    up.post(CGEventTapLocation::HID);
    Ok(())
}
