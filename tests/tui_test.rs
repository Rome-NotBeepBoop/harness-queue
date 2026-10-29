use harness_queue::harness::agy::AgyHarness;
use harness_queue::models::{AgentTarget, HarnessKind, Mode};
use harness_queue::supervisor::Supervisor;
use harness_queue::terminal::mock::MockTerminalAdapter;
use harness_queue::tui::app::TuiApp;
use std::sync::Arc;

#[test]
fn test_tui_app_initialization_and_usage() {
    let mock_terminal = Arc::new(MockTerminalAdapter::new());
    let agy_harness = Arc::new(AgyHarness);
    let mut supervisor = Supervisor::new(mock_terminal, agy_harness, Mode::AutoSend);
    supervisor.target = Some(AgentTarget {
        terminal_id: "term_tui_1".to_string(),
        pane_id: "pane_tui_1".to_string(),
        pid: 200,
        harness: HarnessKind::Agy,
        session_id: "sess_agy_tui".to_string(),
        cwd: "/Users/romey/Developer/harness-queue".to_string(),
    });

    let app = TuiApp::new(&mut supervisor);
    assert!(app.running);
    assert!(!app.logs.is_empty());

    let usage = app.supervisor.get_target_usage_summary();
    assert!(usage.is_some());
    assert!(usage.unwrap().contains("Antigravity"));
}
