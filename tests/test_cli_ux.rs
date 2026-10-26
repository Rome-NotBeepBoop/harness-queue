use harness_queue::cli::{format_agent_detection, format_mode_selection_menu};
use harness_queue::models::{AgentTarget, HarnessKind};

#[test]
fn test_cli_ux_formatting() {
    let target = AgentTarget {
        terminal_id: "term_1".to_string(),
        pane_id: "w1:p3".to_string(),
        pid: 1234,
        harness: HarnessKind::Codex,
        session_id: "7f2a-abcd".to_string(),
        cwd: "/tmp".to_string(),
    };

    let detection = format_agent_detection("Herdr", &target);
    assert!(detection.contains("✓ Herdr detected"));
    assert!(detection.contains("✓ Adjacent agent: OpenAI Codex"));
    assert!(detection.contains("Session 7f2a · pane w1:p3"));

    let menu = format_mode_selection_menu();
    assert!(menu.contains("Harness Queue v0.1"));
    assert!(menu.contains("1) Auto-resume"));
    assert!(menu.contains("2) Auto-send"));
    assert!(menu.contains("› Select [1]:"));
}
