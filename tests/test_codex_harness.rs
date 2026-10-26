use harness_queue::harness::codex::parse_rate_limits_payload;
use harness_queue::harness::mock::MockHarness;
use harness_queue::harness::{AgentReadiness, HarnessAdapter, QuotaStatus, SupportLevel};
use harness_queue::models::{AgentTarget, HarnessKind};
use harness_queue::terminal::mock::MockTerminalAdapter;
use harness_queue::terminal::AgentStatus;

#[test]
fn test_parse_codex_rate_limits_normal() {
    let payload = r#"{
        "ordinaryUsageAllowed": true,
        "rateLimits": {
            "limitId": "codex",
            "primary": {
                "usedPercent": 25,
                "windowDurationMins": 300,
                "resetsAt": 1793058717
            },
            "rateLimitReachedType": null,
            "spendControlReached": false
        }
    }"#;

    let status = parse_rate_limits_payload(payload).expect("parsed quota");
    assert_eq!(status, QuotaStatus::Available);
}

#[test]
fn test_parse_codex_rate_limits_exhausted() {
    let payload = r#"{
        "ordinaryUsageAllowed": false,
        "rateLimits": {
            "limitId": "codex",
            "primary": {
                "usedPercent": 100,
                "windowDurationMins": 300,
                "resetsAt": 1793059999
            },
            "rateLimitReachedType": "usage_limit_reached",
            "spendControlReached": false
        }
    }"#;

    let status = parse_rate_limits_payload(payload).expect("parsed quota");
    match status {
        QuotaStatus::RateLimited { resets_at, reason } => {
            assert_eq!(resets_at, Some(1793059999));
            assert!(reason.contains("usage_limit_reached") || reason.contains("100%"));
        }
        other => panic!("Expected RateLimited, got {:?}", other),
    }
}

#[test]
fn test_approval_blocked_is_not_rate_limited() {
    let harness = MockHarness::new(HarnessKind::Codex);
    let terminal = MockTerminalAdapter::new();
    terminal.set_status(AgentStatus::BlockedApproval);

    let target = AgentTarget {
        terminal_id: "term1".to_string(),
        pane_id: "w1:p2".to_string(),
        pid: 1234,
        harness: HarnessKind::Codex,
        session_id: "sess_1".to_string(),
        cwd: "/tmp".to_string(),
    };

    let readiness = harness.is_ready(&target, &terminal).expect("readiness check");
    assert!(matches!(readiness, AgentReadiness::BlockedOnApproval(_)));
    // Quota remains available, blocked approval does NOT trigger rate limit
    let quota = harness.check_quota("sess_1").expect("quota check");
    assert_eq!(quota, QuotaStatus::Available);
}

#[test]
fn test_codex_capabilities() {
    let harness = MockHarness::new(HarnessKind::Codex);
    let caps = harness.capabilities();
    assert_eq!(caps.level, SupportLevel::Full);
    assert!(caps.can_track_session);
    assert!(caps.can_check_quota);
    assert!(caps.can_auto_resume);
}

#[test]
fn test_codex_rate_limits_missing_response_fails_safe() {
    let unparsed = r#"{"id":999,"result":{}}"#;
    let res = parse_rate_limits_payload(unparsed);
    // If rateLimits payload is completely missing, should report Unknown rather than assuming Available
    match res {
        Ok(QuotaStatus::Unknown) | Err(_) => {},
        Ok(QuotaStatus::Available) => panic!("Should not blindly report Available on missing rateLimits payload"),
        _ => {},
    }
}
