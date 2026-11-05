use super::*;

#[test]
fn real_progress_replenishes_completion_budget_but_identical_reads_do_not() {
        use super::super::*;
        use super::empty_request;
        use crate::core::chat::agent_loop::challenge::paths::{extract_goal_paths, is_share_tool, is_write_tool, looks_like_local_dev_server};
        let tools = ToolManager::new(crate::core::tools::registry::ToolRegistry::new());
        let mut gate = CompletionGate::new();
        gate.verification_retries = 1;
        let outcome = ToolOutcome {
            call_id: "read1".into(),
            tool_name: "read_file".into(),
            arguments: "{}".into(),
            result: "new evidence".into(),
            success: true,
            user_denied: false,
            plan_gate: false,
        };
        gate.record_tool_outcome(&tools, &outcome);
        assert_eq!(gate.verification_retries, 0);
        gate.verification_retries = 1;
        gate.record_tool_outcome(&tools, &outcome);
        assert_eq!(gate.verification_retries, 1);
    }

    #[test]
    fn finite_background_job_must_exit_before_completion() {
        use super::super::*;
        use super::empty_request;
        use crate::core::chat::agent_loop::challenge::paths::{extract_goal_paths, is_share_tool, is_write_tool, looks_like_local_dev_server};
        let tools = ToolManager::new(crate::core::tools::registry::ToolRegistry::new());
        let mut gate = CompletionGate::new();
        gate.record_tool_outcome(
            &tools,
            &ToolOutcome {
                call_id: "launch".into(),
                tool_name: "run_shell".into(),
                arguments: r#"{"command":"cargo check","run_in_background":true}"#.into(),
                result: "job-1".into(),
                success: true,
                user_denied: false,
                plan_gate: false,
            },
        );
        let mut request = empty_request();
        let mut user_idx = None;
        assert!(matches!(
            gate.evaluate_final_answer(
                &mut request,
                &mut user_idx,
                "done".into(),
                String::new(),
                Some("stop".into())
            ),
            ChallengeOutcome::ContinueWithChallenge { .. }
        ));
        gate.record_tool_outcome(
            &tools,
            &ToolOutcome {
                call_id: "wait".into(),
                tool_name: "wait_for_shell".into(),
                arguments: r#"{"job_id":"job-1"}"#.into(),
                result: "status: done\nexit_code: Some(0)\nchecked".into(),
                success: true,
                user_denied: false,
                plan_gate: false,
            },
        );
        assert!(gate.pending_shell_jobs.is_empty());
        assert!(!looks_like_local_dev_server(
            "run_shell",
            r#"{"command":"pnpm build","run_in_background":true}"#,
            "",
        ));
    }

    #[test]
    fn nonempty_truncated_answer_is_continued() {
        use super::super::*;
        use super::empty_request;
        use crate::core::chat::agent_loop::challenge::paths::{extract_goal_paths, is_share_tool, is_write_tool, looks_like_local_dev_server};
        let mut gate = CompletionGate::new();
        let mut request = empty_request();
        let mut user_idx = None;
        assert!(matches!(
            gate.evaluate_final_answer(
                &mut request,
                &mut user_idx,
                "partial answer".into(),
                String::new(),
                Some("length".into())
            ),
            ChallengeOutcome::ContinueWithChallenge { .. }
        ));
        assert!(request
            .messages
            .iter()
            .any(|m| m.content == "partial answer"));
        assert!(request
            .messages
            .last()
            .unwrap()
            .content
            .contains("Continue from where"));
    }
    use super::super::*;
        use super::empty_request;
        use crate::core::chat::agent_loop::challenge::paths::{extract_goal_paths, is_share_tool, is_write_tool, looks_like_local_dev_server};

    #[test]
    fn code_readback_and_unrelated_or_failed_commands_do_not_verify_code() {
        let mut outcome = ToolOutcome {
            call_id: "1".into(),
            tool_name: "read_file".into(),
            arguments: r#"{"path":"src/lib.rs"}"#.into(),
            result: "ok".into(),
            success: true,
            user_denied: false,
            plan_gate: false,
        };
        let paths = HashSet::from(["src/lib.rs".into()]);
        assert!(!provides_verification_evidence(&outcome, &paths));
        outcome.tool_name = "run_shell".into();
        outcome.arguments = r#"{"command":"npm run check"}"#.into();
        outcome.result = "exit_code: 0\nok".into();
        assert!(!provides_verification_evidence(&outcome, &paths));
        outcome.arguments = r#"{"command":"echo cargo test"}"#.into();
        assert!(!provides_verification_evidence(&outcome, &paths));
        outcome.arguments = r#"{"command":"cargo test"}"#.into();
        outcome.result = "exit_code: 1\nfailed".into();
        assert!(!provides_verification_evidence(&outcome, &paths));
        outcome.result = "exit_code: 0\npassed".into();
        assert!(provides_verification_evidence(&outcome, &paths));
        assert!(!paths_match("a.rs", "data.rs"));
    }

    #[test]
    fn verification_requires_every_changed_file_and_is_invalidated_by_edits() {
        let tools = ToolManager::new(crate::core::tools::registry::ToolRegistry::new());
        let mut gate = CompletionGate::new();
        let write = |path: &str| ToolOutcome {
            call_id: "1".into(),
            tool_name: "write_file".into(),
            arguments: serde_json::json!({"path":path}).to_string(),
            result: "ok".into(),
            success: true,
            user_denied: false,
            plan_gate: false,
        };
        gate.record_tool_outcomes(&tools, &[write("a.rs"), write("b.rs")]);
        gate.note_verified_paths(["a.rs".into()]);
        assert!(!gate.verification_succeeded);
        gate.note_verified_paths(["b.rs".into()]);
        assert!(gate.verification_succeeded);
        gate.record_tool_outcome(&tools, &write("a.rs"));
        assert!(!gate.verification_succeeded);
    }

    #[test]
    fn extracts_goal_paths_from_user_text() {
        let paths = extract_goal_paths("Please edit `src/main.rs` and fix README.md");
        assert!(paths.iter().any(|p| p.contains("main.rs")));
        assert!(paths.iter().any(|p| p.contains("README.md")));
    }

    #[test]
    fn stall_challenge_fires_after_repeated_success() {
        use crate::core::tools::context::Tool;
        use crate::core::tools::registry::ToolRegistry;
        use crate::runtime::ToolManager;
        use std::sync::Arc;

        struct StubTool;
        impl Tool for StubTool {
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
        registry.register(Arc::new(StubTool));
        let tools = ToolManager::new(registry);
        let mut gate = CompletionGate::new();
        let outcome = ToolOutcome {
            call_id: "1".into(),
            tool_name: "read_file".into(),
            arguments: r#"{"path":"a.rs"}"#.into(),
            result: "ok".into(),
            success: true,
            user_denied: false,
            plan_gate: false,
        };
        for _ in 0..MAX_NO_PROGRESS_REPEATS {
            gate.record_tool_outcome(&tools, &outcome);
        }
        assert!(gate.stalled);
    }

    #[test]
    fn detects_background_vite_as_local_server() {
        assert!(looks_like_local_dev_server(
            "run_shell",
            r#"{"command":"npx vite --host","run_in_background":true}"#,
            "",
        ));
        assert!(!looks_like_local_dev_server(
            "run_shell",
            r#"{"command":"npx vite --host","run_in_background":false}"#,
            "",
        ));
        assert!(is_write_tool("write_file"));
        assert!(is_share_tool("share_preview_url"));
    }
