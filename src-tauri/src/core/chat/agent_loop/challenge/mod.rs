//! Honest-completion enforcement.
//!
//! A task-like final answer that claims completion without any successful
//! modifying tool (or without a verifying check) is challenged and sent back
//! so the model either actually executes the work or explicitly admits
//! nothing was changed — claiming done is not accepted at face value.
//!
//! [`CompletionGate`] tracks whether this turn produced verified work and how
//! many times each completion challenge has already been issued.
//! [`super::turn_end::TurnEnd`] is what the loop asks when the model calls no tools.

mod gate;
mod paths;
mod text;

pub use gate::CompletionGate;
pub(crate) use paths::{extract_paths_from_args, push_challenge_message};
pub use text::{
    challenges_enabled, set_challenges_enabled, ChallengeOutcome, COMPLETION_CHALLENGE,
    GOAL_COVERAGE_CHALLENGE, IMAGE_REQUIRED_CHALLENGE, LENGTH_EXCEEDED_CHALLENGE,
    MAX_COMPLETION_RETRIES, MAX_NO_PROGRESS_REPEATS, OPEN_TASKS_CHALLENGE,
    SHARE_DELIVERABLE_CHALLENGE, STALL_CHALLENGE, VERIFICATION_CHALLENGE,
};
