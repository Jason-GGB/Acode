use serde_json::json;
use tauri::AppHandle;
use tauri::Manager;

use crate::app_state::AppState;

use crate::core::remote::upload::{MAX_CHUNK_BYTES, MAX_UPLOAD_BYTES};

pub(super) fn workspace_snapshot_payload(
    app: &AppHandle,
    session_id: Option<&str>,
) -> serde_json::Value {
    let SessionFileRoot::Workspace(workspace) = session_file_root(app, session_id, None) else {
        return json!({
            "workspaceId": null,
            "name": null,
            "rootPath": null,
            "sessionId": session_id,
            "runState": "idle",
            "changedFiles": []
        });
    };
    json!({
        "workspaceId": workspace.id,
        "name": workspace.name,
        "rootPath": workspace.root.to_string_lossy(),
        "sessionId": session_id,
        "runState": "idle",
        "changedFiles": []
    })
}

enum SessionFileRoot {
    Workspace(crate::core::workspace::Workspace),
    Inbox(std::path::PathBuf),
    NoRoot,
}

fn session_file_root(
    app: &AppHandle,
    session_id: Option<&str>,
    workspace_id: Option<&str>,
) -> SessionFileRoot {
    use crate::core::remote::companion_workspace::{
        project_companion_workspace, CompanionWorkspace,
    };

    let Some(state) = app.try_state::<AppState>() else {
        return SessionFileRoot::NoRoot;
    };
    let session_id = session_id.map(str::trim).filter(|id| !id.is_empty());
    let workspaces = state.core.workspaces().list();
    let bound =
        session_id.and_then(|id| state.core.chat().conversation().workspace_for_session(id));
    match project_companion_workspace(workspace_id, bound.as_deref(), &workspaces, session_id) {
        CompanionWorkspace::Workspace(workspace) => SessionFileRoot::Workspace(workspace.clone()),
        CompanionWorkspace::Inbox { session_id } => {
            match crate::core::remote::upload::inbox_dir(app, &session_id) {
                Some(root) => SessionFileRoot::Inbox(root),
                None => SessionFileRoot::NoRoot,
            }
        }
        CompanionWorkspace::NoRoot => SessionFileRoot::NoRoot,
    }
}

fn guess_mime(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("txt") | Some("log") | Some("gitignore") => "text/plain",
        Some("md") | Some("markdown") => "text/markdown",
        Some("html") | Some("htm") => "text/html",
        Some("css") => "text/css",
        Some("csv") => "text/csv",
        Some("json") => "application/json",
        Some("xml") => "application/xml",
        Some("yaml") | Some("yml") => "application/yaml",
        Some("toml") => "application/toml",
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("gz") | Some("tgz") => "application/gzip",
        Some("7z") => "application/x-7z-compressed",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("doc") => "application/msword",
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some("xls") => "application/vnd.ms-excel",
        Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        Some("ppt") => "application/vnd.ms-powerpoint",
        Some("pptx") => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        _ => "application/octet-stream",
    }
}

/// Resolve `rel_path` inside the workspace root, rejecting escapes
/// (absolute paths, `..`, symlinks pointing outside the root).
fn resolve_in_workspace(
    root: &std::path::Path,
    rel_path: &str,
) -> Result<std::path::PathBuf, String> {
    let rel = rel_path.trim().trim_start_matches(['/', '\\']);
    if rel.is_empty() {
        return Err("empty path".into());
    }
    let mut candidate = root.to_path_buf();
    for part in rel.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err("path escapes workspace root".into());
        }
        candidate.push(part);
    }
    let canonical_root =
        std::fs::canonicalize(root).map_err(|e| format!("workspace root unavailable: {e}"))?;
    let canonical =
        std::fs::canonicalize(&candidate).map_err(|_| format!("file not found: {rel}"))?;
    if !canonical.starts_with(&canonical_root) {
        return Err("path escapes workspace root".into());
    }
    Ok(canonical)
}

/// Directories outside any workspace whose files the phone may still pull: generated
/// images live in app-data, quick-ask turns (no workspace) write into the `peek-public`
/// scratch root, and phone uploads land in the companion inbox.
fn shared_asset_roots(app: &AppHandle) -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    if let Ok(data) = app.path().app_data_dir() {
        roots.push(data.join("generated"));
    }
    if let Ok(config) = app.path().app_config_dir() {
        roots.push(config.join("companion-inbox"));
    }
    roots.push(std::env::temp_dir().join("peek-public"));
    roots
}

/// Absolute path under one of [`shared_asset_roots`]; `None` when it is not one of ours
/// (the caller then falls back to workspace-relative resolution).
fn resolve_shared_asset(app: &AppHandle, raw_path: &str) -> Option<std::path::PathBuf> {
    let trimmed = raw_path.trim().trim_start_matches("path:");
    let candidate = std::path::Path::new(trimmed);
    if !candidate.is_absolute() {
        return None;
    }
    let canonical = std::fs::canonicalize(candidate).ok()?;
    for root in shared_asset_roots(app) {
        let Ok(root) = std::fs::canonicalize(&root) else {
            continue;
        };
        if canonical.starts_with(&root) {
            return Some(canonical);
        }
    }
    None
}

/// Shared-asset path first, else inside the session / requested workspace.
fn resolve_download_path(
    app: &AppHandle,
    session_id: Option<&str>,
    workspace_id: Option<&str>,
    rel_path: &str,
) -> Result<std::path::PathBuf, String> {
    if let Some(file) = resolve_shared_asset(app, rel_path) {
        return Ok(file);
    }
    let root = match session_file_root(app, session_id, workspace_id) {
        SessionFileRoot::Workspace(workspace) => workspace.root,
        SessionFileRoot::Inbox(root) => root,
        SessionFileRoot::NoRoot => return Err("No workspace selected".into()),
    };
    resolve_in_workspace(&root, rel_path)
}

pub(super) fn begin_file_download(
    app: &AppHandle,
    session_id: Option<&str>,
    workspace_id: Option<&str>,
    rel_path: &str,
) -> Result<serde_json::Value, String> {
    let file = resolve_download_path(app, session_id, workspace_id, rel_path)?;
    let meta = std::fs::metadata(&file).map_err(|e| format!("stat failed: {e}"))?;
    if !meta.is_file() {
        return Err("not a file".into());
    }
    let size = meta.len();
    if size > MAX_UPLOAD_BYTES {
        return Err(format!(
            "file too large for download: {size} bytes (max {MAX_UPLOAD_BYTES})"
        ));
    }
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| rel_path.trim_start_matches(['/', '\\']).to_string());
    let mime = guess_mime(&file).to_string();
    let id = crate::core::remote::download::mint(file, name.clone(), mime.clone(), size)?;
    let url = crate::core::remote::download::public_download_url(app, &id);
    Ok(json!({
        "downloadId": id,
        "url": url,
        "size": size,
        "name": name,
        "mime": mime,
    }))
}

pub(super) async fn read_workspace_file_payload(
    app: &AppHandle,
    session_id: Option<&str>,
    workspace_id: Option<&str>,
    rel_path: &str,
    max_bytes: i32,
    mode: &str,
    offset: Option<u64>,
    length: Option<u64>,
) -> Result<serde_json::Value, String> {
    let file = resolve_download_path(app, session_id, workspace_id, rel_path)?;
    let rel = rel_path.trim().trim_start_matches(['/', '\\']).to_string();
    let download = mode.eq_ignore_ascii_case("download");
    let max_text_bytes = max_bytes.clamp(1, 2_000_000) as u64;
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::{Read, Seek, SeekFrom};

        let meta = std::fs::metadata(&file).map_err(|e| format!("stat failed: {e}"))?;
        if !meta.is_file() {
            return Err("not a file".into());
        }
        let size = meta.len();
        if download {
            if size > MAX_UPLOAD_BYTES {
                return Err(format!(
                    "file too large for download: {size} bytes (max {MAX_UPLOAD_BYTES})"
                ));
            }
            use base64::engine::general_purpose::STANDARD as B64;
            use base64::Engine as _;
            let name = file
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| rel.clone());
            let mime = guess_mime(&file);
            let start = offset.unwrap_or(0);
            if start > size {
                return Err(format!("offset {start} past end of file ({size})"));
            }
            let want = length
                .unwrap_or(MAX_CHUNK_BYTES as u64)
                .min(MAX_CHUNK_BYTES as u64)
                .min(size.saturating_sub(start));
            let mut buf = vec![0u8; want as usize];
            let n = if want == 0 {
                0
            } else {
                let mut f = std::fs::File::open(&file).map_err(|e| format!("read failed: {e}"))?;
                f.seek(SeekFrom::Start(start))
                    .map_err(|e| format!("seek failed: {e}"))?;
                f.read(&mut buf).map_err(|e| format!("read failed: {e}"))?
            };
            buf.truncate(n);
            let next = start + n as u64;
            Ok(json!({
                "path": rel,
                "name": name,
                "size": size,
                "mime": mime,
                "offset": start,
                "length": n,
                "eof": next >= size,
                "contentBase64": B64.encode(&buf),
            }))
        } else {
            let bytes = std::fs::read(&file).map_err(|e| format!("read failed: {e}"))?;
            let truncated = bytes.len() as u64 > max_text_bytes;
            let slice = if truncated {
                &bytes[..max_text_bytes as usize]
            } else {
                &bytes[..]
            };
            Ok(json!({
                "path": rel,
                "content": String::from_utf8_lossy(slice),
                "truncated": truncated,
                "size": size,
            }))
        }
    })
    .await
    .map_err(|e| format!("task failed: {e}"))?
}

fn should_descend_workspace_entry(entry: &walkdir::DirEntry) -> bool {
    if entry.depth() == 0 || !entry.file_type().is_dir() {
        return true;
    }
    !matches!(
        entry.file_name().to_string_lossy().as_ref(),
        ".git" | ".svn" | ".hg" | "node_modules" | ".next" | ".nuxt" | "target" | "dist" | "build"
    )
}

fn list_files_under(root: std::path::PathBuf) -> Vec<String> {
    let mut files = walkdir::WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(should_descend_workspace_entry)
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            entry
                .path()
                .strip_prefix(&root)
                .ok()
                .map(|path| path.to_string_lossy().replace('\\', "/"))
        })
        .take(5_000)
        .collect::<Vec<_>>();
    files.sort_unstable_by_key(|path| path.to_lowercase());
    files
}

pub(super) async fn list_workspace_files_payload(
    app: &AppHandle,
    session_id: Option<&str>,
    workspace_id: Option<&str>,
) -> serde_json::Value {
    let (id, name, root_path, root) = match session_file_root(app, session_id, workspace_id) {
        SessionFileRoot::NoRoot => {
            return json!({
                "workspaceId": null,
                "name": null,
                "rootPath": null,
                "files": [],
                "error": "No workspace selected"
            });
        }
        SessionFileRoot::Inbox(root) => (None, None, None, root),
        SessionFileRoot::Workspace(workspace) => {
            let root = workspace.root.clone();
            let root_path = workspace.root.to_string_lossy().into_owned();
            (
                Some(workspace.id),
                Some(workspace.name),
                Some(root_path),
                root,
            )
        }
    };
    let files = tauri::async_runtime::spawn_blocking(move || list_files_under(root))
        .await
        .unwrap_or_default();
    json!({
        "workspaceId": id,
        "name": name,
        "rootPath": root_path,
        "files": files,
    })
}
