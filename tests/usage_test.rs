use harness_queue::harness::agy::AgyHarness;
use harness_queue::harness::claude::ClaudeHarness;
use harness_queue::harness::codex::CodexHarness;
use harness_queue::harness::opencode::OpenCodeHarness;
use harness_queue::harness::pi::PiHarness;
use harness_queue::harness::HarnessAdapter;
use harness_queue::models::{AgentTarget, HarnessKind};
use harness_queue::terminal::mock::MockTerminalAdapter;

#[test]
fn test_usage_summaries_only_for_supported_agents() {
    let mock_terminal = MockTerminalAdapter::new();

    let codex_target = AgentTarget {
        terminal_id: "term_1".to_string(),
        pane_id: "pane_1".to_string(),
        pid: 100,
        harness: HarnessKind::Codex,
        session_id: "sess_codex".to_string(),
        cwd: "/tmp/project".to_string(),
    };
    let codex_harness = CodexHarness::new();
    let codex_usage = codex_harness.get_usage_summary(&codex_target, &mock_terminal);
    assert!(codex_usage.is_some(), "Codex must produce a usage summary");

    let claude_target = AgentTarget {
        terminal_id: "term_2".to_string(),
        pane_id: "pane_2".to_string(),
        pid: 101,
        harness: HarnessKind::ClaudeCode,
        session_id: "sess_claude".to_string(),
        cwd: "/tmp/project".to_string(),
    };
    let claude_harness = ClaudeHarness;
    let claude_usage = claude_harness.get_usage_summary(&claude_target, &mock_terminal);
    assert!(
        claude_usage.is_some(),
        "Claude Code must produce a usage summary"
    );

    let agy_target = AgentTarget {
        terminal_id: "term_3".to_string(),
        pane_id: "pane_3".to_string(),
        pid: 102,
        harness: HarnessKind::Agy,
        session_id: "sess_agy".to_string(),
        cwd: "/tmp/project".to_string(),
    };
    let agy_harness = AgyHarness;
    let agy_usage = agy_harness.get_usage_summary(&agy_target, &mock_terminal);
    assert!(agy_usage.is_some(), "AGY must produce a usage summary");

    // Unsupported agents must return None:
    let pi_target = AgentTarget {
        terminal_id: "term_4".to_string(),
        pane_id: "pane_4".to_string(),
        pid: 103,
        harness: HarnessKind::Pi,
        session_id: "sess_pi".to_string(),
        cwd: "/tmp/project".to_string(),
    };
    let pi_harness = PiHarness;
    assert_eq!(
        pi_harness.get_usage_summary(&pi_target, &mock_terminal),
        None,
        "Pi must not return a usage summary"
    );

    let opencode_target = AgentTarget {
        terminal_id: "term_5".to_string(),
        pane_id: "pane_5".to_string(),
        pid: 104,
        harness: HarnessKind::OpenCode,
        session_id: "sess_opencode".to_string(),
        cwd: "/tmp/project".to_string(),
    };
    let opencode_harness = OpenCodeHarness;
    assert_eq!(
        opencode_harness.get_usage_summary(&opencode_target, &mock_terminal),
        None,
        "OpenCode must not return a usage summary"
    );
}
