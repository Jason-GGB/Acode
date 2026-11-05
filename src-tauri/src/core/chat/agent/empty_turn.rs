use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::core::ai::provider::ProviderError;
use crate::core::chat::limits::estimate_tokens;
use crate::core::runtime::{ChatMessage, ChatRequest, MessageStatus, Role, StreamEvent};
use crate::core::tools::context::ToolContext;

use super::super::agent_loop::challenge::push_challenge_message;
use super::super::agent_loop::post_edit_verify::{verify_feedback_content, VerificationQueue};
use super::super::agent_loop::soft_inject::drain_soft_injects;
use super::super::agent_loop::task_state::TaskState;
use super::super::agent_loop::turn_end::{Answer, Decision, TurnEnd};
use super::super::agent_loop::types::now_millis;

impl super::AgentRunner {
    pub(super) async fn finish_empty_turn(
        &self,
        request: &mut ChatRequest,
        user_msg_index: &mut Option<usize>,
        turn_end: &mut TurnEnd,
        task_state: &mut TaskState,
        verification_queue: &mut VerificationQueue,
        criteria_retries: &mut u32,
        steps: &mut u32,
        used_tokens: &mut usize,
        tool_ctx: &ToolContext,
        tx: &mpsc::Sender<StreamEvent>,
        cancelled: &Arc<AtomicBool>,
        soft_queue: &Arc<Mutex<VecDeque<String>>>,
        content: String,
        reasoning: String,
        finish_reason: Option<String>,
    ) -> Result<super::AfterStep, ProviderError> {
        let unresolved = task_state.unresolved_criteria();
        let mut decision = turn_end.decide(
            request,
            user_msg_index,
            Answer {
                content: content.clone(),
                reasoning: reasoning.clone(),
                finish_reason: finish_reason.clone(),
                paused: task_state.execution_paused(),
                unresolved_criteria: unresolved.clone(),
                criteria_retries: *criteria_retries,
            },
        );
        if matches!(decision, Decision::Verify) {
            let mut pending = std::mem::take(verification_queue);
            let verify_ctx = tool_ctx.clone();
            let report =
                tauri::async_runtime::spawn_blocking(move || pending.run_pending(&verify_ctx))
                    .await
                    .map_err(|e| {
                        ProviderError::message(format!("verification task failed: {e}"))
                    })?;
            if cancelled.load(Ordering::Relaxed) {
                return Err(ProviderError::cancelled());
            }
            turn_end.note_verification_drained();
            turn_end
                .gate_mut()
                .note_verified_paths(report.verified_paths);
            if !report.outcomes.is_empty() {
                for outcome in report.outcomes {
                    task_state.record(&outcome);
                    *used_tokens += estimate_tokens(&outcome.result);
                    tool_ctx.conversation.journal().record_tool_outcome(
                        tool_ctx.root_session_id(),
                        &request.request_id,
                        &tool_ctx.assistant_message_id,
                        &outcome.tool_name,
                        &outcome.arguments,
                        outcome.success,
                        &outcome.result,
                    );
                    push_challenge_message(
                        request,
                        user_msg_index,
                        &verify_feedback_content(&outcome),
                    );
                }
                *steps += 1;
                return Ok(super::AfterStep::Continue);
            }
            decision = turn_end.decide(
                request,
                user_msg_index,
                Answer {
                    content,
                    reasoning,
                    finish_reason,
                    paused: task_state.execution_paused(),
                    unresolved_criteria: unresolved,
                    criteria_retries: *criteria_retries,
                },
            );
        }
        match decision {
            Decision::Verify => return Ok(super::AfterStep::Continue),
            Decision::Continue { status_kind, .. } => {
                if status_kind == "acceptance_criteria" {
                    *criteria_retries += 1;
                } else {
                    let _ = tx.send(StreamEvent::Status { kind: status_kind }).await;
                }
                *steps += 1;
                return Ok(super::AfterStep::Continue);
            }
            Decision::Finish {
                content,
                reasoning,
                finish_reason,
                drain_followups,
            } => {
                if drain_followups {
                    request.messages.push(ChatMessage {
                        id: format!("msg-{}", now_millis()),
                        session_id: request.session_id.clone(),
                        role: Role::Assistant,
                        content: content.clone(),
                        reasoning: reasoning.clone(),
                        work_timeline: None,
                        tool_activities: None,
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                        status: MessageStatus::Done,
                        timestamp: now_millis(),
                        estimated_tokens: None,
                    });
                    if drain_soft_injects(&soft_queue, request, &tx, user_msg_index).await {
                        *steps += 1;
                        return Ok(super::AfterStep::Continue);
                    }
                    tokio::task::yield_now().await;
                    if drain_soft_injects(&soft_queue, request, &tx, user_msg_index).await {
                        *steps += 1;
                        return Ok(super::AfterStep::Continue);
                    }
                }
                let _ = tx
                    .send(StreamEvent::TurnComplete {
                        content,
                        reasoning,
                        tool_calls: vec![],
                        finish_reason,
                    })
                    .await;
                return Ok(super::AfterStep::Break);
            }
        }
    }
}
