//! Whether a model step with no tool calls finishes the turn or continues it.
//!
//! Recorders ask the name table here when they fold a tool result. The loop
//! asks [`TurnEnd::decide`] only after that step produced no tool calls.

use crate::core::chat::agent_loop::challenge::{
    push_challenge_message, ChallengeOutcome, CompletionGate, OPEN_TASKS_CHALLENGE,
};
use crate::core::chat::agent_loop::types::{non_empty, ToolOutcome};
use crate::core::runtime::{ChatRequest, Role};
use crate::runtime::ToolManager;

/// File tools that count as a successful workspace mutation.
pub const MUTATION_TOOL_NAMES: &[&str] = &[
    "apply_patch",
    "write_file",
    "replace_in_file",
    "replace_many_in_file",
    "move_path",
    "delete_text_range",
    "delete_go_symbol",
    "edit_notebook_cell",
    "write",
    "edit",
];

const TODO_OPEN_TASKS: &str = concat!(
    "[System] Completion rejected: todo items are still open. ",
    "Mark each finished item completed with `todo_write`, then answer.",
);

/// A tool name that edits a file.
pub fn counts_as_mutation(name: &str) -> bool {
    MUTATION_TOOL_NAMES.contains(&name)
}

/// A tool name that runs a shell command.
pub(crate) fn counts_as_shell(name: &str) -> bool {
    shell_kind(name).is_some()
}

/// Host shell versus DeepSeek harness shell.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShellKind {
    Host,
    Dsh,
}

pub(crate) fn shell_kind(name: &str) -> Option<ShellKind> {
    match name {
        "run_shell" => Some(ShellKind::Host),
        "bash" | "pwsh" => Some(ShellKind::Dsh),
        _ => None,
    }
}

/// How a later call observes a shell job started earlier.
#[derive(Clone, Copy)]
pub(crate) enum JobWatch {
    HostPoll,
    HostStop,
    DshPoll,
    DshStop,
}

pub(crate) fn job_watch(name: &str) -> Option<JobWatch> {
    match name {
        "wait_for_shell" | "read_shell_output" => Some(JobWatch::HostPoll),
        "stop_shell" => Some(JobWatch::HostStop),
        "job_output" => Some(JobWatch::DshPoll),
        "job_kill" => Some(JobWatch::DshStop),
        _ => None,
    }
}

pub(crate) fn is_shell_poll(name: &str) -> bool {
    matches!(
        job_watch(name),
        Some(JobWatch::HostPoll | JobWatch::DshPoll)
    )
}

/// Checklist tools. Plan tasks carry acceptance criteria; todos do not.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskListKind {
    Plan,
    Todo,
}

pub(crate) fn task_list_kind(name: &str) -> Option<TaskListKind> {
    match name {
        "update_tasks" => Some(TaskListKind::Plan),
        "todo_write" => Some(TaskListKind::Todo),
        _ => None,
    }
}

/// Pending and in-progress todos. `None` when the payload is not a todo list.
pub(crate) fn open_todo_count(arguments: &str) -> Option<u32> {
    let value: serde_json::Value = serde_json::from_str(arguments).ok()?;
    let todos = value.get("todos")?.as_array()?;
    Some(
        todos
            .iter()
            .filter(|todo| {
                matches!(
                    todo.get("status").and_then(|status| status.as_str()),
                    Some("pending" | "in_progress")
                )
            })
            .count() as u32,
    )
}

pub(crate) fn dsh_job_finished(result: &str) -> bool {
    [
        "[status: completed]",
        "[status: failed]",
        "[status: cancelled]",
    ]
    .iter()
    .any(|status| result.contains(status))
}

pub(crate) fn dsh_job_id(result: &str) -> Option<String> {
    let marker = "[job_id:";
    let start = result.find(marker)? + marker.len();
    let rest = result[start..].trim_start();
    let end = rest.find(']')?;
    let id = rest[..end].trim();
    (!id.is_empty()).then(|| id.to_string())
}

pub(crate) fn dsh_background_result(result: &str) -> bool {
    result.contains("moved to the background")
}

/// Challenge text for shell jobs that have not exited.
pub(crate) fn pending_shell_message(ids: &str, host: bool, dsh: bool) -> String {
    let call = match (host, dsh) {
        (true, true) => "wait_for_shell/read_shell_output and job_output",
        (false, true) => "job_output",
        _ => "wait_for_shell/read_shell_output",
    };
    format!(
        "[System] Finite shell jobs are still pending: {ids}. Call {call} until they exit. Do not restart them or claim completion from a job ID."
    )
}

/// Challenge text for whichever task lists are still open.
pub(crate) fn open_work_message(plan_open: bool, todos_open: bool) -> String {
    match (plan_open, todos_open) {
        (true, true) => format!("{OPEN_TASKS_CHALLENGE}\n{TODO_OPEN_TASKS}"),
        (true, false) => OPEN_TASKS_CHALLENGE.to_string(),
        (false, true) => TODO_OPEN_TASKS.to_string(),
        (false, false) => String::new(),
    }
}

/// What the loop should do with a model step that called no tools.
#[derive(Debug)]
pub enum Decision {
    /// Send the turn-complete event with these fields.
    Finish {
        content: String,
        reasoning: Option<String>,
        finish_reason: Option<String>,
        /// Record the assistant reply and drain follow-up inserts before sending.
        drain_followups: bool,
    },
    /// The feedback is already on the request. Run another model step.
    Continue {
        status_kind: String,
        message: String,
    },
    /// Recorded edits have not been through the verification queue yet.
    Verify,
}

/// The model step Turn end is judging.
pub struct Answer {
    pub content: String,
    pub reasoning: String,
    pub finish_reason: Option<String>,
    pub paused: bool,
    pub unresolved_criteria: Vec<String>,
    pub criteria_retries: u32,
}

/// Owns the completion gate and the name table the rest of the turn records through.
pub struct TurnEnd {
    gate: CompletionGate,
    verification_pending: bool,
}

impl TurnEnd {
    pub fn new() -> Self {
        Self {
            gate: CompletionGate::new(),
            verification_pending: false,
        }
    }

    pub(crate) fn gate_mut(&mut self) -> &mut CompletionGate {
        &mut self.gate
    }

    /// Fold tool results into the evidence used by a later [`Self::decide`].
    pub fn record(&mut self, tools: &ToolManager, outcomes: &[ToolOutcome]) {
        if outcomes.iter().any(|outcome| {
            outcome.success
                && (counts_as_mutation(&outcome.tool_name)
                    || matches!(
                        outcome.tool_name.as_str(),
                        "run_subagent" | "run_parallel_subagents" | "run_skill"
                    ))
        }) {
            self.verification_pending = true;
        }
        self.gate.record_tool_outcomes(tools, outcomes);
    }

    /// The verification queue has been drained for the edits recorded so far.
    pub(crate) fn note_verification_drained(&mut self) {
        self.verification_pending = false;
    }

    /// Judge a step that called no tools.
    pub fn decide(
        &mut self,
        request: &mut ChatRequest,
        user_msg_index: &mut Option<usize>,
        answer: Answer,
    ) -> Decision {
        if answer.paused {
            return Decision::Finish {
                content: answer.content,
                reasoning: non_empty(answer.reasoning),
                finish_reason: Some("user_paused".into()),
                drain_followups: false,
            };
        }
        if self.verification_pending {
            return Decision::Verify;
        }
        if !answer.unresolved_criteria.is_empty() {
            if answer.criteria_retries < 3 {
                let message = format!(
                    "[System] Acceptance criteria still lack successful evidence: {}. Complete and verify them, then update_tasks with evidence call IDs, or report the blocker.",
                    answer.unresolved_criteria.join("; ")
                );
                push_challenge_message(request, user_msg_index, &message);
                return Decision::Continue {
                    status_kind: "acceptance_criteria".into(),
                    message,
                };
            }
            return Decision::Finish {
                content: format!(
                    "任务尚未通过验收：{}。已保留任务状态和执行证据。",
                    answer.unresolved_criteria.join("；")
                ),
                reasoning: non_empty(answer.reasoning),
                finish_reason: Some("unverified_completion".into()),
                drain_followups: false,
            };
        }
        match self.gate.evaluate_final_answer(
            request,
            user_msg_index,
            answer.content,
            answer.reasoning,
            answer.finish_reason,
        ) {
            ChallengeOutcome::ContinueWithChallenge { status_kind } => {
                let message = request
                    .messages
                    .iter()
                    .rev()
                    .find(|message| {
                        message.role == Role::User && message.id.starts_with("agent-feedback-")
                    })
                    .map(|message| message.content.clone())
                    .unwrap_or_default();
                Decision::Continue {
                    status_kind,
                    message,
                }
            }
            ChallengeOutcome::Finish {
                content,
                reasoning,
                finish_reason,
            } => Decision::Finish {
                content,
                reasoning,
                finish_reason,
                drain_followups: true,
            },
        }
    }
}

#[cfg(test)]
#[path = "turn_end_tests.rs"]
mod tests;
