use harness_queue::cli::{format_fallback_notification, select_fallback_agent_target};
use harness_queue::models::HarnessKind;
use std::io::Cursor;

#[test]
fn test_fallback_notification_content() {
    let note = format_fallback_notification();
    assert!(note.contains("No Herdr, Orca, or Ghostty terminal panes detected"));
    assert!(note.contains("Terminal Agent Selection"));
}

#[test]
fn test_select_fallback_agent_interactive_agy_first_session() {
    // Input: "1" for AGY harness, "1" for first session
    let mut input = Cursor::new(b"1\n1\n");
    let mut output = Vec::new();

    let target = select_fallback_agent_target(&mut input, &mut output).expect("should return an AgentTarget");
    assert_eq!(target.harness, HarnessKind::Agy);
    assert_eq!(target.terminal_id, "standalone");
}

#[test]
fn test_select_fallback_agent_custom_session() {
    // Input: "2" (Codex), "99" (custom session), "custom-session-123\n", "/tmp/worktree\n"
    let mut input = Cursor::new(b"2\n99\ncustom-session-123\n/tmp/worktree\n");
    let mut output = Vec::new();

    let target = select_fallback_agent_target(&mut input, &mut output).expect("should return an AgentTarget");
    assert_eq!(target.harness, HarnessKind::Codex);
    assert_eq!(target.session_id, "custom-session-123");
    assert_eq!(target.cwd, "/tmp/worktree");
}

#[test]
fn test_select_fallback_agent_cancelled_on_q() {
    let mut input = Cursor::new(b"q\n");
    let mut output = Vec::new();

    let target = select_fallback_agent_target(&mut input, &mut output);
    assert!(target.is_none());
}
