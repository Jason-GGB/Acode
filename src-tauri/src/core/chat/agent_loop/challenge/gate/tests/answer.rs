use super::*;

#[test]
fn image_mode_rejects_caption_without_generate_image() {
        let mut gate = CompletionGate::new();
        gate.require_image();
        let mut request = empty_request();
        let mut user_idx = None;
        let first = gate.evaluate_final_answer(
            &mut request,
            &mut user_idx,
            "新图来了 📸 衣服换成了雪碧风格。".into(),
            String::new(),
            Some("stop".into()),
        );
        match first {
            ChallengeOutcome::ContinueWithChallenge { status_kind } => {
                assert_eq!(status_kind, "require_image");
            }
            other => panic!("expected require_image challenge, got {other:?}"),
        }
        let second = gate.evaluate_final_answer(
            &mut request,
            &mut user_idx,
            "已经生成好了。".into(),
            String::new(),
            Some("stop".into()),
        );
        match second {
            ChallengeOutcome::Finish {
                content,
                finish_reason,
                ..
            } => {
                assert!(content.contains("未能生成图片"));
                assert_eq!(finish_reason.as_deref(), Some("missing_image"));
            }
            other => panic!("expected missing_image finish, got {other:?}"),
        }
    }

    #[test]
    fn verification_requires_read_of_mutated_path() {
        use crate::core::tools::context::Tool;
        use crate::core::tools::registry::ToolRegistry;
        use crate::runtime::ToolManager;
        use std::sync::Arc;

        struct WriteTool;
        impl Tool for WriteTool {
            fn name(&self) -> &str {
                "write_file"
            }
            fn description(&self) -> &str {
                "w"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            fn read_only(&self) -> bool {
                false
            }
            fn execute(
                &self,
                _ctx: &crate::core::tools::context::ToolContext,
                _args: serde_json::Value,
            ) -> Result<String, crate::core::tools::error::ToolError> {
                Ok("ok".into())
            }
        }
        struct ReadTool;
        impl Tool for ReadTool {
            fn name(&self) -> &str {
                "read_file"
            }
            fn description(&self) -> &str {
                "r"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            fn read_only(&self) -> bool {
                true
            }
            fn execute(
                &self,
                _ctx: &crate::core::tools::context::ToolContext,
                _args: serde_json::Value,
            ) -> Result<String, crate::core::tools::error::ToolError> {
                Ok("ok".into())
            }
        }

        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(WriteTool));
        registry.register(Arc::new(ReadTool));
        let tools = ToolManager::new(registry);
        let mut gate = CompletionGate::new();
        gate.record_tool_outcomes(
            &tools,
            &[
                ToolOutcome {
                    call_id: "1".into(),
                    tool_name: "write_file".into(),
                    arguments: r#"{"path":"src/a.txt","content":"x"}"#.into(),
                    result: "ok".into(),
                    success: true,
                    user_denied: false,
                    plan_gate: false,
                },
                ToolOutcome {
                    call_id: "2".into(),
                    tool_name: "read_file".into(),
                    arguments: r#"{"path":"noise.txt"}"#.into(),
                    result: "noise".into(),
                    success: true,
                    user_denied: false,
                    plan_gate: false,
                },
            ],
        );
        let mut request = empty_request();
        let mut user_idx = None;
        match gate.evaluate_final_answer(
            &mut request,
            &mut user_idx,
            "here is the change".into(),
            String::new(),
            Some("stop".into()),
        ) {
            ChallengeOutcome::ContinueWithChallenge { status_kind } => {
                assert_eq!(status_kind, "verify_completion");
            }
            other => panic!("expected verify_completion, got {other:?}"),
        }
    }

    #[test]
    fn goal_coverage_requires_all_paths() {
        use crate::core::tools::context::Tool;
        use crate::core::tools::registry::ToolRegistry;
        use crate::runtime::ToolManager;
        use std::sync::Arc;

        struct WriteTool;
        impl Tool for WriteTool {
            fn name(&self) -> &str {
                "write_file"
            }
            fn description(&self) -> &str {
                "w"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            fn read_only(&self) -> bool {
                false
            }
            fn execute(
                &self,
                _ctx: &crate::core::tools::context::ToolContext,
                _args: serde_json::Value,
            ) -> Result<String, crate::core::tools::error::ToolError> {
                Ok("ok".into())
            }
        }

        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(WriteTool));
        let tools = ToolManager::new(registry);
        let mut gate = CompletionGate::new();
        let mut request = empty_request();
        request.messages.push(ChatMessage {
            id: "u".into(),
            session_id: "s".into(),
            role: Role::User,
            content: "edit a.txt and b.txt".into(),
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
        gate.capture_goal_from_request(&request);
        gate.record_tool_outcome(
            &tools,
            &ToolOutcome {
                call_id: "1".into(),
                tool_name: "write_file".into(),
                arguments: r#"{"path":"a.txt","content":"1"}"#.into(),
                result: "ok".into(),
                success: true,
                user_denied: false,
                plan_gate: false,
            },
        );
        let mut user_idx = None;
        match gate.evaluate_final_answer(
            &mut request,
            &mut user_idx,
            "done".into(),
            String::new(),
            Some("stop".into()),
        ) {
            ChallengeOutcome::ContinueWithChallenge { status_kind } => {
                assert_eq!(status_kind, "goal_coverage");
            }
            other => panic!("expected goal_coverage, got {other:?}"),
        }
    }

    #[test]
    fn skips_plan_approve_when_capturing_goals() {
        let mut gate = CompletionGate::new();
        let mut request = empty_request();
        request.messages.push(ChatMessage {
            id: "u1".into(),
            session_id: "s".into(),
            role: Role::User,
            content: "Please edit `src/main.rs`".into(),
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
        request.messages.push(ChatMessage {
            id: "u2".into(),
            session_id: "s".into(),
            role: Role::User,
            content: "计划已批准。现在按批准的计划执行，本回合写操作已解除限制。".into(),
            reasoning: None,
            work_timeline: None,
            tool_activities: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
            status: MessageStatus::Done,
            timestamp: 1,
            estimated_tokens: None,
        });
        gate.capture_goal_from_request(&request);
        assert!(gate.goal_paths.iter().any(|p| p.contains("main.rs")));
    }

    #[test]
    fn write_outcome_records_mutated_path() {
        use crate::core::tools::context::Tool;
        use crate::core::tools::registry::ToolRegistry;
        use crate::runtime::ToolManager;
        use std::sync::Arc;

        struct WriteTool;
        impl Tool for WriteTool {
            fn name(&self) -> &str {
                "write_file"
            }
            fn description(&self) -> &str {
                "w"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            fn read_only(&self) -> bool {
                false
            }
            fn execute(
                &self,
                _ctx: &crate::core::tools::context::ToolContext,
                _args: serde_json::Value,
            ) -> Result<String, crate::core::tools::error::ToolError> {
                Ok("ok".into())
            }
        }

        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(WriteTool));
        let tools = ToolManager::new(registry);
        let mut gate = CompletionGate::new();
        let mut request = empty_request();
        request.messages.push(ChatMessage {
            id: "u".into(),
            session_id: "s".into(),
            role: Role::User,
            content: "Write src/a.txt to alpha.".into(),
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
        gate.capture_goal_from_request(&request);
        assert_eq!(gate.goal_paths.len(), 1, "goals: {:?}", gate.goal_paths);
        gate.record_tool_outcome(
            &tools,
            &ToolOutcome {
                call_id: "1".into(),
                tool_name: "write_file".into(),
                arguments: r#"{"path":"src/a.txt","content":"alpha"}"#.into(),
                result: "written".into(),
                success: true,
                user_denied: false,
                plan_gate: false,
            },
        );
        assert!(
            gate.goal_paths_covered(),
            "mutated={:?} goals={:?}",
            gate.mutated_paths,
            gate.goal_paths
        );
        let mut user_idx = None;
        match gate.evaluate_final_answer(
            &mut request,
            &mut user_idx,
            "任务完成".into(),
            String::new(),
            Some("stop".into()),
        ) {
            ChallengeOutcome::ContinueWithChallenge { status_kind } => {
                assert_ne!(status_kind, "goal_coverage");
                assert_eq!(status_kind, "verify_completion");
            }
            other => panic!("expected verify_completion, got {other:?}"),
        }
    }

    #[test]
    fn length_with_empty_content_is_challenged_once() {
        let mut gate = CompletionGate::new();
        let mut request = empty_request();
        let mut user_idx = None;
        match gate.evaluate_final_answer(
            &mut request,
            &mut user_idx,
            String::new(),
            "a".repeat(200),
            Some("length".into()),
        ) {
            ChallengeOutcome::ContinueWithChallenge { status_kind } => {
                assert_eq!(status_kind, "length_no_tools");
            }
            other => panic!("expected length_no_tools, got {other:?}"),
        }
        match gate.evaluate_final_answer(
            &mut request,
            &mut user_idx,
            String::new(),
            String::new(),
            Some("length".into()),
        ) {
            ChallengeOutcome::Finish {
                content,
                finish_reason,
                ..
            } => {
                assert!(content.contains("Token Limit"), "{content}");
                assert_eq!(finish_reason.as_deref(), Some("length"));
            }
            other => panic!("expected finish after retry, got {other:?}"),
        }
    }
