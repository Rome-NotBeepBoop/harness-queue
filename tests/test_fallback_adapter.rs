use harness_queue::models::{AgentTarget, HarnessKind};
use harness_queue::terminal::fallback::FallbackAdapter;
use harness_queue::terminal::{AgentStatus, TerminalAdapter};
use std::sync::Mutex;

static ENV_MUTEX: Mutex<()> = Mutex::new(());

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
    let _guard = ENV_MUTEX.lock().unwrap();
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
    let _guard = ENV_MUTEX.lock().unwrap();
    std::env::set_var("MOCK_SPAWN", "1");
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
    std::env::remove_var("MOCK_SPAWN");
}

#[test]
fn test_fallback_adapter_turn_lifecycle_busy_to_ready() {
    let _guard = ENV_MUTEX.lock().unwrap();
    // Point AGY_BIN to a command that takes 100ms (e.g. "sleep")
    std::env::set_var("AGY_BIN", "sleep");
    let target = AgentTarget {
        terminal_id: "standalone".to_string(),
        pane_id: "tab_agent".to_string(),
        pid: 0,
        harness: HarnessKind::Agy,
        session_id: "sess-test".to_string(),
        cwd: std::env::current_dir().unwrap().to_string_lossy().to_string(),
    };

    let adapter = FallbackAdapter::new(target.clone(), false);
    assert_eq!(adapter.get_agent_status("tab_agent").unwrap(), AgentStatus::Ready);

    let ack = adapter.send_prompt(&target, "0.15").expect("send_prompt should spawn sleep");
    assert!(ack.acknowledged);

    // Immediately after spawn, the agent must be Busy
    assert_eq!(adapter.get_agent_status("tab_agent").unwrap(), AgentStatus::Busy);

    // Wait for sleep to finish
    std::thread::sleep(std::time::Duration::from_millis(250));

    // After finish, the agent must transition back to Ready
    assert_eq!(adapter.get_agent_status("tab_agent").unwrap(), AgentStatus::Ready);

    std::env::remove_var("AGY_BIN");
}

#[test]
fn test_fallback_adapter_spawn_failure_propagates_error() {
    let _guard = ENV_MUTEX.lock().unwrap();
    std::env::set_var("AGY_BIN", "nonexistent_binary_xyz_123");
    let target = AgentTarget {
        terminal_id: "standalone".to_string(),
        pane_id: "tab_agent".to_string(),
        pid: 0,
        harness: HarnessKind::Agy,
        session_id: "sess-test".to_string(),
        cwd: std::env::current_dir().unwrap().to_string_lossy().to_string(),
    };

    let adapter = FallbackAdapter::new(target.clone(), false);
    let res = adapter.send_prompt(&target, "hello");
    assert!(res.is_err(), "Spawn failure must return Err and not falsely acknowledge");

    std::env::remove_var("AGY_BIN");
}

