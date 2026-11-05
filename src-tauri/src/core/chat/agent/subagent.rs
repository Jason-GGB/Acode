use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::core::ai::provider::AIProvider;
use crate::core::runtime::{ChatMessage, ChatRequest, MessageStatus, Role, StreamEvent};
use crate::core::tools::context::ToolContext;
use crate::core::tools::error::ToolError;
use crate::core::tools::registry::ToolRegistry;
use crate::runtime::ToolManager;

use super::super::agent_loop::types::now_millis;
use super::AgentRunner;

impl AgentRunner {
    pub async fn run_subagent(
        provider: Arc<dyn AIProvider>,
        registry: Arc<ToolRegistry>,
        tool_ctx: ToolContext,
        prompt: String,
        read_only: bool,
    ) -> Result<String, ToolError> {
        let mut tool_ctx = tool_ctx;
        tool_ctx.provider = Some(Arc::clone(&provider));
        let active_tools = Arc::new(ToolManager::new(registry.filter_for_subagent(read_only)));
        tool_ctx.registry = Some(active_tools.registry());
        let runner = AgentRunner::new(provider, active_tools).with_max_turn_tokens(48_000);
        let (tx, mut rx) = mpsc::channel::<StreamEvent>(64);
        let cancelled = Arc::clone(&tool_ctx.cancelled);
        let soft_queue = Arc::new(Mutex::new(VecDeque::new()));
        let request = ChatRequest {
            request_id: format!("sub-{}", now_millis()),
            session_id: tool_ctx.session_id.clone(),
            messages: vec![ChatMessage {
                id: format!("msg-{}", now_millis()),
                session_id: tool_ctx.session_id.clone(),
                role: Role::User,
                content: prompt,
                reasoning: None,
                work_timeline: None,
                tool_activities: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
                status: MessageStatus::Done,
                timestamp: now_millis(),
                estimated_tokens: None,
            }],
            context: Default::default(),
            provider: None,
            stream: true,
            tools: std::sync::Arc::from([]),
            temperature: None,
            max_tokens: None,
        };

        // Spawn a background task to receive from rx concurrently to avoid channel deadlocks.
        let answer = Arc::new(tokio::sync::Mutex::new((
            String::new(),
            Option::<String>::None,
        )));
        let answer_clone = Arc::clone(&answer);
        let progress_bus = Arc::clone(&tool_ctx.event_bus);
        let progress_subagent_id = tool_ctx.subagent_id.clone();
        let rx_task = tauri::async_runtime::spawn(async move {
            let mut response_reported = false;
            let mut reasoning_reported = false;
            while let Some(event) = rx.recv().await {
                match event {
                    StreamEvent::TurnComplete {
                        content,
                        finish_reason,
                        ..
                    } => {
                        let mut lock = answer_clone.lock().await;
                        lock.0 = content;
                        lock.1 = finish_reason;
                    }
                    StreamEvent::Delta(delta) => {
                        if !response_reported {
                            if let Some(subagent_id) = &progress_subagent_id {
                                progress_bus.emit(crate::core::event::BusEvent::SubagentProgress {
                                    subagent_id: subagent_id.clone(),
                                    kind: "responding".to_string(),
                                    content: "Generating response".to_string(),
                                    timestamp_ms: now_millis(),
                                });
                            }
                            response_reported = true;
                        }
                        let mut lock = answer_clone.lock().await;
                        lock.0.push_str(&delta);
                    }
                    StreamEvent::Reasoning(_) => {
                        if !reasoning_reported {
                            if let Some(subagent_id) = &progress_subagent_id {
                                progress_bus.emit(crate::core::event::BusEvent::SubagentProgress {
                                    subagent_id: subagent_id.clone(),
                                    kind: "reasoning".to_string(),
                                    content: "Reasoning".to_string(),
                                    timestamp_ms: now_millis(),
                                });
                            }
                            reasoning_reported = true;
                        }
                    }
                    StreamEvent::Usage(usage) => {
                        progress_bus.emit(crate::core::event::BusEvent::TokenUsage {
                            session_id: None,
                            message_id: None,
                            model: "subagent".to_string(),
                            usage,
                        });
                    }
                    StreamEvent::Status { kind } => {
                        if let Some(subagent_id) = &progress_subagent_id {
                            progress_bus.emit(crate::core::event::BusEvent::SubagentProgress {
                                subagent_id: subagent_id.clone(),
                                kind: "status".to_string(),
                                content: kind,
                                timestamp_ms: now_millis(),
                            });
                        }
                    }
                    _ => {}
                }
            }
        });

        Box::pin(runner.run(request, tool_ctx, tx, cancelled, soft_queue))
            .await
            .map_err(|error| ToolError::new(error.to_string()))?;

        // Wait for the receiver task to finish draining
        let _ = rx_task.await;

        let (final_answer, finish_reason) = answer.lock().await.clone();
        if finish_reason.as_deref() == Some("tool_failure_breaker") {
            return Err(ToolError::new(final_answer));
        }
        Ok(final_answer)
    }
}
