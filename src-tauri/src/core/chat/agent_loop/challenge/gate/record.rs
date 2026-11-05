use super::super::paths::{
    count_open_tasks_from_args, extract_goal_paths, extract_paths_from_args, extract_shell_command,
    is_plan_approve_boilerplate, is_share_tool, is_write_tool, looks_like_local_dev_server,
    paths_match, provides_completion_evidence, provides_verification_evidence,
    push_challenge_message,
};
use super::super::text::{
    challenges_enabled, MAX_COMPLETION_RETRIES, MAX_NO_PROGRESS_REPEATS, STALL_CHALLENGE,
};
use super::{pending_shell_job, CompletionGate};
use crate::core::chat::agent_loop::post_edit_verify::is_mutation_tool;
use crate::core::chat::agent_loop::types::ToolOutcome;
use crate::core::runtime::{ChatRequest, Role};
use crate::runtime::ToolManager;
use std::collections::HashSet;

impl CompletionGate {
    pub fn new() -> Self {
        Self {
            disabled: !challenges_enabled(),
            ..Self::default()
        }
    }

    pub fn set_workspace_root(&mut self, path: std::path::PathBuf) {
        self.workspace_root = Some(path);
    }

    pub fn require_image(&mut self) {
        self.require_image = true;
    }

    pub fn require_share_deliverable(&mut self) {
        self.require_share_deliverable = true;
    }

    /// Capture path-like targets from the latest real user goal (skip plan-approve boilerplate).
    pub fn capture_goal_from_request(&mut self, request: &ChatRequest) {
        let Some(user) = request.messages.iter().rev().find(|message| {
            message.role == Role::User
                && !crate::runtime::tool::is_internal_feedback(&message.id)
                && !is_plan_approve_boilerplate(&message.content)
        }) else {
            return;
        };
        if user.id.starts_with("agent-followup-") {
            self.goal_paths.extend(extract_goal_paths(&user.content));
        } else {
            self.goal_paths = extract_goal_paths(&user.content);
        }
    }

    /// Record a batch with mutation pass first, then verification — fixes parallel
    /// waves where a read arrives before a write in the outcome list.
    pub fn record_tool_outcomes(&mut self, tools: &ToolManager, outcomes: &[ToolOutcome]) {
        let mut finished_jobs = Vec::new();
        for outcome in outcomes {
            let args: serde_json::Value =
                serde_json::from_str(&outcome.arguments).unwrap_or_default();
            if outcome.success {
                if let Some((id, job)) = pending_shell_job(outcome, &args) {
                    self.pending_shell_jobs.insert(id, job);
                }
            }
            let id = args["job_id"].as_str().unwrap_or("");
            match crate::core::chat::agent_loop::turn_end::job_watch(&outcome.tool_name) {
                Some(crate::core::chat::agent_loop::turn_end::JobWatch::HostPoll) => {
                    if outcome.result.starts_with("status: done") {
                        self.take_finished_shell(id, outcome, &mut finished_jobs);
                    } else if outcome.success && self.pending_shell_jobs.contains_key(id) {
                        self.background_retries = 0;
                    }
                }
                Some(crate::core::chat::agent_loop::turn_end::JobWatch::HostStop)
                    if outcome.success =>
                {
                    self.take_finished_shell(id, outcome, &mut finished_jobs);
                }
                Some(crate::core::chat::agent_loop::turn_end::JobWatch::DshPoll) => {
                    if crate::core::chat::agent_loop::turn_end::dsh_job_finished(&outcome.result) {
                        self.take_finished_shell(id, outcome, &mut finished_jobs);
                    } else if outcome.success && self.pending_shell_jobs.contains_key(id) {
                        self.background_retries = 0;
                    }
                }
                Some(crate::core::chat::agent_loop::turn_end::JobWatch::DshStop)
                    if outcome.success =>
                {
                    self.take_finished_shell(id, outcome, &mut finished_jobs);
                }
                _ => {}
            }
        }
        for outcome in outcomes
            .iter()
            .filter(|o| o.success && provides_completion_evidence(o))
        {
            for path in extract_paths_from_args(&outcome.arguments) {
                self.verified_paths
                    .retain(|verified| !paths_match(verified, &path));
            }
        }
        for outcome in outcomes {
            self.record_side_effects(tools, outcome);
        }
        for outcome in outcomes {
            if !outcome.success {
                continue;
            }
            if outcome.tool_name == "generate_image" {
                self.image_succeeded = true;
                self.mutation_succeeded = true;
                self.verification_succeeded = true;
            } else if provides_completion_evidence(outcome) {
                self.mutation_succeeded = true;
                self.verification_succeeded = false;
            }
        }
        for outcome in outcomes {
            if !outcome.success || !self.mutation_succeeded {
                continue;
            }
            for path in &self.mutated_paths {
                let scope_matches =
                    !crate::core::chat::agent_loop::turn_end::counts_as_shell(&outcome.tool_name)
                        || self.workspace_root.as_ref().is_none_or(|root| {
                            extract_shell_command(&outcome.arguments).is_some_and(|command| {
                                crate::core::chat::agent_loop::post_edit_verify::check_targets_path(
                                    root, &command, path,
                                )
                            })
                        });
                if scope_matches
                    && provides_verification_evidence(outcome, &HashSet::from([path.clone()]))
                {
                    self.verified_paths.insert(path.clone());
                }
            }
        }
        self.refresh_verification();
        if !finished_jobs.is_empty() {
            self.record_tool_outcomes(tools, &finished_jobs);
        }
    }

    fn take_finished_shell(
        &mut self,
        id: &str,
        outcome: &ToolOutcome,
        finished: &mut Vec<ToolOutcome>,
    ) {
        let Some(job) = self.pending_shell_jobs.remove(id) else {
            return;
        };
        let mut args: serde_json::Value = serde_json::from_str(&job.arguments).unwrap_or_default();
        args["run_in_background"] = false.into();
        finished.push(ToolOutcome {
            call_id: outcome.call_id.clone(),
            tool_name: if job.dsh {
                "bash".into()
            } else {
                "run_shell".into()
            },
            arguments: args.to_string(),
            result: outcome.result.clone(),
            success: outcome.success,
            user_denied: false,
            plan_gate: false,
        });
    }

    pub fn note_verified_paths(&mut self, paths: impl IntoIterator<Item = String>) {
        self.verified_paths.extend(paths);
        self.refresh_verification();
    }

    fn refresh_verification(&mut self) {
        self.verification_succeeded = self.image_succeeded
            || (!self.mutated_paths.is_empty()
                && self.mutated_paths.iter().all(|path| {
                    self.verified_paths
                        .iter()
                        .any(|verified| paths_match(path, verified))
                }));
    }

    /// Fold one tool outcome (single-item wrapper around [`Self::record_tool_outcomes`]).
    #[cfg(test)]
    pub fn record_tool_outcome(&mut self, tools: &ToolManager, outcome: &ToolOutcome) {
        self.record_tool_outcomes(tools, std::slice::from_ref(outcome));
    }

    /// Merge paths a sub-agent mutated into this parent gate.
    pub fn note_subagent_mutations(&mut self, paths: impl IntoIterator<Item = String>) {
        let mut any = false;
        for path in paths {
            if path.is_empty() {
                continue;
            }
            self.mutated_paths.insert(path);
            any = true;
        }
        if any {
            self.verified_paths.clear();
            self.mutation_succeeded = true;
            self.verification_succeeded = false;
            self.wrote_files = true;
        }
    }

    /// Track open plan/todo items from `update_tasks` payloads.
    #[allow(dead_code)]
    pub fn note_open_task_count(&mut self, count: u32) {
        self.open_task_count = count;
    }

    fn record_side_effects(&mut self, _tools: &ToolManager, outcome: &ToolOutcome) {
        if outcome.success
            && crate::core::chat::agent_loop::turn_end::task_list_kind(&outcome.tool_name)
                == Some(crate::core::chat::agent_loop::turn_end::TaskListKind::Plan)
        {
            // Explicit acceptance criteria supersede path-name guesses (a path
            // mentioned by the user may be a reference or a file to preserve).
            if serde_json::from_str::<serde_json::Value>(&outcome.arguments)
                .ok()
                .and_then(|value| value["tasks"].as_array().cloned())
                .is_some_and(|tasks| {
                    tasks.iter().any(|task| {
                        task["acceptance_criteria"]
                            .as_array()
                            .is_some_and(|criteria| !criteria.is_empty())
                    })
                })
            {
                self.goal_paths.clear();
            }
            if let Some(count) = count_open_tasks_from_args(&outcome.arguments) {
                self.open_task_count = count;
            }
        }
        if outcome.success
            && crate::core::chat::agent_loop::turn_end::task_list_kind(&outcome.tool_name)
                == Some(crate::core::chat::agent_loop::turn_end::TaskListKind::Todo)
        {
            if let Some(count) =
                crate::core::chat::agent_loop::turn_end::open_todo_count(&outcome.arguments)
            {
                self.open_todo_count = count;
            }
        }
        if outcome.tool_name == "complete_plan_step" && outcome.success {
            self.open_task_count = self.open_task_count.saturating_sub(1);
        }
        if outcome.success
            && matches!(
                outcome.tool_name.as_str(),
                "run_subagent" | "run_parallel_subagents" | "run_skill"
            )
        {
            let paths = crate::core::tools::agent::extract_touched_paths_from_subagent_result(
                &outcome.result,
            );
            if !paths.is_empty() {
                self.note_subagent_mutations(paths);
            }
        }
        if !outcome.success {
            return;
        }
        if !(matches!(outcome.tool_name.as_str(), "update_tasks" | "save_plan")
            || crate::core::chat::agent_loop::turn_end::is_shell_poll(&outcome.tool_name))
        {
            use std::hash::{Hash, Hasher};
            let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
            (&outcome.tool_name, &outcome.arguments, &outcome.result).hash(&mut fingerprint);
            if self.progress_seen.insert(fingerprint.finish()) {
                self.empty_completion_retries = 0;
                self.verification_retries = 0;
                self.goal_coverage_retries = 0;
                self.open_tasks_retries = 0;
                self.length_retries = 0;
            }
        }
        let key = crate::core::chat::agent_loop::failure::failure_key(outcome);
        let new_evidence = self
            .success_repeats
            .get(&key)
            .is_none_or(|(result, _)| result != &outcome.result);
        if new_evidence {
            for (_, count) in self.success_repeats.values_mut() {
                *count = 0;
            }
            self.stalled = false;
        }
        let repeated = self
            .success_repeats
            .entry(key)
            .or_insert_with(|| (outcome.result.clone(), 0));
        if repeated.0 != outcome.result {
            *repeated = (outcome.result.clone(), 0);
        }
        repeated.1 += 1;
        if repeated.1 >= MAX_NO_PROGRESS_REPEATS
            && !crate::core::chat::agent_loop::turn_end::is_shell_poll(&outcome.tool_name)
        {
            self.stalled = true;
        }
        if provides_completion_evidence(outcome) {
            for path in extract_paths_from_args(&outcome.arguments) {
                self.mutated_paths.insert(path);
            }
        }
        if is_share_tool(&outcome.tool_name) {
            self.shared_deliverable = true;
        }
        if is_write_tool(&outcome.tool_name) || is_mutation_tool(&outcome.tool_name) {
            self.wrote_files = true;
        }
        if looks_like_local_dev_server(&outcome.tool_name, &outcome.arguments, &outcome.result) {
            self.started_local_server = true;
        }
    }

    /// After a tool batch: if the same successful call is looping, inject a
    /// stall challenge before the next model turn.
    pub fn maybe_challenge_stall(
        &mut self,
        request: &mut ChatRequest,
        user_msg_index: &mut Option<usize>,
    ) -> Option<String> {
        if self.disabled || !self.stalled || self.stall_retries >= MAX_COMPLETION_RETRIES {
            return None;
        }
        self.stall_retries += 1;
        self.stalled = false;
        push_challenge_message(request, user_msg_index, STALL_CHALLENGE);
        Some("stall_loop".to_string())
    }

    pub fn repeated_without_progress(&self) -> bool {
        !self.disabled
            && self.success_repeats.iter().any(|(key, (_, count))| {
                *count >= 8
                    && !crate::core::chat::agent_loop::turn_end::is_shell_poll(
                        key.split_once('|').map(|(name, _)| name).unwrap_or(key),
                    )
            })
    }
}
