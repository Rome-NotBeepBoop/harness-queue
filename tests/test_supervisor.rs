use harness_queue::harness::mock::MockHarness;
use harness_queue::harness::QuotaStatus;
use harness_queue::models::{HarnessKind, ItemStatus, Mode, DEFAULT_CONTINUATION_PROMPT};
use harness_queue::supervisor::Supervisor;
use harness_queue::terminal::mock::MockTerminalAdapter;
use harness_queue::terminal::AgentStatus;
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
    assert!(events2.iter().any(|e| e.contains("Waiting") || e.contains("busy")));
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
    assert!(events.iter().any(|e| e.contains("Waiting for quota recovery")));
    assert_eq!(sup.queue.get(1).unwrap().status, ItemStatus::Queued);

    // Recover quota
    harness.set_quota(QuotaStatus::Available);

    // Tick recovers and dispatches prompt
    let events2 = sup.tick().expect("tick recovered");
    assert!(events2.iter().any(|e| e.contains("Prompt #1 sent!")));
    assert_eq!(sup.queue.get(1).unwrap().status, ItemStatus::Acknowledged);
}
