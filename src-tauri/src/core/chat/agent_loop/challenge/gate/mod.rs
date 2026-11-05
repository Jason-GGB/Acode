mod evaluate;
mod record;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use crate::core::chat::agent_loop::types::ToolOutcome;

#[derive(Default)]
pub struct CompletionGate {
    mutation_succeeded: bool,
    verification_succeeded: bool,
    empty_completion_retries: u32,
    verification_retries: u32,
    goal_coverage_retries: u32,
    stall_retries: u32,
    /// Paths mentioned in the current-turn user goal (heuristic).
    goal_paths: HashSet<String>,
    /// Paths successfully mutated this turn.
    mutated_paths: HashSet<String>,
    /// Evidence applies to the latest edit of each path, never the whole turn.
    verified_paths: HashSet<String>,
    workspace_root: Option<std::path::PathBuf>,
    /// Success fingerprints: tool|args → count (no-progress loop detection).
    success_repeats: HashMap<String, (String, u32)>,
    stalled: bool,
    wrote_files: bool,
    started_local_server: bool,
    shared_deliverable: bool,
    share_nudge_retries: u32,
    /// Image chat mode: a turn cannot finish without a successful generate_image.
    require_image: bool,
    image_succeeded: bool,
    image_retries: u32,
    /// Open plan/todo items still pending this turn (from update_tasks).
    open_task_count: u32,
    open_tasks_retries: u32,
    /// Only nudge Companion share when the session originated from the phone app.
    require_share_deliverable: bool,
    /// Disable all challenges for this gate instance (eval ablation).
    disabled: bool,
    length_retries: u32,
    /// Distinct successful results replenish continuation budgets. Repeating
    /// identical reads does not buy an endless series of completion retries.
    progress_seen: HashSet<u64>,
    pending_shell_jobs: HashMap<String, PendingJob>,
    open_todo_count: u32,
    background_retries: u32,
}

struct PendingJob {
    arguments: String,
    dsh: bool,
}

fn pending_shell_job(
    outcome: &ToolOutcome,
    args: &serde_json::Value,
) -> Option<(String, PendingJob)> {
    let kind = crate::core::chat::agent_loop::turn_end::shell_kind(&outcome.tool_name)?;
    let command = args
        .get("command")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if crate::core::tools::shell_jobs::background_allowed(command) {
        return None;
    }
    let requested = args
        .get("run_in_background")
        .and_then(|value| value.as_bool())
        == Some(true);
    let promoted = kind == crate::core::chat::agent_loop::turn_end::ShellKind::Dsh
        && crate::core::chat::agent_loop::turn_end::dsh_background_result(&outcome.result);
    if !requested && !promoted {
        return None;
    }
    let id = if kind == crate::core::chat::agent_loop::turn_end::ShellKind::Dsh {
        crate::core::chat::agent_loop::turn_end::dsh_job_id(&outcome.result)
            .unwrap_or_else(|| outcome.result.trim().to_string())
    } else {
        outcome.result.trim().to_string()
    };
    if id.is_empty() {
        return None;
    }
    Some((
        id,
        PendingJob {
            arguments: outcome.arguments.clone(),
            dsh: kind == crate::core::chat::agent_loop::turn_end::ShellKind::Dsh,
        },
    ))
}
