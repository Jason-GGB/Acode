//! Finder selection, the macOS counterpart of Explorer's selected files.

use std::path::PathBuf;
use std::process::Command;

use crate::core::context::image_capture::partition_selected_files;
use crate::core::context::models::{CaptureError, CaptureSource, WindowInfo};
use crate::core::context::provider::{CaptureProvider, CaptureResult, PartialCapture};

pub struct ExplorerProvider;

impl ExplorerProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ExplorerProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureProvider for ExplorerProvider {
    fn capture(&self, window: &WindowInfo) -> CaptureResult {
        if !window.is_explorer() {
            return CaptureResult::Empty;
        }
        match finder_selection() {
            Ok(files) if !files.is_empty() => {
                let (selected_files, selected_images) = partition_selected_files(files);
                if selected_files.is_empty() && selected_images.is_empty() {
                    CaptureResult::Empty
                } else {
                    CaptureResult::Success(PartialCapture {
                        selected_text: None,
                        selected_files,
                        selected_images,
                        source: CaptureSource::Explorer,
                    })
                }
            }
            Ok(_) => CaptureResult::Empty,
            Err(error) => {
                tracing::warn!(provider = "finder", error = %error, "context provider failed");
                CaptureResult::Empty
            }
        }
    }
}

fn finder_selection() -> Result<Vec<PathBuf>, CaptureError> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(
            r#"tell application "Finder"
set paths to {}
repeat with f in selection
set end of paths to POSIX path of (f as alias)
end repeat
set AppleScript's text item delimiters to linefeed
return paths as text
end tell"#,
        )
        .output()
        .map_err(|error| CaptureError::Explorer(format!("osascript: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CaptureError::Explorer(stderr.trim().to_string()));
    }
    Ok(parse_finder_paths(&String::from_utf8_lossy(&output.stdout)))
}

/// Split Finder's linefeed-delimited POSIX paths.
pub(crate) fn parse_finder_paths(output: &str) -> Vec<PathBuf> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && *line != "missing value")
        .map(PathBuf::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_finder_paths;

    #[test]
    fn splits_finder_paths_and_skips_an_empty_selection() {
        let paths = parse_finder_paths("/Users/a/notes.txt\n/Users/a/pic.png\n");
        assert_eq!(paths.len(), 2);
        assert!(parse_finder_paths("").is_empty());
        assert!(parse_finder_paths("missing value\n").is_empty());
    }
}
