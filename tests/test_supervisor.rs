use harness_queue::harness::mock::MockHarness;
use harness_queue::harness::QuotaStatus;
use harness_queue::models::{
    AgentTarget, HarnessKind, ItemStatus, Mode, State, DEFAULT_CONTINUATION_PROMPT,
};
use harness_queue::supervisor::Supervisor;
use harness_queue::terminal::mock::MockTerminalAdapter;
use harness_queue::terminal::{AgentStatus, NeighborDetection, TerminalError};
use std::sync::Arc;

#[test]
fn test_supervisor_auto_resume_flow() {
    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));

    let mut sup = Supervisor::new(terminal.clone(), harness.clone(), Mode::AutoResume);
    sup.initialize().expect("initialized");

    // Mode 1 enqueues the default prompt automatically
    assert_eq!(sup.queue.len(), 1);
    let item = sup.queue.get(1).expect("item 1");
    assert_eq!(item.prompt, DEFAULT_CONTINUATION_PROMPT);

    // Initial tick with ready agent dispatches prompt #1
    let events = sup.tick().expect("tick 1");
    assert!(events.iter().any(|e| e.contains("Prompt #1 sent!")));

    assert_eq!(sup.queue.get(1).unwrap().status, ItemStatus::Acknowledged);
    assert_eq!(*terminal.status.lock().unwrap(), AgentStatus::Busy);

    // Prompt #2 queued while agent is busy
    let id2 = sup.enqueue_prompt("Next task".to_string());
    assert_eq!(id2, 2);

    // Next tick: agent is busy, prompt remains queued
    let events2 = sup.tick().expect("tick 2");
    assert!(events2
        .iter()
        .any(|e| e.contains("Waiting") || e.contains("busy")));
    assert_eq!(sup.queue.get(2).unwrap().status, ItemStatus::Queued);

    // Agent finishes prompt #1
    terminal.set_status(AgentStatus::Ready);

    // Next tick: dispatches prompt #2
    let events3 = sup.tick().expect("tick 3");
    assert!(events3.iter().any(|e| e.contains("Prompt #2 sent!")));
    assert_eq!(sup.queue.get(2).unwrap().status, ItemStatus::Acknowledged);
}

#[test]
fn test_supervisor_rate_limit_and_recovery() {
    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));

    // Simulate rate limit
    harness.set_quota(QuotaStatus::RateLimited {
        resets_at: Some(1793050000),
        reason: "429 Too Many Requests".to_string(),
    });

    let mut sup = Supervisor::new(terminal.clone(), harness.clone(), Mode::AutoSend);
    sup.initialize().expect("initialized");
    sup.enqueue_prompt("Custom prompt".to_string());

    // Tick while rate-limited pauses dispatch
    let events = sup.tick().expect("tick rate-limited");
    assert!(events
        .iter()
        .any(|e| e.contains("Waiting for quota recovery")));
    assert_eq!(sup.queue.get(1).unwrap().status, ItemStatus::Queued);

    // Recover quota
    harness.set_quota(QuotaStatus::Available);

    // Tick recovers and dispatches prompt
    let events2 = sup.tick().expect("tick recovered");
    assert!(events2.iter().any(|e| e.contains("Prompt #1 sent!")));
    assert_eq!(sup.queue.get(1).unwrap().status, ItemStatus::Acknowledged);
}

#[test]
fn test_supervisor_active_quota_held_until_zero_percent_and_recovery() {
    use harness_queue::models::State;

    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));

    // Agent has active remaining quota (e.g. 10% left in 5h window)
    harness.set_quota(QuotaStatus::Available);
    harness.set_active_remaining_quota(true);

    let mut sup = Supervisor::new(terminal.clone(), harness.clone(), Mode::AutoSend);
    sup.initialize().expect("initialized");
    let id = sup.enqueue_prompt("Overnight prompt".to_string());
    assert_eq!(id, 1);

    // Initial tick: agent has active remaining quota -> supervisor must HOLD, not dispatch!
    let events = sup.tick().expect("tick held");
    assert!(events
        .iter()
        .any(|e| e.contains("Agent has remaining quota. Queue is HELD")));
    assert_eq!(sup.state_machine.current(), State::Held);
    assert_eq!(sup.queue.get(1).unwrap().status, ItemStatus::Held);
    assert_eq!(*terminal.status.lock().unwrap(), AgentStatus::Ready);
    assert_eq!(
        terminal
            .prompt_count
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );

    // Subsequent tick while quota is still active: still held, no duplicate log
    let events_held2 = sup.tick().expect("tick held 2");
    assert!(events_held2.is_empty());
    assert_eq!(sup.state_machine.current(), State::Held);

    // User exhausts remaining quota in terminal -> remaining hits 0% (rate limit reached)
    harness.set_active_remaining_quota(false);
    harness.set_quota(QuotaStatus::RateLimited {
        resets_at: Some(1793055000),
        reason: "Rate limit reached".to_string(),
    });

    let events_limit = sup.tick().expect("tick limit");
    assert!(events_limit
        .iter()
        .any(|e| e.contains("Waiting for quota recovery")));
    assert_eq!(sup.state_machine.current(), State::WaitingForRecovery);
    assert_eq!(sup.queue.get(1).unwrap().status, ItemStatus::Queued);

    // Recovery time arrives -> quota resets to fresh 100%, and prompt is dispatched!
    harness.set_quota(QuotaStatus::Available);
    let events_recover = sup.tick().expect("tick recover");
    assert!(events_recover
        .iter()
        .any(|e| e.contains("Usage reset detected")));
    assert!(events_recover.iter().any(|e| e.contains("Prompt #1 sent!")));
    assert_eq!(sup.queue.get(1).unwrap().status, ItemStatus::Acknowledged);
    assert_eq!(
        terminal
            .prompt_count
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
}

#[test]
fn test_uncertain_delivery_halts_subsequent_dispatches() {
    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));

    let mut sup = Supervisor::new(terminal.clone(), harness, Mode::AutoSend);
    sup.initialize().unwrap();

    sup.enqueue_prompt("Task 1".to_string());
    sup.enqueue_prompt("Task 2".to_string());

    // Force uncertain delivery
    sup.queue.mark_submitting(1, 1000).unwrap();
    sup.queue.mark_uncertain(1).unwrap();
    sup.state_machine
        .transition(harness_queue::models::State::DeliveryUncertain)
        .unwrap();

    terminal.set_status(AgentStatus::Ready);
    let events = sup.tick().unwrap();

    // Must NOT dispatch task 2 while task 1 is uncertain
    assert!(!events.iter().any(|e| e.contains("Prompt #2 sent!")));
    assert_eq!(sup.queue.get(2).unwrap().status, ItemStatus::Queued);
}

#[test]
fn test_target_death_halts_supervisor() {
    struct DeadTerminal;
    impl harness_queue::terminal::TerminalAdapter for DeadTerminal {
        fn terminal_name(&self) -> &'static str {
            "Herdr"
        }
        fn is_available(&self) -> bool {
            true
        }
        fn detect_current_pane(&self) -> Result<String, TerminalError> {
            Ok("w1:p1".to_string())
        }
        fn find_adjacent_agent(&self, _pane: &str) -> Result<NeighborDetection, TerminalError> {
            Ok(NeighborDetection::Exact(AgentTarget {
                terminal_id: "term_2".to_string(),
                pane_id: "w1:p2".to_string(),
                pid: 1234,
                harness: HarnessKind::Codex,
                session_id: "sess_1".to_string(),
                cwd: "/tmp".to_string(),
            }))
        }
        fn get_agent_status(&self, _pane: &str) -> Result<AgentStatus, TerminalError> {
            Ok(AgentStatus::Ready)
        }
        fn send_prompt(
            &self,
            _target: &AgentTarget,
            _prompt: &str,
        ) -> Result<harness_queue::terminal::DeliveryAck, TerminalError> {
            Ok(harness_queue::terminal::DeliveryAck {
                acknowledged: true,
                details: "ok".to_string(),
            })
        }
        fn verify_target_alive(&self, _target: &AgentTarget) -> Result<bool, TerminalError> {
            // Target terminal closed/dead
            Ok(false)
        }
    }

    let terminal = Arc::new(DeadTerminal);
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));

    let mut sup = Supervisor::new(terminal, harness, Mode::AutoSend);
    sup.initialize().unwrap();
    sup.enqueue_prompt("Task 1".to_string());

    let res = sup.tick();
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("changed or died"));
}

#[test]
fn test_supervisor_force_send_held_prompts() {
    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));

    // Agent has active remaining quota
    harness.set_active_remaining_quota(true);

    let mut sup = Supervisor::new(terminal.clone(), harness.clone(), Mode::AutoSend);
    sup.initialize().unwrap();

    let id = sup.enqueue_prompt("Run task despite remaining quota".to_string());
    assert_eq!(id, 1);

    // First tick: remaining quota holds the prompt
    let events = sup.tick().expect("tick with active remaining quota");
    assert!(events
        .iter()
        .any(|e| e.contains("Agent has remaining quota. Queue is HELD")));
    assert_eq!(sup.state_machine.current(), State::Held);
    assert_eq!(sup.queue.get(1).unwrap().status, ItemStatus::Held);
    assert_eq!(
        terminal
            .prompt_count
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );

    // User triggers force-send
    let force_events = sup.force_send_held();
    assert!(force_events
        .iter()
        .any(|e| e.contains("Force-sending HELD prompts")));
    assert_eq!(sup.state_machine.current(), State::AgentReady);
    assert_eq!(sup.queue.get(1).unwrap().status, ItemStatus::Queued);
    assert!(sup.force_send_held);

    // Next tick: dispatches the prompt immediately even though remaining quota was active!
    let dispatch_events = sup.tick().expect("tick after force send");
    assert!(dispatch_events.iter().any(|e| e.contains("Prompt #1 sent!")));
    assert_eq!(sup.queue.get(1).unwrap().status, ItemStatus::Acknowledged);
    assert_eq!(
        terminal
            .prompt_count
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    // Queue is empty now, so force_send_held flag resets
    assert!(!sup.force_send_held);
}
