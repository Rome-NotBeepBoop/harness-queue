use harness_queue::models::State;
use harness_queue::state_machine::{StateMachine, TransitionError};

#[test]
fn test_valid_lifecycle_transitions() {
    let mut sm = StateMachine::new();
    assert_eq!(sm.current(), State::Detecting);

    sm.transition(State::SelectingMode)
        .expect("to SelectingMode");
    sm.transition(State::Watching).expect("to Watching");
    sm.transition(State::AgentReady).expect("to AgentReady");

    assert!(sm.can_dispatch());

    sm.transition(State::Submitting).expect("to Submitting");
    assert!(!sm.can_dispatch());

    sm.transition(State::PromptAcknowledged)
        .expect("to PromptAcknowledged");
    sm.transition(State::AgentBusy).expect("to AgentBusy");
    assert!(!sm.can_dispatch());

    sm.transition(State::AgentReady).expect("to AgentReady");
    assert!(sm.can_dispatch());
}

#[test]
fn test_rate_limited_recovery_transitions() {
    let mut sm = StateMachine::new();
    sm.transition(State::SelectingMode).unwrap();
    sm.transition(State::Watching).unwrap();

    // Interruption detected
    sm.transition(State::RateLimited).unwrap();
    assert!(!sm.can_dispatch());

    sm.transition(State::WaitingForRecovery).unwrap();
    assert!(!sm.can_dispatch());

    sm.transition(State::RecoveringSession).unwrap();
    assert!(!sm.can_dispatch());

    sm.transition(State::AgentReady).unwrap();
    assert!(sm.can_dispatch());
}

#[test]
fn test_held_state_transitions_and_cannot_dispatch() {
    let mut sm = StateMachine::new();
    sm.transition(State::SelectingMode).unwrap();
    sm.transition(State::Watching).unwrap();

    // From Watching into Held
    sm.transition(State::Held).unwrap();
    assert_eq!(sm.current(), State::Held);
    assert!(!sm.can_dispatch(), "Held state must never dispatch prompts");

    // From Held to RateLimited once remaining quota reaches 0%
    sm.transition(State::RateLimited).unwrap();
    assert_eq!(sm.current(), State::RateLimited);

    // Can also transition from AgentReady to Held
    let mut sm2 = StateMachine::new();
    sm2.transition(State::SelectingMode).unwrap();
    sm2.transition(State::Watching).unwrap();
    sm2.transition(State::AgentReady).unwrap();
    sm2.transition(State::Held).unwrap();
    assert_eq!(sm2.current(), State::Held);
    assert!(!sm2.can_dispatch());
}

#[test]
fn test_invalid_transitions_rejected() {
    let mut sm = StateMachine::new();
    // Cannot jump from Detecting directly to Submitting
    assert!(matches!(
        sm.transition(State::Submitting),
        Err(TransitionError::InvalidTransition { .. })
    ));
}
