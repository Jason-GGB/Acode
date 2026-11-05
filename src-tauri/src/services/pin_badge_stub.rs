//! Pin badges follow PixPin and Snipaste, which are Windows hosts.

use tauri::AppHandle;

use crate::models::settings::AppSettings;

/// No pin hosts to watch on this platform.
pub fn start(_app: AppHandle) {}

/// Pin-badge switches have no host on this platform.
pub fn configure(_pixpin_enabled: bool, _snipaste_enabled: bool) {}

/// Pin-badge switches have no host on this platform.
pub fn configure_from_settings(settings: &AppSettings) {
    configure(
        settings.pixpin_pin_ai_enabled,
        settings.snipaste_pin_ai_enabled,
    );
}
