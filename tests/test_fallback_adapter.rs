use harness_queue::models::{AgentTarget, HarnessKind};
use harness_queue::terminal::fallback::FallbackAdapter;
use harness_queue::terminal::{AgentStatus, TerminalAdapter};

#[test]
fn test_fallback_adapter_metadata() {
    let target = AgentTarget {
        terminal_id: "standalone".to_string(),
        pane_id: "tab_agent".to_string(),
        pid: 0,
        harness: HarnessKind::Agy,
        session_id: "sess-test".to_string(),
        cwd: "/path/to/project".to_string(),
    };

    let adapter = FallbackAdapter::new(target.clone(), false);
    assert!(adapter.terminal_name().contains("Fallback"));
    assert!(adapter.is_available());
    assert_eq!(adapter.detect_current_pane().unwrap(), "standalone_pane");
    assert_eq!(adapter.get_agent_status("tab_agent").unwrap(), AgentStatus::Ready);
    assert!(adapter.verify_target_alive(&target).unwrap());
}

#[test]
fn test_fallback_adapter_windows_terminal_naming() {
    std::env::set_var("WT_SESSION", "mock-guid-1234");
    let target = AgentTarget {
        terminal_id: "standalone".to_string(),
        pane_id: "tab_agent".to_string(),
        pid: 0,
        harness: HarnessKind::Agy,
        session_id: "sess-test".to_string(),
        cwd: "/path/to/project".to_string(),
    };
    let adapter = FallbackAdapter::new(target, false);
    assert_eq!(adapter.terminal_name(), "Windows Terminal (Fallback)");
    std::env::remove_var("WT_SESSION");
}

#[test]
fn test_fallback_adapter_send_prompt_simulated() {
    let target = AgentTarget {
        terminal_id: "standalone".to_string(),
        pane_id: "tab_agent".to_string(),
        pid: 0,
        harness: HarnessKind::Agy,
        session_id: "sess-test".to_string(),
        cwd: "/path/to/project".to_string(),
    };

    let adapter = FallbackAdapter::new(target.clone(), false);
    // Submit prompt (mock/dry run or CLI invocation)
    let ack = adapter.send_prompt(&target, "echo hello").expect("send_prompt should succeed");
    assert!(ack.acknowledged);
    assert!(!ack.details.is_empty());
}
