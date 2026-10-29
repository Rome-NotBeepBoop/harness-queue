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

#[test]
fn test_compute_header_lines_wide_screen_is_two_liner() {
    use harness_queue::tui::app::compute_header_lines;
    use ratatui::style::Style;
    use ratatui::text::Span;

    let target = AgentTarget {
        terminal_id: "term_tui_1".to_string(),
        pane_id: "pane_tui_1".to_string(),
        pid: 200,
        harness: HarnessKind::Agy,
        session_id: "term_bbda9ebd".to_string(),
        cwd: "/Users/romey/Developer/harness-queue".to_string(),
    };

    let badge = Span::styled(" ✓ READY ", Style::default());
    let badge_len = 9;

    // Big screen (width 160)
    let wide_lines = compute_header_lines(
        "Orca IDE",
        "Auto-send",
        badge.clone(),
        badge_len,
        Some(&target),
        Some("Antigravity: Active · Quota available"),
        160,
    );
    assert_eq!(
        wide_lines.len(),
        2,
        "When terminal is big/wide, Session Overview must be exactly a 2-liner!"
    );

    // Narrow screen (split pane width 50)
    let narrow_lines = compute_header_lines(
        "Orca IDE",
        "Auto-send",
        badge,
        badge_len,
        Some(&target),
        Some("Antigravity: Active · Quota available"),
        50,
    );
    assert!(
        narrow_lines.len() >= 4,
        "When terminal is narrow, info must autofall below onto new lines! Found: {}",
        narrow_lines.len()
    );
}
