mod answer;
mod progress;

pub(crate) use crate::core::chat::agent_loop::challenge::paths::{
    extract_goal_paths, is_share_tool, is_write_tool, looks_like_local_dev_server, paths_match,
    provides_verification_evidence,
};
pub(crate) use crate::core::chat::agent_loop::challenge::MAX_NO_PROGRESS_REPEATS;
pub use crate::core::chat::agent_loop::challenge::ChallengeOutcome;
pub use crate::core::chat::agent_loop::types::ToolOutcome;
pub use crate::core::runtime::{ChatMessage, ChatRequest, MessageStatus, Role};
pub use crate::runtime::ToolManager;
pub use super::CompletionGate;

pub fn empty_request() -> ChatRequest {
        ChatRequest {
            request_id: "r".into(),
            session_id: "s".into(),
            messages: vec![],
            context: crate::core::runtime::RequestContext::default(),
            provider: None,
            stream: true,
            tools: std::sync::Arc::from([]),
            temperature: None,
            max_tokens: None,
        }
    }

