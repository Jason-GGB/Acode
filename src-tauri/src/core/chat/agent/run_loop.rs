use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use tracing::Instrument;

use crate::core::ai::provider::ProviderError;
use crate::core::runtime::{ChatMessage, ChatRequest, MessageStatus, Role, StreamEvent};
use crate::core::tools::context::ToolContext;

use super::super::agent_loop::failure::FailureBreaker;
use super::super::agent_loop::mid_turn_compact;
use super::super::agent_loop::post_edit_verify::VerificationQueue;
use super::super::agent_loop::soft_inject::drain_soft_injects;
use super::super::agent_loop::stream_turn::StreamTurnResult;
use super::super::agent_loop::tools::ToolExecutor;
use super::super::agent_loop::turn_end::TurnEnd;
use super::super::agent_loop::types::estimate_request_tokens;
use crate::core::chat::limits::estimate_tokens;

impl super::AgentRunner {
    pub(super) async fn run_loop(
        &self,
        mut request: ChatRequest,
        tool_ctx: ToolContext,
        tx: mpsc::Sender<StreamEvent>,
        cancelled: Arc<AtomicBool>,
        soft_queue: Arc<Mutex<VecDeque<String>>>,
    ) -> Result<(), ProviderError> {
        let mut tool_executor =
            ToolExecutor::new(Arc::clone(&self.tools), self.tool_output_max_chars);
        if self.tools.is_dsh() {
            let text = crate::core::tools::dsh::tool_prompt(&self.tools.schemas());
            let mut guidance = request
                .messages
                .first()
                .cloned()
                .unwrap_or_else(|| ChatMessage {
                    id: String::new(),
                    session_id: request.session_id.clone(),
                    role: Role::System,
                    content: String::new(),
                    reasoning: None,
                    work_timeline: None,
                    tool_activities: None,
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    status: MessageStatus::Done,
                    timestamp: 0,
                    estimated_tokens: None,
                });
            if request
                .messages
                .first()
                .is_none_or(|m| m.role != Role::System)
            {
                let mut identity = guidance.clone();
                identity.id = format!("dsh-identity-{}", request.session_id);
                identity.role = Role::System;
                identity.content = crate::core::tools::dsh::IDENTITY.trim().into();
                request.messages.insert(0, identity);
            }
            guidance.id = format!("dsh-tools-{}", request.session_id);
            guidance.role = Role::System;
            guidance.content = text;
            guidance.reasoning = None;
            guidance.tool_calls = None;
            guidance.tool_call_id = None;
            request.messages.insert(1, guidance);
            if self
                .tools
                .registry()
                .names()
                .iter()
                .any(|name| name == "skill")
            {
                let mut catalog = request.messages[1].clone();
                catalog.id = format!("dsh-skills-{}", request.session_id);
                catalog.content = crate::core::tools::dsh::skill_catalog();
                request.messages.insert(2, catalog);
            }
        }
        let mut steps = 0u32;
        let mut context_recoveries = 0u32;
        let mut failure_breaker = FailureBreaker::new();
        let mut turn_end = TurnEnd::new();
        turn_end
            .gate_mut()
            .set_workspace_root(tool_ctx.workspace_root.clone());
        let mut verification_queue = VerificationQueue::default();
        let mut task_state = super::super::agent_loop::task_state::TaskState::new(&request);
        let mut criteria_retries = 0u32;
        let mut progress_evidence = std::collections::HashSet::new();
        let mut discovered_tools = std::collections::HashSet::new();
        if crate::core::tools::image_mode::is_image_mode(&request.session_id) {
            turn_end.gate_mut().require_image();
        }
        if crate::core::chat::session_origin::shared_session_origin_store()
            .is_companion(tool_ctx.root_session_id())
        {
            turn_end.gate_mut().require_share_deliverable();
        }
        turn_end.gate_mut().capture_goal_from_request(&request);
        let mut user_msg_index = request
            .messages
            .iter()
            .rposition(|msg| msg.role == Role::User);
        let mut used_tokens = estimate_request_tokens(&request);
        let mut last_compact_msg_len = 0usize;
        // Freeze tool schemas for the whole turn. Mid-turn Plan accept must not
        // rebuild `tools` (DeepSeek disk cache). Plan writers are blocked by
        // authorize, not by shrinking this frozen set.
        let mut frozen_tools: Option<std::sync::Arc<[serde_json::Value]>> = None;

        loop {
            let permission_mode = crate::core::tools::tool_approval::shared_tool_approval_store()
                .mode_for_session(tool_ctx.root_session_id());
            let policy = match permission_mode {
                crate::models::settings::ToolApprovalMode::Ask => "Read-only: filesystem mutations and arbitrary shell execution are denied. Reading and session bookkeeping are allowed. User approval cannot elevate this mode; the user must switch permission mode.",
                crate::models::settings::ToolApprovalMode::Auto => "Workspace-write: native file tools can modify the workspace. External paths retain their permission gate. Shell commands run without OS filesystem isolation and require separate approval for each invocation. Grants apply only once.",
                crate::models::settings::ToolApprovalMode::AlwaysAllow => "Full-access: tools can operate outside the workspace without interactive approval. Explicit tool safety rules still apply.",
            };
            if !request
                .messages
                .iter()
                .any(|message| message.id == "permission-policy" && message.content == policy)
            {
                request
                    .messages
                    .retain(|message| message.id != "permission-policy");
                let mut message = crate::core::chat::conversation_manager::create_message(
                    &request.session_id,
                    Role::System,
                    policy.into(),
                    MessageStatus::Done,
                );
                message.id = "permission-policy".into();
                request.messages.insert(0, message);
            }
            if cancelled.load(Ordering::Relaxed) {
                return Err(ProviderError::cancelled());
            }
            drain_soft_injects(&soft_queue, &mut request, &tx, &mut user_msg_index).await;
            if task_state.observe_followups(&request) {
                turn_end.gate_mut().capture_goal_from_request(&request);
            }

            if self.max_steps > 0 && steps >= self.max_steps {
                let _ = tx
                    .send(StreamEvent::TurnComplete {
                        content: format!(
                            "已停止：本轮达到最大工具步数上限（{}）。可发送「继续」让我接着做未完成的部分。",
                            self.max_steps
                        ),
                        reasoning: None,
                        tool_calls: vec![],
                        finish_reason: Some("max_steps".to_string()),
                    })
                    .await;
                break;
            }

            if let Some(mut outcome) = mid_turn_compact::maybe_compact(
                &self.provider,
                self.max_turn_tokens,
                &mut request,
                &mut user_msg_index,
                &mut used_tokens,
                &mut last_compact_msg_len,
                &tx,
            )
            .await
            {
                outcome.summary.content.push_str(&format!(
                    "\n\n[Preserved task state]\n{}",
                    task_state.snapshot()
                ));
                if let Some(message) = request
                    .messages
                    .iter_mut()
                    .find(|m| m.id == outcome.summary.id)
                {
                    message.content = outcome.summary.content.clone();
                }
                super::persist_mid_turn_compact(&tool_ctx, outcome);
            }

            if !self.tools.is_dsh() {
                task_state.inject(&mut request);
            } else {
                let plan_id = format!("dsh-plan-{}", request.session_id);
                request.messages.retain(|m| m.id != plan_id);
                if crate::core::tools::plan_mode::shared_plan_mode_store()
                    .is_active(tool_ctx.root_session_id())
                {
                    let mut policy = request.messages[0].clone();
                    policy.id = plan_id;
                    policy.content = "Plan mode is active. Inspect and research with read-only tools. Do not modify files or run shell commands. Present the complete markdown plan with exit_plan_mode for user review. Continue execution only after approval. Delegated agents must remain read-only.".into();
                    request
                        .messages
                        .insert(3.min(request.messages.len()), policy);
                }
                for notice in crate::core::tools::dsh::completion_notices(&tool_ctx.session_id) {
                    if let Some(mut message) = request.messages.last().cloned() {
                        message.id = uuid::Uuid::new_v4().to_string();
                        message.role = Role::User;
                        message.content = notice;
                        message.reasoning = None;
                        message.tool_calls = None;
                        message.tool_call_id = None;
                        message.name = None;
                        request.messages.push(message);
                    }
                }
            }
            let tools = frozen_tools
                .get_or_insert_with(|| {
                    self.tools.focused_schemas(
                        &request,
                        tool_ctx.root_session_id(),
                        &discovered_tools,
                    )
                })
                .clone();
            request.tools = tools;
            tool_executor.set_allowed_tools(
                request
                    .tools
                    .iter()
                    .filter_map(|schema| schema["function"]["name"].as_str().map(str::to_owned)),
            );
            if !self.tools.is_dsh() {
                crate::core::chat::prompt::ensure_plan_mode_prompt(
                    &mut request.messages,
                    tool_ctx.root_session_id(),
                );
            }
            let stream_turn_span = tracing::info_span!(
                target: "peek.agent",
                "agent.stream_turn",
                session_id = %request.session_id,
                step = steps,
            );
            let turn_result = self
                .stream_with_recovery(&request, &tx, &cancelled)
                .instrument(stream_turn_span)
                .await;
            let turn = match turn_result {
                Ok(turn) => turn,
                Err(error) => {
                    // Reactive compaction: the provider rejected the request for
                    // exceeding the context window. Fold history and retry only
                    // while each recovery actually reduces the request size,
                    // instead of hard-failing the turn.
                    if error.is_context_window_exceeded() && context_recoveries < 3 {
                        context_recoveries += 1;
                        let before_tokens = estimate_request_tokens(&request);
                        if let Some(mut outcome) = mid_turn_compact::force_compact(
                            &self.provider,
                            self.max_turn_tokens,
                            &mut request,
                            &mut user_msg_index,
                            &mut used_tokens,
                            &tx,
                        )
                        .await
                        {
                            last_compact_msg_len = request.messages.len();
                            outcome.summary.content.push_str(&format!(
                                "\n\n[Preserved task state]\n{}",
                                task_state.snapshot()
                            ));
                            if let Some(message) = request
                                .messages
                                .iter_mut()
                                .find(|m| m.id == outcome.summary.id)
                            {
                                message.content = outcome.summary.content.clone();
                            }
                            super::persist_mid_turn_compact(&tool_ctx, outcome);
                            if estimate_request_tokens(&request) < before_tokens {
                                let _ = tx
                                    .send(StreamEvent::Status {
                                        kind: format!("stream_retry:{context_recoveries}:3"),
                                    })
                                    .await;
                                continue;
                            }
                        }
                    }
                    return Err(error);
                }
            };
            context_recoveries = 0;
            let StreamTurnResult {
                content,
                reasoning,
                tool_calls,
                finish_reason,
            } = turn;

            used_tokens += estimate_tokens(&content) + estimate_tokens(&reasoning);

            let step = if tool_calls.is_empty() {
                self.finish_empty_turn(
                    &mut request,
                    &mut user_msg_index,
                    &mut turn_end,
                    &mut task_state,
                    &mut verification_queue,
                    &mut criteria_retries,
                    &mut steps,
                    &mut used_tokens,
                    &tool_ctx,
                    &tx,
                    &cancelled,
                    &soft_queue,
                    content,
                    reasoning,
                    finish_reason,
                )
                .await?
            } else {
                self.finish_tool_batch(
                    &mut request,
                    &mut user_msg_index,
                    &mut turn_end,
                    &mut task_state,
                    &mut verification_queue,
                    &mut failure_breaker,
                    &mut tool_executor,
                    &mut criteria_retries,
                    &mut steps,
                    &mut used_tokens,
                    &mut progress_evidence,
                    &mut discovered_tools,
                    &tool_ctx,
                    &tx,
                    &cancelled,
                    &soft_queue,
                    content,
                    reasoning,
                    tool_calls,
                )
                .await?
            };
            match step {
                super::AfterStep::Continue => continue,
                super::AfterStep::Break => break,
                super::AfterStep::Return => return Ok(()),
            }
        }

        Ok(())
    }
}
