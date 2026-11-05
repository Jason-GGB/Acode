use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use tracing::Instrument;

use crate::core::ai::provider::{AIProvider, ProviderError};
use crate::core::chat::limits::{
    DEFAULT_MAX_STEPS, DEFAULT_MAX_TURN_TOKENS, TOOL_OUTPUT_MAX_CHARS,
};
use crate::core::runtime::{ChatRequest, StreamEvent};
use crate::core::tools::context::ToolContext;
use crate::runtime::ToolManager;

use super::agent_loop::mid_turn_compact;

mod empty_turn;
mod recovery;
mod run_loop;
mod subagent;
mod tool_batch;

pub(super) enum AfterStep {
    Continue,
    Break,
    Return,
}

pub struct AgentRunner {
    provider: Arc<dyn AIProvider>,
    tools: Arc<ToolManager>,
    max_steps: u32,
    max_turn_tokens: usize,
    tool_output_max_chars: usize,
}

impl AgentRunner {
    /// Create a runner with default step / token / tool-output limits.
    pub fn new(provider: Arc<dyn AIProvider>, tools: Arc<ToolManager>) -> Self {
        Self {
            provider,
            tools,
            max_steps: DEFAULT_MAX_STEPS,
            max_turn_tokens: DEFAULT_MAX_TURN_TOKENS,
            tool_output_max_chars: TOOL_OUTPUT_MAX_CHARS,
        }
    }

    /// Override the per-turn token budget used for soft truncation.
    pub fn with_max_turn_tokens(mut self, max_turn_tokens: usize) -> Self {
        self.max_turn_tokens = max_turn_tokens;
        self
    }

    /// Override the max tool-loop steps (`0` = unlimited).
    pub fn with_max_steps(mut self, max_steps: u32) -> Self {
        self.max_steps = max_steps;
        self
    }

    #[cfg(test)]
    pub fn with_limits(
        provider: Arc<dyn AIProvider>,
        tools: Arc<ToolManager>,
        max_steps: u32,
        max_turn_tokens: usize,
        tool_output_max_chars: usize,
    ) -> Self {
        Self {
            provider,
            tools,
            max_steps,
            max_turn_tokens,
            tool_output_max_chars,
        }
    }

    /// Drive the agent loop: stream model output, execute tools, enforce
    /// completion / verification challenges, and honor cancellation.
    ///
    /// This is orchestration only — the actual policies live in
    /// `super::agent_loop` so new turn behaviors can be added there without
    /// growing this function.
    pub async fn run(
        &self,
        request: ChatRequest,
        tool_ctx: ToolContext,
        tx: mpsc::Sender<StreamEvent>,
        cancelled: Arc<AtomicBool>,
        soft_queue: Arc<Mutex<VecDeque<String>>>,
    ) -> Result<(), ProviderError> {
        let span = tracing::info_span!(
            target: "peek.agent",
            "agent_run",
            session_id = %request.session_id,
            request_id = %request.request_id,
            provider = ?request.provider,
        );
        if self.provider.uses_dsh_tools() && !self.tools.is_dsh() {
            let adapted = Self {
                provider: Arc::clone(&self.provider),
                tools: Arc::new(self.tools.dsh_contract()),
                max_steps: self.max_steps,
                max_turn_tokens: self.max_turn_tokens,
                tool_output_max_chars: self.tool_output_max_chars,
            };
            let mut context = tool_ctx;
            context.registry = Some(adapted.tools.registry());
            return adapted
                .run_loop(request, context, tx, cancelled, soft_queue)
                .instrument(span)
                .await;
        }
        self.run_loop(request, tool_ctx, tx, cancelled, soft_queue)
            .instrument(span)
            .await
    }
}

fn persist_mid_turn_compact(
    tool_ctx: &ToolContext,
    outcome: mid_turn_compact::MidTurnCompactOutcome,
) {
    let Some(before_id) = outcome.persist_before_id else {
        return;
    };
    tool_ctx.conversation.insert_compaction_summary(
        tool_ctx.root_session_id(),
        outcome.summary,
        Some(before_id.as_str()),
    );
}
