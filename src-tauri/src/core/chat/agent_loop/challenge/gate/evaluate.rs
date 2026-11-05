use super::super::paths::{paths_match, push_challenge_message};
use super::super::text::{
    ChallengeOutcome, COMPLETION_CHALLENGE, GOAL_COVERAGE_CHALLENGE, IMAGE_REQUIRED_CHALLENGE,
    LENGTH_EXCEEDED_CHALLENGE, MAX_COMPLETION_RETRIES, SHARE_DELIVERABLE_CHALLENGE,
    VERIFICATION_CHALLENGE,
};
use super::CompletionGate;
use crate::core::chat::agent_loop::types::{non_empty, now_millis};
use crate::core::runtime::{ChatMessage, ChatRequest, MessageStatus, Role};

fn push_completion_feedback(
    request: &mut ChatRequest,
    user_msg_index: &mut Option<usize>,
    content: String,
    reasoning: String,
    feedback: &str,
) {
    request.messages.push(ChatMessage {
        id: format!("msg-{}", now_millis()),
        session_id: request.session_id.clone(),
        role: Role::Assistant,
        content,
        reasoning: non_empty(reasoning),
        work_timeline: None,
        tool_activities: None,
        tool_calls: None,
        tool_call_id: None,
        name: None,
        status: MessageStatus::Done,
        timestamp: now_millis(),
        estimated_tokens: None,
    });
    push_challenge_message(request, user_msg_index, feedback);
}

/// Successful file mutations prove that work happened. Orchestration-only tools
fn reject_unverified_completion(
    content: &mut String,
    request: &ChatRequest,
    mutation_succeeded: bool,
    verification_succeeded: bool,
    has_open_tasks: bool,
) -> bool {
    if mutation_succeeded && verification_succeeded && !has_open_tasks {
        return false;
    }
    if crate::runtime::tool::is_question_only_request(request) {
        return false;
    }
    // After retries are exhausted: rewrite when there was mutation without
    // verification, open tasks remain, or a completion claim without mutation.
    let should_rewrite = (mutation_succeeded && !verification_succeeded)
        || has_open_tasks
        || (!mutation_succeeded && has_completion_claim(content));
    if !should_rewrite {
        return false;
    }
    *content = if content
        .chars()
        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
    {
        if has_open_tasks {
            "未完成：仍有未勾选的计划步骤。请为已完成的步骤提供证据并标记完成，或明确取消超出范围的步骤。".to_string()
        } else if mutation_succeeded {
            "未验证完成：虽然执行了修改，但没有成功检查修改后的结果，因此不能确认任务真的完成。请运行读取检查、测试或构建验证。".to_string()
        } else {
            "未完成：本轮没有任何修改类工具成功执行，因此无法确认发生了实际改动。请重新执行所需操作，或明确说明当前阻塞项。".to_string()
        }
    } else if has_open_tasks {
        "Not completed: plan/todo items are still open. Complete them with evidence or cancel out-of-scope steps before claiming done.".to_string()
    } else if mutation_succeeded {
        "Completion not verified: a modification ran, but its result was not successfully checked. Run a read-back, test, build, or equivalent verification before claiming completion.".to_string()
    } else {
        "Not completed: no modifying tool succeeded in this turn, so no actual change can be verified. Run the required operation or state the current blocker explicitly.".to_string()
    };
    true
}

fn image_missing_message(content: &str) -> String {
    if content
        .chars()
        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
    {
        "未能生成图片：这一轮没有成功调用生图工具，所以没有新图。请再发送一次，或检查设置 → 生图。"
            .to_string()
    } else {
        "No image was generated: generate_image did not succeed this turn, so there is no new picture. Send again, or check Settings → Image."
            .to_string()
    }
}

fn has_completion_claim(content: &str) -> bool {
    // Keep this strict: weak phrases like "搞定/修改了/done" used to false-trigger
    // an extra model round (and another full think cycle).
    const CLAIMS: &[&str] = &[
        "已完成",
        "全部完成",
        "完成修改",
        "修改完成",
        "修复完成",
        "更新完成",
        "创建完成",
        "写入完成",
        "任务完成",
        "全部搞定",
        "大功告成",
        "successfully completed",
        "task completed",
        "all done",
        "all set",
        "has been fixed",
        "have fixed",
        "has been completed",
        "have completed",
        "implementation is complete",
        "changes are complete",
    ];
    let lower = content.to_ascii_lowercase();
    CLAIMS
        .iter()
        .any(|claim| lower.contains(&claim.to_ascii_lowercase()))
}

impl CompletionGate {
    /// Evaluate a tool-call-free model answer against the honest-completion
    /// policy, deciding whether to challenge it or accept it as final.
    pub fn evaluate_final_answer(
        &mut self,
        request: &mut ChatRequest,
        user_msg_index: &mut Option<usize>,
        content: String,
        reasoning: String,
        finish_reason: Option<String>,
    ) -> ChallengeOutcome {
        if self.disabled {
            return ChallengeOutcome::Finish {
                content,
                reasoning: non_empty(reasoning),
                finish_reason,
            };
        }

        if !self.pending_shell_jobs.is_empty() {
            if self.background_retries < 3 {
                self.background_retries += 1;
                let jobs = self
                    .pending_shell_jobs
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ");
                let host = self.pending_shell_jobs.values().any(|job| !job.dsh);
                let dsh = self.pending_shell_jobs.values().any(|job| job.dsh);
                push_completion_feedback(
                    request,
                    user_msg_index,
                    content,
                    reasoning,
                    &crate::core::chat::agent_loop::turn_end::pending_shell_message(
                        &jobs, host, dsh,
                    ),
                );
                return ChallengeOutcome::ContinueWithChallenge {
                    status_kind: "wait_background_jobs".into(),
                };
            }
            return ChallengeOutcome::Finish {
                content: "任务尚未完成：后台命令仍在运行，已保留任务 ID 和输出。".into(),
                reasoning: non_empty(reasoning),
                finish_reason: Some("unverified_completion".into()),
            };
        }

        if self.require_image && !self.image_succeeded {
            if self.image_retries < MAX_COMPLETION_RETRIES {
                self.image_retries += 1;
                push_completion_feedback(
                    request,
                    user_msg_index,
                    content,
                    reasoning,
                    IMAGE_REQUIRED_CHALLENGE,
                );
                return ChallengeOutcome::ContinueWithChallenge {
                    status_kind: "require_image".to_string(),
                };
            }
            return ChallengeOutcome::Finish {
                content: image_missing_message(&content),
                reasoning: non_empty(reasoning),
                finish_reason: Some("missing_image".to_string()),
            };
        }

        if finish_reason.as_deref() == Some("length")
            && self.length_retries
                < if content.trim().is_empty() {
                    MAX_COMPLETION_RETRIES
                } else {
                    3
                }
        {
            self.length_retries += 1;
            let feedback = if content.trim().is_empty() {
                LENGTH_EXCEEDED_CHALLENGE
            } else {
                "[System] The previous response was truncated by the output limit. Continue from where it stopped, without repeating its text. Complete remaining work before finishing."
            };
            push_completion_feedback(request, user_msg_index, content, reasoning, feedback);
            return ChallengeOutcome::ContinueWithChallenge {
                status_kind: "length_no_tools".to_string(),
            };
        }

        if !self.mutation_succeeded
            && !crate::runtime::tool::is_question_only_request(request)
            && has_completion_claim(&content)
            && self.empty_completion_retries < MAX_COMPLETION_RETRIES
        {
            self.empty_completion_retries += 1;
            push_completion_feedback(
                request,
                user_msg_index,
                content,
                reasoning,
                COMPLETION_CHALLENGE,
            );
            return ChallengeOutcome::ContinueWithChallenge {
                status_kind: "reject_empty_completion".to_string(),
            };
        }

        if self.mutation_succeeded
            && !self.goal_paths.is_empty()
            && !self.goal_paths_covered()
            && self.goal_coverage_retries < MAX_COMPLETION_RETRIES
        {
            self.goal_coverage_retries += 1;
            push_completion_feedback(
                request,
                user_msg_index,
                content,
                reasoning,
                GOAL_COVERAGE_CHALLENGE,
            );
            return ChallengeOutcome::ContinueWithChallenge {
                status_kind: "goal_coverage".to_string(),
            };
        }

        if self.mutation_succeeded
            && !self.verification_succeeded
            && self.verification_retries < MAX_COMPLETION_RETRIES
        {
            self.verification_retries += 1;
            push_completion_feedback(
                request,
                user_msg_index,
                content,
                reasoning,
                VERIFICATION_CHALLENGE,
            );
            return ChallengeOutcome::ContinueWithChallenge {
                status_kind: "verify_completion".to_string(),
            };
        }

        if (self.open_task_count > 0 || self.open_todo_count > 0)
            && self.open_tasks_retries < MAX_COMPLETION_RETRIES
        {
            self.open_tasks_retries += 1;
            push_completion_feedback(
                request,
                user_msg_index,
                content,
                reasoning,
                &crate::core::chat::agent_loop::turn_end::open_work_message(
                    self.open_task_count > 0,
                    self.open_todo_count > 0,
                ),
            );
            return ChallengeOutcome::ContinueWithChallenge {
                status_kind: "open_tasks".to_string(),
            };
        }

        if self.require_share_deliverable
            && (self.wrote_files || self.started_local_server)
            && !self.shared_deliverable
            && self.share_nudge_retries < MAX_COMPLETION_RETRIES
        {
            self.share_nudge_retries += 1;
            push_completion_feedback(
                request,
                user_msg_index,
                content,
                reasoning,
                SHARE_DELIVERABLE_CHALLENGE,
            );
            return ChallengeOutcome::ContinueWithChallenge {
                status_kind: "share_deliverable".to_string(),
            };
        }

        let mut final_content = content;
        if final_content.trim().is_empty() {
            final_content = if finish_reason.as_deref() == Some("length") {
                "⚠️ 模型深度思考耗尽了单次输出上限（Token Limit Exceeded），未能生成正文与执行计划。建议在输入时给出明确的分析目标（例如“请直接分析性能瓶颈并制定排查方案”），或输入“请直接输出分析结论与方案”继续。".to_string()
            } else {
                "⚠️ 模型完成了思考，但未能生成正文内容。您可以输入“请根据上述思考直接输出方案”让其继续。".to_string()
            };
        }
        let completion_rejected = reject_unverified_completion(
            &mut final_content,
            request,
            self.mutation_succeeded,
            self.verification_succeeded,
            self.open_task_count > 0 || self.open_todo_count > 0,
        );
        ChallengeOutcome::Finish {
            content: final_content,
            reasoning: non_empty(reasoning),
            finish_reason: if completion_rejected {
                Some("unverified_completion".to_string())
            } else {
                finish_reason
            },
        }
    }

    pub(super) fn goal_paths_covered(&self) -> bool {
        if self.goal_paths.is_empty() {
            return true;
        }
        self.goal_paths.iter().all(|goal| {
            self.mutated_paths
                .iter()
                .any(|mutated| paths_match(goal, mutated))
        })
    }
}
