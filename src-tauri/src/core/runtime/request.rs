use serde::{Deserialize, Serialize};
use std::path::Path;

use super::message::ChatMessage;
use crate::core::context::models::IDEContext;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceContext {
    pub name: String,
    pub root: String,
}

/// Windows 上下文 — 由 ContextResolver 填充。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct RequestContext {
    pub selection: Option<String>,
    pub selected_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_images: Vec<String>,
    pub active_window: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_file: Option<String>,
    pub workspace: Option<WorkspaceContext>,
    pub clipboard: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_shell_execution: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ide_context: Option<IDEContext>,
}

impl RequestContext {
    pub fn set_workspace(&mut self, name: String, root: &Path) {
        self.selected_files = self
            .selected_files
            .iter()
            .filter_map(|selected| relative_selected_file(root, Path::new(selected)))
            .collect();
        self.workspace = Some(WorkspaceContext {
            name,
            root: root.display().to_string(),
        });
    }
}

/// 这条路径在当前系统上是绝对路径，或是 Windows 盘符 / UNC。
pub(crate) fn path_is_absolute(path: &Path) -> bool {
    path.is_absolute() || windows_absolute_text(&slash_form(path))
}

/// Windows 盘符路径是否落在这个根下面。盘符比较忽略大小写。
pub(crate) fn windows_drive_path_starts_with(path: &Path, root: &Path) -> bool {
    let path = slash_form(path).to_ascii_lowercase();
    let root = slash_form(root);
    let root = root.trim_end_matches('/').to_ascii_lowercase();
    if !windows_absolute_text(&path) || !windows_absolute_text(&root) {
        return false;
    }
    path == root
        || path
            .strip_prefix(&root)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn relative_selected_file(root: &Path, selected: &Path) -> Option<String> {
    let normalized_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let normalized_selected =
        std::fs::canonicalize(selected).unwrap_or_else(|_| selected.to_path_buf());
    if let Some(display) = native_relative(&normalized_root, &normalized_selected) {
        return Some(display);
    }
    let selected_text = slash_form(&normalized_selected);
    if let Some(rest) = strip_windows_prefix(&selected_text, &slash_form(&normalized_root)) {
        return Some(rest);
    }
    if windows_absolute_text(&selected_text) {
        return selected_text
            .rsplit('/')
            .find(|part| !part.is_empty())
            .map(str::to_string);
    }
    normalized_selected
        .file_name()
        .map(|name| name.to_string_lossy().replace('\\', "/"))
}

fn native_relative(root: &Path, selected: &Path) -> Option<String> {
    let path = selected.strip_prefix(root).ok()?;
    if path.as_os_str().is_empty() {
        return None;
    }
    Some(path.to_string_lossy().replace('\\', "/"))
}

fn strip_windows_prefix(selected: &str, root: &str) -> Option<String> {
    let root = root.trim_end_matches('/');
    if !windows_absolute_text(selected) || !windows_absolute_text(root) {
        return None;
    }
    let rest = selected.strip_prefix(root)?.strip_prefix('/')?;
    (!rest.is_empty()).then(|| rest.to_string())
}

fn slash_form(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn windows_absolute_text(text: &str) -> bool {
    let bytes = text.as_bytes();
    (bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/'))
        || text.starts_with(r"\\")
        || text.starts_with("//")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_makes_selected_files_relative_and_hides_external_roots() {
        let mut context = RequestContext {
            selected_files: vec![
                r"D:\Code\Peek\src-tauri\src\main.rs".to_string(),
                r"C:\Temp\notes.txt".to_string(),
            ],
            ..RequestContext::default()
        };

        context.set_workspace("Peek".to_string(), Path::new(r"D:\Code\Peek"));

        assert_eq!(
            context.selected_files,
            vec!["src-tauri/src/main.rs", "notes.txt"]
        );
        assert_eq!(context.workspace.as_ref().unwrap().name, "Peek");
    }
}

/// 统一 AI 请求 — Provider 各自内部转换，不暴露 DeepSeekRequest 等。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub request_id: String,
    pub session_id: String,
    pub messages: Vec<ChatMessage>,
    pub context: RequestContext,
    pub provider: Option<String>,
    pub stream: bool,
    #[serde(default = "empty_tools", skip_serializing_if = "tools_is_empty")]
    pub tools: std::sync::Arc<[serde_json::Value]>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

fn empty_tools() -> std::sync::Arc<[serde_json::Value]> {
    std::sync::Arc::from([])
}

fn tools_is_empty(tools: &std::sync::Arc<[serde_json::Value]>) -> bool {
    tools.is_empty()
}
