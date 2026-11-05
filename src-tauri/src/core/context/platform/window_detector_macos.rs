//! Frontmost on-screen window via the window server list.

use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::{CFDictionaryGetValue, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowListExcludeDesktopElements,
    kCGWindowListOptionOnScreenOnly,
};

use crate::core::context::models::{CaptureError, WindowInfo};

pub struct WindowDetector;

impl WindowDetector {
    /// Frontmost ordinary window. Menu bar, Dock, and wallpaper are skipped.
    pub fn detect() -> Result<WindowInfo, CaptureError> {
        let windows = copy_window_info(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID,
        )
        .ok_or_else(|| CaptureError::WindowDetection("window list unavailable".into()))?;

        for item in windows.iter() {
            let raw = *item;
            if raw.is_null() {
                continue;
            }
            let Some(info) = window_from_dict(raw as CFDictionaryRef) else {
                continue;
            };
            if info.layer != 0 || is_system_owner(&info.owner) {
                continue;
            }
            return Ok(WindowInfo {
                hwnd: info.number as isize,
                pid: info.pid,
                process_name: info.owner,
                title: info.title,
            });
        }

        Err(CaptureError::WindowDetection("no foreground window".into()))
    }
}

struct ListedWindow {
    number: u32,
    pid: u32,
    layer: i64,
    owner: String,
    title: String,
}

fn window_from_dict(dict: CFDictionaryRef) -> Option<ListedWindow> {
    let owner = dict_string(dict, "kCGWindowOwnerName")?;
    let pid = dict_i64(dict, "kCGWindowOwnerPID")?;
    let number = dict_i64(dict, "kCGWindowNumber")?;
    if pid <= 0 || number <= 0 {
        return None;
    }
    Some(ListedWindow {
        number: number as u32,
        pid: pid as u32,
        layer: dict_i64(dict, "kCGWindowLayer").unwrap_or(0),
        owner,
        title: dict_string(dict, "kCGWindowName").unwrap_or_default(),
    })
}

fn dict_string(dict: CFDictionaryRef, key: &str) -> Option<String> {
    dict_value(dict, key)?
        .downcast::<CFString>()
        .map(|s| s.to_string())
}

fn dict_i64(dict: CFDictionaryRef, key: &str) -> Option<i64> {
    dict_value(dict, key)?.downcast::<CFNumber>()?.to_i64()
}

fn dict_value(dict: CFDictionaryRef, key: &str) -> Option<CFType> {
    let key = CFString::new(key);
    let value = unsafe { CFDictionaryGetValue(dict, key.as_concrete_TypeRef().cast()) };
    if value.is_null() {
        None
    } else {
        Some(unsafe { CFType::wrap_under_get_rule(value.cast()) })
    }
}

/// Owners that sit above apps but are not the user's foreground window.
pub(crate) fn is_system_owner(name: &str) -> bool {
    matches!(
        name,
        "Window Server"
            | "Dock"
            | "SystemUIServer"
            | "Control Center"
            | "Notification Center"
            | "Wallpaper"
    )
}

#[cfg(test)]
mod tests {
    use super::is_system_owner;

    #[test]
    fn skips_dock_and_keeps_finder() {
        assert!(is_system_owner("Dock"));
        assert!(!is_system_owner("Finder"));
    }
}
