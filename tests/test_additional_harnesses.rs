use harness_queue::harness::create_harness_adapter;
use harness_queue::harness::{AgentReadiness, QuotaStatus, SupportLevel};
use harness_queue::models::{AgentTarget, HarnessKind};
use harness_queue::terminal::mock::MockTerminalAdapter;

#[test]
fn test_all_harness_kinds_can_be_created() {
    let kinds = [
        HarnessKind::Codex,
        HarnessKind::ClaudeCode,
        HarnessKind::OpenCode,
        HarnessKind::Omp,
        HarnessKind::Pi,
        HarnessKind::Agy,
    ];

    for kind in &kinds {
        let adapter = create_harness_adapter(kind);
        assert_eq!(&adapter.kind(), kind);
        let caps = adapter.capabilities();
        assert!(caps.can_track_session);
        // Truthful reporting: only Codex currently has full structured app-server quota integration
        if *kind == HarnessKind::Codex {
            assert_eq!(caps.level, SupportLevel::Full);
            assert!(caps.can_check_quota);
        } else {
            assert_eq!(caps.level, SupportLevel::Partial);
            assert!(!caps.can_check_quota);
        }
    }
}

#[test]
fn test_claude_harness_submission_via_terminal() {
    let adapter = create_harness_adapter(&HarnessKind::ClaudeCode);
    let terminal = MockTerminalAdapter::new();
    let target = AgentTarget {
        terminal_id: "term_claude".to_string(),
        pane_id: "w1:p2".to_string(),
        pid: 3001,
        harness: HarnessKind::ClaudeCode,
        session_id: "claude-sess-1".to_string(),
        cwd: "/tmp".to_string(),
    };

    let readiness = adapter.is_ready(&target, &terminal).expect("readiness");
    assert_eq!(readiness, AgentReadiness::Ready);

    let ack = adapter
        .submit_prompt(&target, "Fix the bug", &terminal)
        .expect("submitted");
    assert!(ack.acknowledged);

    // Quota returns Available or Unknown for partial adapters
    let quota = adapter.check_quota("claude-sess-1").expect("quota");
    assert_eq!(quota, QuotaStatus::Available);
}

#[test]
fn test_all_harnesses_report_active_remaining_quota_when_available() {
    let kinds = [
        HarnessKind::Agy,
        HarnessKind::ClaudeCode,
        HarnessKind::Codex,
        HarnessKind::Pi,
        HarnessKind::OpenCode,
        HarnessKind::Omp,
    ];

    for kind in &kinds {
        let adapter = create_harness_adapter(kind);
        let target = AgentTarget {
            terminal_id: "term_test".to_string(),
            pane_id: "w1:p2".to_string(),
            pid: 4000,
            harness: kind.clone(),
            session_id: "test_sess".to_string(),
            cwd: "/tmp".to_string(),
        };
        let terminal = MockTerminalAdapter::with_target(target.clone());
        terminal.set_status(harness_queue::terminal::AgentStatus::Ready);

        assert!(
            adapter.has_active_remaining_quota(&target, &terminal),
            "Harness {:?} must report has_active_remaining_quota == true when quota is available",
            kind
        );
    }
}

#[test]
fn test_all_harnesses_report_no_active_quota_when_rate_limited() {
    let kinds = [
        HarnessKind::Agy,
        HarnessKind::ClaudeCode,
        HarnessKind::Codex,
        HarnessKind::Pi,
        HarnessKind::OpenCode,
        HarnessKind::Omp,
    ];

    for kind in &kinds {
        let adapter = create_harness_adapter(kind);
        let target = AgentTarget {
            terminal_id: "term_test".to_string(),
            pane_id: "w1:p2".to_string(),
            pid: 4000,
            harness: kind.clone(),
            session_id: "test_sess".to_string(),
            cwd: "/tmp".to_string(),
        };
        let terminal = MockTerminalAdapter::with_target(target.clone());
        terminal.set_status(harness_queue::terminal::AgentStatus::RateLimited);

        assert!(
            !adapter.has_active_remaining_quota(&target, &terminal),
            "Harness {:?} must report has_active_remaining_quota == false when RateLimited",
            kind
        );
    }
}

#[test]
fn test_agy_harness_supervisor_holds_prompt_until_zero_percent_and_recovery() {
    use harness_queue::harness::agy::AgyHarness;
    use harness_queue::models::{ItemStatus, Mode, State};
    use harness_queue::supervisor::Supervisor;
    use std::sync::Arc;

    let target = AgentTarget {
        terminal_id: "term_agy".to_string(),
        pane_id: "w1:p2".to_string(),
        pid: 5000,
        harness: HarnessKind::Agy,
        session_id: "agy_sess".to_string(),
        cwd: "/tmp".to_string(),
    };
    let terminal = Arc::new(MockTerminalAdapter::with_target(target));
    terminal.set_status(harness_queue::terminal::AgentStatus::Ready);
    let agy = Arc::new(AgyHarness);

    let mut sup = Supervisor::new(terminal.clone(), agy, Mode::AutoSend);
    sup.initialize().expect("initialized");
    assert_eq!(sup.target.as_ref().unwrap().harness, HarnessKind::Agy);

    // Enqueue prompt while Antigravity has quota available
    let id = sup.enqueue_prompt("testing testing".to_string());
    assert_eq!(id, 1);

    // Supervisor tick: Antigravity has remaining quota -> must HOLD in State::Held, NOT send!
    let events = sup.tick().expect("tick held");
    assert!(
        events
            .iter()
            .any(|e| e.contains("Agent has remaining quota. Queue is HELD")),
        "Events must include held notification, got: {:?}",
        events
    );
    assert_eq!(sup.state_machine.current(), State::Held);
    assert_eq!(sup.queue.get(1).unwrap().status, ItemStatus::Held);
    assert_eq!(
        terminal
            .prompt_count
            .load(std::sync::atomic::Ordering::SeqCst),
        0,
        "Prompt must NOT be sent to Antigravity while quota is active"
    );

    // Subsequent tick: remains held, no duplicate log spammed
    let events2 = sup.tick().expect("tick 2");
    assert!(events2.is_empty());
    assert_eq!(sup.state_machine.current(), State::Held);
    assert_eq!(
        terminal
            .prompt_count
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}
