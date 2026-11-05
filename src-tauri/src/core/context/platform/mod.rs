mod cursor;
#[cfg_attr(target_os = "macos", path = "window_detector_macos.rs")]
mod window_detector;

pub use cursor::cursor_pos;
pub use window_detector::WindowDetector;
