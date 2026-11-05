use std::sync::atomic::{AtomicBool, Ordering};

/// How many times an unverified completion claim is challenged and sent back
/// before the final answer is replaced with an explicit unverified result.
pub const MAX_COMPLETION_RETRIES: u32 = 1;

/// Successful identical tool+args repeats that count as a no-progress loop.
pub const MAX_NO_PROGRESS_REPEATS: u32 = 3;

/// Injected after the model's final answer claims completion without any
/// successful modifying tool. The model must either actually execute the
/// work or explicitly admit nothing was changed — claiming done is not
/// accepted.
pub const COMPLETION_CHALLENGE: &str = concat!(
    "[System] Completion claim rejected: no modifying tool succeeded this turn. ",
    "Do not restate prior reasoning. Either call the required tools now, ",
    "or clearly say what was not changed and what is blocking.",
);

pub const VERIFICATION_CHALLENGE: &str = concat!(
    "[System] Changes ran but were not verified. ",
    "Do not restate prior reasoning. Read back the modified files (or inspect ",
    "the checkpoint/diff), or run a test/build check, then report the verified ",
    "result or the failure.",
);

pub const GOAL_COVERAGE_CHALLENGE: &str = concat!(
    "[System] Completion claim rejected: mentioned target paths were not ",
    "touched by a successful modifying tool this turn. Edit those paths or ",
    "explicitly state they are out of scope / blocked.",
);

pub const STALL_CHALLENGE: &str = concat!(
    "[System] Tool loop stall detected: the same tool+arguments succeeded ",
    "repeatedly without progress. Do not repeat that call. Change strategy ",
    "(different tool, different arguments, or read back evidence) or stop ",
    "and report the blocker.",
);

pub const SHARE_DELIVERABLE_CHALLENGE: &str = concat!(
    "[System] This turn edited files or started a local web server, but you have ",
    "not shared the deliverable. Before finishing, call `share_to_companion` for ",
    "the files the user should open, and/or `share_preview_url` for a local ",
    "http://127.0.0.1:... preview. Do not wrap files in HTML. Then answer.",
);

pub const IMAGE_REQUIRED_CHALLENGE: &str = concat!(
    "[System] No image was generated this turn. You described a picture that does not exist. ",
    "Call `generate_image` now with a complete prompt. Do not write a caption, ",
    "say the image is ready, or offer a follow-up edit until the tool returns markdown.",
);

pub const OPEN_TASKS_CHALLENGE: &str = concat!(
    "[System] Completion rejected: plan/todo items are still open. ",
    "Mark each finished step with `complete_plan_step` and evidence ",
    "(path or check command), or cancel out-of-scope steps explicitly, then answer.",
);

pub const LENGTH_EXCEEDED_CHALLENGE: &str = concat!(
    "[System] The previous turn hit the output token limit with no tool calls and no answer. ",
    "Do not continue or restate that reasoning. Call tools now. ",
    "For large HTML/JSON/logs/flamegraphs, do not read_file or paginate the whole file — ",
    "write a short run_shell script that extracts/aggregates the needed facts and prints ",
    "a compact summary. For source, Grep then read_file ",
    "(around_line for a hit, or the key file from the start).",
);

/// Global ablation switch used by the eval harness (`--no-challenge`).
static CHALLENGES_DISABLED: AtomicBool = AtomicBool::new(false);

pub fn set_challenges_enabled(enabled: bool) {
    CHALLENGES_DISABLED.store(!enabled, Ordering::Relaxed);
}

pub fn challenges_enabled() -> bool {
    !CHALLENGES_DISABLED.load(Ordering::Relaxed)
}

/// What the loop should do after evaluating a tool-call-free final answer.
#[derive(Debug)]
pub enum ChallengeOutcome {
    /// A challenge was appended to `request` as a user message; the loop
    /// should emit the given status and run another provider turn.
    ContinueWithChallenge { status_kind: String },
    /// No further challenge applies; this is the turn's actual final answer.
    Finish {
        content: String,
        reasoning: Option<String>,
        finish_reason: Option<String>,
    },
}
