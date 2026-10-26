use harness_queue::harness::mock::MockHarness;
use harness_queue::harness::{create_harness_adapter, QuotaStatus, SupportLevel};
use harness_queue::models::{
    AgentTarget, HarnessKind, ItemStatus, Mode, DEFAULT_CONTINUATION_PROMPT,
};
use harness_queue::supervisor::Supervisor;
use harness_queue::terminal::ghostty::classify_osascript_error;
use harness_queue::terminal::mock::MockTerminalAdapter;
use harness_queue::terminal::{AgentStatus, NeighborDetection, TerminalAdapter, TerminalError};
use std::sync::Arc;

// Scenario 1: Running hq inside herdr detects exactly one neighboring Codex session.
#[test]
fn scenario_01_herdr_detects_one_neighboring_codex() {
    let terminal = MockTerminalAdapter::new();
    let current = terminal.current_pane.clone();
    let res = terminal.find_adjacent_agent(&current).unwrap();

    match res {
        NeighborDetection::Exact(target) => {
            assert_eq!(target.harness, HarnessKind::Codex);
            assert_eq!(target.pane_id, "w1:p2");
            assert_eq!(target.session_id, "7f2a-mock");
        }
        _ => panic!("Expected Exact neighbor"),
    }
}

// Scenario 2: Auto-resume loads the correct default prompt without user input.
#[test]
fn scenario_02_auto_resume_loads_default_prompt() {
    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));

    let mut sup = Supervisor::new(terminal, harness, Mode::AutoResume);
    sup.initialize().unwrap();

    assert_eq!(sup.queue.len(), 1);
    let item = sup.queue.get(1).unwrap();
    assert_eq!(item.prompt, DEFAULT_CONTINUATION_PROMPT);
    assert_eq!(item.status, ItemStatus::Queued);
}

// Scenario 3: Auto-send accepts a custom prompt.
#[test]
fn scenario_03_auto_send_accepts_custom_prompt() {
    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));

    let mut sup = Supervisor::new(terminal, harness, Mode::AutoSend);
    sup.initialize().unwrap();

    assert_eq!(sup.queue.len(), 0);
    let id = sup.enqueue_prompt("Finish the Receiving workspace".to_string());
    assert_eq!(id, 1);
    assert_eq!(sup.queue.get(1).unwrap().prompt, "Finish the Receiving workspace");
}

// Scenario 4: Before quota recovery, prompts remain queued.
#[test]
fn scenario_04_before_quota_recovery_prompts_remain_queued() {
    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));
    harness.set_quota(QuotaStatus::RateLimited {
        resets_at: Some(1793050000),
        reason: "Quota limit".to_string(),
    });

    let mut sup = Supervisor::new(terminal, harness, Mode::AutoSend);
    sup.initialize().unwrap();
    sup.enqueue_prompt("Task 1".to_string());

    let events = sup.tick().unwrap();
    assert!(events.iter().any(|e| e.contains("Waiting for quota recovery")));
    assert_eq!(sup.queue.get(1).unwrap().status, ItemStatus::Queued);
}

// Scenario 5: A reset timestamp alone does not trigger a submission.
#[test]
fn scenario_05_reset_timestamp_alone_does_not_trigger_submission() {
    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));

    // Even if reset timestamp is in the past, if provider reports RateLimited, do not dispatch
    harness.set_quota(QuotaStatus::RateLimited {
        resets_at: Some(100), // Past timestamp
        reason: "Reset pending fresh server observation".to_string(),
    });

    let mut sup = Supervisor::new(terminal, harness, Mode::AutoSend);
    sup.initialize().unwrap();
    sup.enqueue_prompt("Task 1".to_string());

    let _ = sup.tick().unwrap();
    // Must remain queued
    assert_eq!(sup.queue.get(1).unwrap().status, ItemStatus::Queued);
}

// Scenario 6: Confirmed quota recovery enables dispatch when the original session is ready.
#[test]
fn scenario_06_confirmed_quota_recovery_enables_dispatch() {
    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));
    harness.set_quota(QuotaStatus::RateLimited {
        resets_at: Some(1000),
        reason: "Limit".to_string(),
    });

    let mut sup = Supervisor::new(terminal.clone(), harness.clone(), Mode::AutoSend);
    sup.initialize().unwrap();
    sup.enqueue_prompt("Task 1".to_string());
    let _ = sup.tick().unwrap();
    assert_eq!(sup.queue.get(1).unwrap().status, ItemStatus::Queued);

    // Quota confirmed available and session is ready
    harness.set_quota(QuotaStatus::Available);
    terminal.set_status(AgentStatus::Ready);

    let events = sup.tick().unwrap();
    assert!(events.iter().any(|e| e.contains("Usage reset detected")));
    assert!(events.iter().any(|e| e.contains("Prompt #1 sent!")));
    assert_eq!(sup.queue.get(1).unwrap().status, ItemStatus::Acknowledged);
}

// Scenario 7: HQ reports Prompt sent! after acknowledged submission.
#[test]
fn scenario_07_prompt_sent_after_acknowledged_submission() {
    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));

    let mut sup = Supervisor::new(terminal, harness, Mode::AutoSend);
    sup.initialize().unwrap();
    sup.enqueue_prompt("Task 1".to_string());

    let events = sup.tick().unwrap();
    assert!(events.iter().any(|e| e == "✓ Prompt #1 sent!"));
    assert_eq!(sup.queue.get(1).unwrap().status, ItemStatus::Acknowledged);
}

// Scenario 8: HQ immediately allows a second prompt.
#[test]
fn scenario_08_immediately_allows_second_prompt() {
    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));

    let mut sup = Supervisor::new(terminal, harness, Mode::AutoSend);
    sup.initialize().unwrap();
    sup.enqueue_prompt("Task 1".to_string());
    let _ = sup.tick().unwrap();

    // Immediately accept prompt #2
    let id2 = sup.enqueue_prompt("Task 2".to_string());
    assert_eq!(id2, 2);
    assert_eq!(sup.queue.get(2).unwrap().status, ItemStatus::Queued);
}

// Scenario 9: A second prompt waits while the agent is busy.
#[test]
fn scenario_09_second_prompt_waits_while_agent_is_busy() {
    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));

    let mut sup = Supervisor::new(terminal.clone(), harness, Mode::AutoSend);
    sup.initialize().unwrap();
    sup.enqueue_prompt("Task 1".to_string());
    let _ = sup.tick().unwrap(); // Prompt 1 sent, mock transitions to Busy

    sup.enqueue_prompt("Task 2".to_string());
    let events = sup.tick().unwrap();
    assert!(events.iter().any(|e| e.contains("Waiting for OpenAI Codex to finish")));
    assert_eq!(sup.queue.get(2).unwrap().status, ItemStatus::Queued);
}

// Scenario 10: A second prompt dispatches when the agent becomes idle.
#[test]
fn scenario_10_second_prompt_dispatches_when_idle() {
    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));

    let mut sup = Supervisor::new(terminal.clone(), harness, Mode::AutoSend);
    sup.initialize().unwrap();
    sup.enqueue_prompt("Task 1".to_string());
    let _ = sup.tick().unwrap();

    sup.enqueue_prompt("Task 2".to_string());
    let _ = sup.tick().unwrap(); // waits while busy

    // Agent finishes turn
    terminal.set_status(AgentStatus::Ready);
    let events = sup.tick().unwrap();
    assert!(events.iter().any(|e| e == "✓ Prompt #2 sent!"));
    assert_eq!(sup.queue.get(2).unwrap().status, ItemStatus::Acknowledged);
}

// Scenario 11: Additional prompts maintain FIFO ordering.
#[test]
fn scenario_11_fifo_ordering() {
    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));

    let mut sup = Supervisor::new(terminal.clone(), harness, Mode::AutoSend);
    sup.initialize().unwrap();

    sup.enqueue_prompt("First".to_string());
    sup.enqueue_prompt("Second".to_string());
    sup.enqueue_prompt("Third".to_string());

    // Prompt 1
    let ev1 = sup.tick().unwrap();
    assert!(ev1.iter().any(|e| e == "✓ Prompt #1 sent!"));
    terminal.set_status(AgentStatus::Ready);

    // Prompt 2
    let ev2 = sup.tick().unwrap();
    assert!(ev2.iter().any(|e| e == "✓ Prompt #2 sent!"));
    terminal.set_status(AgentStatus::Ready);

    // Prompt 3
    let ev3 = sup.tick().unwrap();
    assert!(ev3.iter().any(|e| e == "✓ Prompt #3 sent!"));
}

// Scenario 12: New rate-limit interruptions safely pause the queue.
#[test]
fn scenario_12_new_rate_limit_pauses_queue() {
    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));

    let mut sup = Supervisor::new(terminal.clone(), harness.clone(), Mode::AutoSend);
    sup.initialize().unwrap();

    sup.enqueue_prompt("Task 1".to_string());
    sup.enqueue_prompt("Task 2".to_string());

    let _ = sup.tick().unwrap(); // Prompt 1 sent
    terminal.set_status(AgentStatus::Ready);

    // Mid-session rate limit before prompt 2
    harness.set_quota(QuotaStatus::RateLimited {
        resets_at: Some(1793099999),
        reason: "429 Rate limited".to_string(),
    });

    let ev = sup.tick().unwrap();
    assert!(ev.iter().any(|e| e.contains("Waiting for quota recovery")));
    assert_eq!(sup.queue.get(2).unwrap().status, ItemStatus::Queued);
}

// Scenario 13: Duplicate lifecycle events never cause duplicate submission.
#[test]
fn scenario_13_no_duplicate_submission() {
    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));

    let mut sup = Supervisor::new(terminal.clone(), harness, Mode::AutoSend);
    sup.initialize().unwrap();

    sup.enqueue_prompt("Task 1".to_string());
    let _ = sup.tick().unwrap();

    // Repeated ready ticks without new queue items
    terminal.set_status(AgentStatus::Ready);
    let ev2 = sup.tick().unwrap();
    assert!(ev2.is_empty());
    assert_eq!(terminal.prompt_count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

// Scenario 14: An approval-blocked agent is not mistaken for rate-limited.
#[test]
fn scenario_14_approval_blocked_not_mistaken_for_rate_limited() {
    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));
    terminal.set_status(AgentStatus::BlockedApproval);

    let mut sup = Supervisor::new(terminal, harness, Mode::AutoSend);
    sup.initialize().unwrap();
    sup.enqueue_prompt("Task 1".to_string());

    let ev = sup.tick().unwrap();
    assert!(ev.iter().any(|e| e.contains("blocked on approval")));
    assert!(!sup.state_machine.is_rate_limited());
}

// Scenario 15: An ambiguous neighboring pane never receives input automatically.
#[test]
fn scenario_15_ambiguous_neighboring_pane_never_receives_input() {
    let terminal = MockTerminalAdapter::new();
    // Simulate ambiguous neighbor detection
    let target1 = AgentTarget {
        terminal_id: "t1".to_string(),
        pane_id: "w1:p2".to_string(),
        pid: 100,
        harness: HarnessKind::Codex,
        session_id: "s1".to_string(),
        cwd: "/tmp".to_string(),
    };
    let target2 = AgentTarget {
        terminal_id: "t2".to_string(),
        pane_id: "w1:p3".to_string(),
        pid: 101,
        harness: HarnessKind::ClaudeCode,
        session_id: "s2".to_string(),
        cwd: "/tmp".to_string(),
    };

    struct AmbiguousTerminal(MockTerminalAdapter, Vec<AgentTarget>);
    impl harness_queue::terminal::TerminalAdapter for AmbiguousTerminal {
        fn terminal_name(&self) -> &'static str { "Ambiguous" }
        fn is_available(&self) -> bool { true }
        fn detect_current_pane(&self) -> Result<String, TerminalError> { Ok("w1:p1".to_string()) }
        fn find_adjacent_agent(&self, _pane: &str) -> Result<NeighborDetection, TerminalError> {
            Ok(NeighborDetection::Ambiguous(self.1.clone()))
        }
        fn get_agent_status(&self, pane: &str) -> Result<AgentStatus, TerminalError> { self.0.get_agent_status(pane) }
        fn send_prompt(&self, target: &AgentTarget, prompt: &str) -> Result<harness_queue::terminal::DeliveryAck, TerminalError> {
            self.0.send_prompt(target, prompt)
        }
        fn verify_target_alive(&self, target: &AgentTarget) -> Result<bool, TerminalError> {
            self.0.verify_target_alive(target)
        }
    }

    let amb = Arc::new(AmbiguousTerminal(terminal, vec![target1, target2]));
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));

    let mut sup = Supervisor::new(amb, harness, Mode::AutoSend);
    let init_res = sup.initialize();
    assert!(init_res.is_err());
    assert!(init_res.unwrap_err().contains("Ambiguous"));
}

// Scenario 16: Ctrl+C exits cleanly without stopping the agent.
#[test]
fn scenario_16_clean_detachment() {
    let mut sm = harness_queue::state_machine::StateMachine::new();
    sm.transition(harness_queue::models::State::SelectingMode).unwrap();
    sm.transition(harness_queue::models::State::Watching).unwrap();

    // Detach cleanly
    sm.transition(harness_queue::models::State::Detached).unwrap();
    assert_eq!(sm.current(), harness_queue::models::State::Detached);
}

// Scenario 17: Unexpected HQ restart does not blindly resend uncertain submissions.
#[test]
fn scenario_17_uncertain_submission_not_blindly_resent() {
    let mut q = harness_queue::queue::QueueEngine::new();
    let id = q.enqueue("Prompt 1".to_string());
    q.mark_submitting(id, 1000).unwrap();
    q.mark_uncertain(id).unwrap();

    // An uncertain item cannot be peeked as next pending queued item
    assert!(q.peek_next_pending().is_none());
}

// Scenario 18: Herdr pane changes or process replacement cannot silently redirect prompts.
#[test]
fn scenario_18_process_replacement_halts_submission() {
    let terminal = Arc::new(MockTerminalAdapter::new());
    let harness = Arc::new(MockHarness::new(HarnessKind::Codex));

    let mut sup = Supervisor::new(terminal.clone(), harness, Mode::AutoSend);
    sup.initialize().unwrap();
    sup.enqueue_prompt("Prompt 1".to_string());

    // Replace target PID in the pane (e.g. user ran another process)
    if let Some(t) = &mut sup.target {
        t.pid = 99999; // Mismatched PID
    }

    let tick_res = sup.tick();
    assert!(tick_res.is_err());
    assert!(tick_res.unwrap_err().contains("process changed or died"));
}

// Scenario 19: Ghostty missing Automation permissions produces a clear error.
#[test]
fn scenario_19_ghostty_missing_automation_permissions_clear_error() {
    let err_msg = "Ghostty got an error: Not authorized to send Apple events to Ghostty. (-1743)";
    let classified = classify_osascript_error(err_msg);
    match classified {
        TerminalError::PermissionDenied(msg) => {
            assert!(msg.contains("Missing macOS Automation permission for Ghostty"));
            assert!(msg.contains("System Settings -> Privacy & Security -> Automation"));
        }
        _ => panic!("Expected PermissionDenied"),
    }
}

// Scenario 20: Unsupported harness capabilities are reported truthfully.
#[test]
fn scenario_20_unsupported_harness_capabilities_reported_truthfully() {
    let agy = create_harness_adapter(&HarnessKind::Agy);
    let caps = agy.capabilities();
    assert_eq!(caps.level, SupportLevel::Partial);
    assert!(!caps.can_check_quota);

    let codex = create_harness_adapter(&HarnessKind::Codex);
    let codex_caps = codex.capabilities();
    assert_eq!(codex_caps.level, SupportLevel::Full);
    assert!(codex_caps.can_check_quota);
}
