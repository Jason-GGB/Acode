use super::*;
use crate::core::runtime::ChatRequest;

fn tools() -> ToolManager {
    ToolManager::new(crate::core::tools::registry::ToolRegistry::new())
}

fn outcome(name: &str, arguments: &str, result: &str) -> ToolOutcome {
    ToolOutcome {
        call_id: name.into(),
        tool_name: name.into(),
        arguments: arguments.into(),
        result: result.into(),
        success: true,
        user_denied: false,
        plan_gate: false,
    }
}

fn answer(content: &str) -> Answer {
    Answer {
        content: content.into(),
        reasoning: String::new(),
        finish_reason: Some("stop".into()),
        paused: false,
        unresolved_criteria: Vec::new(),
        criteria_retries: 0,
    }
}

fn empty_request() -> ChatRequest {
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

fn decide(end: &mut TurnEnd, answer: Answer) -> Decision {
    let mut request = empty_request();
    end.decide(&mut request, &mut None, answer)
}

#[test]
fn nothing_pending_finishes() {
    let mut end = TurnEnd::new();
    let decision = decide(&mut end, answer("这是目前的结果。"));
    assert!(matches!(
        decision,
        Decision::Finish {
            drain_followups: true,
            finish_reason: Some(reason),
            ..
        } if reason == "stop"
    ));
}

#[test]
fn user_pause_finishes_without_verification() {
    let mut end = TurnEnd::new();
    end.record(
        &tools(),
        &[outcome("write", r#"{"file_path":"a.txt"}"#, "wrote")],
    );
    let mut paused = answer("这是目前的结果。");
    paused.paused = true;
    let decision = decide(&mut end, paused);
    match decision {
        Decision::Finish {
            content,
            finish_reason,
            drain_followups,
            ..
        } => {
            assert_eq!(content, "这是目前的结果。");
            assert_eq!(finish_reason.as_deref(), Some("user_paused"));
            assert!(!drain_followups);
            assert!(!content.contains("验证"));
        }
        other => panic!("expected pause finish, got {other:?}"),
    }
}

#[test]
fn unverified_write_is_not_accepted() {
    let mut end = TurnEnd::new();
    end.record(
        &tools(),
        &[outcome("write", r#"{"file_path":"a.txt"}"#, "wrote")],
    );
    let decision = decide(&mut end, answer("这是目前的结果。"));
    assert!(matches!(decision, Decision::Verify));
}

#[test]
fn pending_bash_job_names_job_output() {
    let mut end = TurnEnd::new();
    end.record(
        &tools(),
        &[outcome(
            "bash",
            r#"{"command":"cargo check","run_in_background":true}"#,
            "[job_id: job-9]\n[status: running]",
        )],
    );
    let Decision::Continue { message, .. } = decide(&mut end, answer("这是目前的结果。"))
    else {
        panic!("expected a challenge");
    };
    assert!(message.contains("job_output"), "{message}");
    assert!(!message.contains("wait_for_shell"), "{message}");
}

#[test]
fn bash_promoted_after_timeout_names_job_output() {
    let mut end = TurnEnd::new();
    end.record(
        &tools(),
        &[outcome(
            "bash",
            r#"{"command":"cargo check","run_in_background":false}"#,
            "partial\n[Command moved to the background after 10000 ms.]\n[job_id: job-2]\n[status: running]",
        )],
    );
    let Decision::Continue { message, .. } = decide(&mut end, answer("这是目前的结果。"))
    else {
        panic!("expected a challenge");
    };
    assert!(message.contains("job_output"), "{message}");
}

#[test]
fn foreground_output_that_mentions_running_can_finish() {
    let mut end = TurnEnd::new();
    end.record(
        &tools(),
        &[outcome(
            "bash",
            r#"{"command":"cargo check","run_in_background":false}"#,
            "note [status: running] in the log\n[exit code: 0]",
        )],
    );
    let decision = decide(&mut end, answer("这是目前的结果。"));
    assert!(
        matches!(decision, Decision::Finish { .. }),
        "a foreground command is not a pending job: {decision:?}"
    );
}

#[test]
fn timeout_promoted_dev_server_asks_to_share() {
    let mut end = TurnEnd::new();
    end.gate_mut().require_share_deliverable();
    end.record(
        &tools(),
        &[outcome(
            "bash",
            r#"{"command":"pnpm dev","run_in_background":false}"#,
            "listening\n[Command moved to the background after 10000 ms.]\n[job_id: job-3]\n[status: running]",
        )],
    );
    let Decision::Continue { message, .. } = decide(&mut end, answer("这是目前的结果。"))
    else {
        panic!("expected a challenge");
    };
    assert!(message.contains("share_preview_url"), "{message}");
    assert!(!message.contains("job_output"), "{message}");
}

#[test]
fn completed_job_output_lets_the_turn_finish() {
    let mut end = TurnEnd::new();
    let manager = tools();
    end.record(
        &manager,
        &[outcome(
            "bash",
            r#"{"command":"cargo check","run_in_background":true}"#,
            "[job_id: job-9]\n[status: running]",
        )],
    );
    end.record(
        &manager,
        &[outcome(
            "job_output",
            r#"{"job_id":"job-9"}"#,
            "ok\n[job_id: job-9]\n[status: completed]\n[exit code: 0]",
        )],
    );
    let decision = decide(&mut end, answer("这是目前的结果。"));
    assert!(
        matches!(decision, Decision::Finish { .. }),
        "finished job should not keep the turn open: {decision:?}"
    );
}

#[test]
fn pending_host_shell_names_wait_for_shell() {
    let mut end = TurnEnd::new();
    end.record(
        &tools(),
        &[outcome(
            "run_shell",
            r#"{"command":"cargo check","run_in_background":true}"#,
            "job-1",
        )],
    );
    let Decision::Continue { message, .. } = decide(&mut end, answer("这是目前的结果。"))
    else {
        panic!("expected a challenge");
    };
    assert!(message.contains("wait_for_shell"), "{message}");
}

#[test]
fn open_todos_name_todo_write() {
    let mut end = TurnEnd::new();
    end.record(
        &tools(),
        &[outcome(
            "todo_write",
            r#"{"todos":[{"content":"Fix login","status":"pending"}]}"#,
            "ok",
        )],
    );
    let Decision::Continue { message, .. } = decide(&mut end, answer("这是目前的结果。"))
    else {
        panic!("expected a challenge");
    };
    assert!(message.contains("todo_write"), "{message}");
    assert!(!message.contains("complete_plan_step"), "{message}");
}

#[test]
fn open_plan_tasks_name_complete_plan_step() {
    let mut end = TurnEnd::new();
    end.record(
        &tools(),
        &[outcome(
            "update_tasks",
            r#"{"tasks":[{"id":"a","content":"Ship","status":"pending"}]}"#,
            "ok",
        )],
    );
    let Decision::Continue { message, .. } = decide(&mut end, answer("这是目前的结果。"))
    else {
        panic!("expected a challenge");
    };
    assert!(message.contains("complete_plan_step"), "{message}");
    assert!(!message.contains("`todo_write`"), "{message}");
}

#[test]
fn both_open_lists_name_both_tools() {
    let mut end = TurnEnd::new();
    let manager = tools();
    end.record(
        &manager,
        &[
            outcome(
                "update_tasks",
                r#"{"tasks":[{"id":"a","content":"Ship","status":"pending"}]}"#,
                "ok",
            ),
            outcome(
                "todo_write",
                r#"{"todos":[{"content":"Fix login","status":"in_progress"}]}"#,
                "ok",
            ),
        ],
    );
    let Decision::Continue { message, .. } = decide(&mut end, answer("这是目前的结果。"))
    else {
        panic!("expected a challenge");
    };
    assert!(message.contains("complete_plan_step"), "{message}");
    assert!(message.contains("todo_write"), "{message}");
}
