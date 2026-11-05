use std::collections::HashSet;

use crate::core::chat::agent_loop::post_edit_verify::is_mutation_tool;
use crate::core::chat::agent_loop::types::{now_millis, ToolOutcome};
use crate::core::runtime::{ChatMessage, ChatRequest, MessageStatus, Role};

pub(crate) fn is_write_tool(name: &str) -> bool {
    matches!(
        name,
        "write_file"
            | "replace_in_file"
            | "replace_many_in_file"
            | "apply_patch"
            | "edit_notebook_cell"
    )
}

pub(crate) fn is_share_tool(name: &str) -> bool {
    matches!(name, "share_to_companion" | "share_preview_url")
}

pub(crate) fn looks_like_local_dev_server(tool_name: &str, arguments: &str, result: &str) -> bool {
    if crate::core::chat::agent_loop::turn_end::shell_kind(tool_name).is_none() {
        return false;
    }
    let value: serde_json::Value =
        serde_json::from_str(arguments).unwrap_or(serde_json::Value::Null);
    let requested = value
        .get("run_in_background")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let promoted = crate::core::chat::agent_loop::turn_end::shell_kind(tool_name)
        == Some(crate::core::chat::agent_loop::turn_end::ShellKind::Dsh)
        && crate::core::chat::agent_loop::turn_end::dsh_background_result(result);
    if !requested && !promoted {
        return false;
    }
    let command = value
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or(arguments)
        .to_ascii_lowercase();
    crate::core::tools::shell_jobs::background_allowed(&command)
        && ["dev", "vite", "watch", "serve"]
            .iter()
            .any(|marker| command.contains(marker))
}

pub(crate) fn paths_match(goal: &str, mutated: &str) -> bool {
    let g = normalize_path_key(goal);
    let m = normalize_path_key(mutated);
    m == g || m.ends_with(&format!("/{g}")) || g.ends_with(&format!("/{m}"))
}

fn normalize_path_key(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .to_ascii_lowercase()
}

pub(crate) fn extract_goal_paths(content: &str) -> HashSet<String> {
    let mut paths = HashSet::new();
    for token in content.split_whitespace() {
        let cleaned = trim_path_token(token);
        if looks_like_path(cleaned) {
            paths.insert(cleaned.to_string());
        }
    }
    // Only tokens inside backticks — `split` on a string with no backticks
    // yields the whole sentence, which must not be treated as a path.
    if content.contains('`') {
        for (index, part) in content.split('`').enumerate() {
            if index % 2 == 0 {
                continue;
            }
            let trimmed = trim_path_token(part.trim());
            if looks_like_path(trimmed) {
                paths.insert(trimmed.to_string());
            }
        }
    }
    paths
}

fn trim_path_token(token: &str) -> &str {
    token.trim_matches(|c: char| {
        matches!(
            c,
            ',' | ';'
                | ':'
                | '"'
                | '\''
                | '`'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '.'
                | '!'
                | '?'
        )
    })
}

fn looks_like_path(value: &str) -> bool {
    if value.len() < 3 || value.len() > 240 || value.contains(' ') {
        return false;
    }
    let has_sep = value.contains('/') || value.contains('\\');
    if has_sep {
        return true;
    }
    // Extension-only paths must look like `name.ext` with a short alphanumeric ext.
    // Reject version-like tokens (`1.0.0`) that have multiple dots.
    let Some((stem, ext)) = value.rsplit_once('.') else {
        return false;
    };
    if stem.is_empty() || stem.starts_with('.') || stem.contains('.') {
        return false;
    }
    let ext_ok = (1..=5).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_alphanumeric());
    ext_ok && stem.chars().any(|c| c.is_ascii_alphanumeric())
}

fn extract_path_from_args(arguments: &str) -> Option<String> {
    extract_paths_from_args(arguments).into_iter().next()
}

pub(crate) fn extract_paths_from_args(arguments: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(arguments) else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for key in ["path", "file_path", "from", "to"] {
        if let Some(path) = value
            .get(key)
            .and_then(|v| v.as_str())
            .filter(|p| !p.is_empty())
        {
            paths.push(path.to_string());
        }
    }
    if let Some(input) = value.get("input").and_then(|v| v.as_str()) {
        for line in input.lines() {
            for marker in [
                "*** Update File: ",
                "*** Add File: ",
                "*** Delete File: ",
                "*** Move to: ",
            ] {
                if let Some(path) = line.strip_prefix(marker) {
                    let path = path.trim();
                    if !path.is_empty() {
                        paths.push(path.to_string());
                    }
                }
            }
        }
    }
    paths
}

pub(crate) fn push_challenge_message(
    request: &mut ChatRequest,
    user_msg_index: &mut Option<usize>,
    feedback: &str,
) {
    if user_msg_index.is_none() {
        *user_msg_index = Some(request.messages.len());
    }
    request.messages.push(ChatMessage {
        id: format!("agent-feedback-{}", uuid::Uuid::new_v4()),
        session_id: request.session_id.clone(),
        role: Role::User,
        content: feedback.to_string(),
        reasoning: None,
        work_timeline: None,
        tool_activities: None,
        tool_calls: None,
        tool_call_id: None,
        name: None,
        status: MessageStatus::Done,
        timestamp: now_millis(),
        estimated_tokens: None,
    });
}

pub(crate) fn provides_completion_evidence(outcome: &ToolOutcome) -> bool {
    is_mutation_tool(&outcome.tool_name)
}

pub(crate) fn provides_verification_evidence(
    outcome: &ToolOutcome,
    mutated_paths: &HashSet<String>,
) -> bool {
    if outcome.tool_name == "read_file" {
        let Some(path) = extract_path_from_args(&outcome.arguments) else {
            return false;
        };
        return mutated_paths.iter().any(|mutated| {
            paths_match(mutated, &path)
                && !crate::core::chat::agent_loop::post_edit_verify::needs_executable_check(mutated)
        });
    }
    if !crate::core::chat::agent_loop::turn_end::counts_as_shell(&outcome.tool_name)
        || !outcome.success
    {
        return false;
    }
    let Some(command) = extract_shell_command(&outcome.arguments) else {
        return false;
    };
    let command = command.to_ascii_lowercase();
    if !crate::core::chat::agent_loop::post_edit_verify::shell_exit_code_ok(&outcome.result) {
        return false;
    }
    // A quoted check name in echo/printf is not a check. Complex shell scripts
    // require explicit verification instead of guessing at their semantics.
    if command.contains([';', '|', '\n', '`', '$']) {
        return false;
    }
    const CHECK_MARKERS: &[&str] = &[
        "cargo test",
        "cargo check",
        "pytest",
        "python -m pytest",
        "python -m unittest",
        "pnpm check",
        "pnpm run check",
        "pnpm run test",
        "pnpm run typecheck",
        "pnpm test",
        "pnpm build",
        "npm run check",
        "npm run -s check",
        "npm run typecheck",
        "npm run -s typecheck",
        "npm run lint",
        "npm run -s lint",
        "npm test",
        "npm run build",
        "vue-tsc",
        "tsc --noemit",
        "tsc -p",
        "node --test",
    ];
    CHECK_MARKERS
        .iter()
        .any(|marker| command.starts_with(marker))
        && mutated_paths.iter().all(|path| {
            let path = path.replace('\\', "/").to_ascii_lowercase();
            if path.ends_with(".rs") || path.ends_with("cargo.toml") || path.ends_with("cargo.lock")
            {
                command.starts_with("cargo ")
            } else if path.ends_with(".py") || path.ends_with("pyproject.toml") {
                command.starts_with("pytest") || command.starts_with("python ")
            } else {
                !command.starts_with("cargo ")
                    && !command.starts_with("pytest")
                    && !command.starts_with("python ")
            }
        })
}

pub(crate) fn extract_shell_command(arguments: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(arguments).ok()?;
    value
        .get("command")
        .and_then(|v| v.as_str())
        .filter(|c| !c.is_empty())
        .map(str::to_string)
}

pub(crate) fn is_plan_approve_boilerplate(content: &str) -> bool {
    let trimmed = content.trim();
    trimmed.starts_with("计划已批准")
        || trimmed.to_ascii_lowercase().starts_with("plan approved")
        || trimmed.contains("本回合写操作已解除限制")
        || trimmed.contains("write tools are unlocked for this turn")
}

pub(crate) fn count_open_tasks_from_args(arguments: &str) -> Option<u32> {
    let value: serde_json::Value = serde_json::from_str(arguments).ok()?;
    let tasks = value.get("tasks")?.as_array()?;
    let open = tasks
        .iter()
        .filter(|task| {
            let status = task
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("pending");
            !matches!(status, "completed" | "cancelled" | "done")
        })
        .count();
    Some(open as u32)
}
