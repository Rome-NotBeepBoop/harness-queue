use harness_queue::models::{AgentTarget, HarnessKind};
use harness_queue::terminal::herdr::parse_neighbor_result;
use harness_queue::terminal::mock::MockTerminalAdapter;
use harness_queue::terminal::{AgentStatus, NeighborDetection, TerminalAdapter};

#[test]
fn test_mock_terminal_adapter_lifecycle() {
    let adapter = MockTerminalAdapter::new();
    assert!(adapter.is_available());
    assert_eq!(adapter.terminal_name(), "Mock");

    let current = adapter.detect_current_pane().expect("current pane");
    assert_eq!(current, "w1:p1");

    match adapter.find_adjacent_agent(&current).expect("find agent") {
        NeighborDetection::Exact(target) => {
            assert_eq!(target.pane_id, "w1:p2");
            assert_eq!(target.harness, HarnessKind::Codex);
            assert_eq!(target.session_id, "7f2a-mock");

            let status = adapter.get_agent_status(&target.pane_id).expect("status");
            assert_eq!(status, AgentStatus::Ready);

            let ack = adapter
                .send_prompt(&target, "Run tests")
                .expect("send prompt");
            assert!(ack.acknowledged);

            // Once prompt is sent, mock transitions to Busy
            let status_after = adapter
                .get_agent_status(&target.pane_id)
                .expect("status after");
            assert_eq!(status_after, AgentStatus::Busy);

            // Target verification
            assert!(adapter.verify_target_alive(&target).expect("alive check"));

            // If PID differs, alive check fails
            let fake_target = AgentTarget {
                pid: 99999,
                ..target
            };
            assert!(!adapter
                .verify_target_alive(&fake_target)
                .expect("mismatch check"));
        }
        other => panic!("Expected Exact neighbor, got {:?}", other),
    }
}

#[test]
fn test_parse_herdr_neighbor_json() {
    let json_str = r#"{
        "id": "cli:pane:neighbor",
        "result": {
            "neighbor": {
                "direction": "right",
                "neighbor_pane_id": "wE:pH",
                "pane_id": "wE:pF"
            }
        }
    }"#;

    let neighbor_id = parse_neighbor_result(json_str).expect("parsed neighbor");
    assert_eq!(neighbor_id, Some("wE:pH".to_string()));
}
