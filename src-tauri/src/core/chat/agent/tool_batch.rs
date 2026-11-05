use std::collections::{HashSet, VecDeque};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::core::ai::provider::ProviderError;
use crate::core::chat::limits::estimate_tokens;
use crate::core::runtime::{ChatMessage, ChatRequest, MessageStatus, Role, StreamEvent};
use crate::core::tools::context::ToolContext;

use super::super::agent_loop::challenge::push_challenge_message;
use super::super::agent_loop::failure::{FailureAction, FailureBreaker};
use super::super::agent_loop::post_edit_verify::VerificationQueue;
use super::super::agent_loop::soft_inject::drain_soft_injects;
use super::super::agent_loop::task_state::TaskState;
use super::super::agent_loop::tools::ToolExecutor;
use super::super::agent_loop::turn_end::TurnEnd;
use super::super::agent_loop::types::{non_empty, now_millis};

impl super::AgentRunner {
    pub(super) async fn finish_tool_batch(
        &self,
        request: &mut ChatRequest,
        user_msg_index: &mut Option<usize>,
        turn_end: &mut TurnEnd,
        task_state: &mut TaskState,
        verification_queue: &mut VerificationQueue,
        failure_breaker: &mut FailureBreaker,
        tool_executor: &mut ToolExecutor,
        criteria_retries: &mut u32,
        steps: &mut u32,
        used_tokens: &mut usize,
        progress_evidence: &mut HashSet<(String, String, String)>,
        discovered_tools: &mut HashSet<String>,
        tool_ctx: &ToolContext,
        tx: &mpsc::Sender<StreamEvent>,
        cancelled: &Arc<AtomicBool>,
        soft_queue: &Arc<Mutex<VecDeque<String>>>,
        content: String,
        reasoning: String,
        tool_calls: Vec<crate::core::runtime::ToolCallPayload>,
    ) -> Result<super::AfterStep, ProviderError> {
        let _ = tx
            .send(StreamEvent::Status {
                kind: format!("tools:{}", tool_calls.len()),
            })
            .await;

        let assistant = ChatMessage {
            id: format!("msg-{}", now_millis()),
            session_id: request.session_id.clone(),
            role: Role::Assistant,
            content: content.clone(),
            reasoning: non_empty(reasoning.clone()),
            work_timeline: None,
            tool_activities: None,
            tool_calls: Some(tool_calls.clone()),
            tool_call_id: None,
            name: None,
            status: MessageStatus::Done,
            timestamp: now_millis(),
            estimated_tokens: None,
        };
        request.messages.push(assistant);

        let parallel = tool_executor.should_run_parallel(&tool_calls);

        let outcomes = if parallel {
            tool_executor
                .execute_tools_parallel(&tool_calls, &tool_ctx, &cancelled)
                .await?
        } else {
            tool_executor
                .execute_tools_serial(&tool_calls, &tool_ctx, &cancelled)
                .await?
        };

        let mut user_denied = false;
        turn_end.record(&self.tools, &outcomes);
        verification_queue.record(&outcomes, &tool_ctx.workspace_root);
        for outcome in &outcomes {
            if outcome.success
                && !matches!(outcome.tool_name.as_str(), "update_tasks" | "save_plan")
                && !super::super::agent_loop::turn_end::is_shell_poll(&outcome.tool_name)
                && progress_evidence.insert((
                    outcome.tool_name.clone(),
                    outcome.arguments.clone(),
                    outcome.result.clone(),
                ))
            {
                *criteria_retries = 0;
            }
            task_state.record(outcome);
            if outcome.success && outcome.tool_name == "search_tools" {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&outcome.result) {
                    if let Some(tools) = value["tools"].as_array() {
                        discovered_tools.extend(
                            tools
                                .iter()
                                .filter_map(|s| s["function"]["name"].as_str().map(str::to_string)),
                        );
                    }
                }
            }
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
            request.messages.push(ChatMessage {
                id: format!("msg-{}", now_millis()),
                session_id: request.session_id.clone(),
                role: Role::Tool,
                content: outcome.result.clone(),
                reasoning: None,
                work_timeline: None,
                tool_activities: None,
                tool_calls: None,
                tool_call_id: Some(outcome.call_id.clone()),
                name: Some(outcome.tool_name.clone()),
                status: MessageStatus::Done,
                timestamp: now_millis(),
                estimated_tokens: None,
            });
            if outcome.user_denied {
                user_denied = true;
            }
        }

        match failure_breaker.check(&outcomes) {
            FailureAction::Stop { reason } => {
                let _ = tx
                    .send(StreamEvent::TurnComplete {
                        content: format!("已停止：{reason}"),
                        reasoning: None,
                        tool_calls: vec![],
                        finish_reason: Some("tool_failure_breaker".to_string()),
                    })
                    .await;
                return Ok(super::AfterStep::Return);
            }
            FailureAction::Challenge {
                status_kind,
                message,
            } => {
                push_challenge_message(request, user_msg_index, &message);
                let _ = tx.send(StreamEvent::Status { kind: status_kind }).await;
            }
            FailureAction::Continue => {}
        }

        if let Some(status_kind) = turn_end
            .gate_mut()
            .maybe_challenge_stall(request, user_msg_index)
        {
            let _ = tx.send(StreamEvent::Status { kind: status_kind }).await;
        }

        if user_denied {
            let _ = tx
                .send(StreamEvent::TurnComplete {
                    content: "已停止：你拒绝了本次工具操作。".to_string(),
                    reasoning: None,
                    tool_calls: vec![],
                    finish_reason: Some("user_denied".to_string()),
                })
                .await;
            return Ok(super::AfterStep::Return);
        }

        if turn_end.gate_mut().repeated_without_progress() {
            let _ = tx.send(StreamEvent::TurnComplete {
                    content: "已暂停：相同工具调用反复返回相同结果，换策略提示后仍未获得新证据。任务尚未完成。".into(),
                    reasoning: None, tool_calls: vec![], finish_reason: Some("no_progress".into()),
                }).await;
            return Ok(super::AfterStep::Break);
        }

        // Soft-inject at tool boundary before the next provider call.
        drain_soft_injects(&soft_queue, request, &tx, user_msg_index).await;
        *steps += 1;
        return Ok(super::AfterStep::Continue);
    }
}
