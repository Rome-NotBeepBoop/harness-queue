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

    let ack = adapter.submit_prompt(&target, "Fix the bug", &terminal).expect("submitted");
    assert!(ack.acknowledged);

    // Quota returns Available or Unknown for partial adapters
    let quota = adapter.check_quota("claude-sess-1").expect("quota");
    assert_eq!(quota, QuotaStatus::Available);
}
