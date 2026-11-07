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

#[test]
fn test_select_fallback_agent_cancelled_on_ctrl_c_at_harness_menu() {
    // \x03 is Ctrl+C (ETX)
    let mut input = Cursor::new(b"\x03\n");
    let mut output = Vec::new();

    let target = select_fallback_agent_target(&mut input, &mut output);
    assert!(target.is_none(), "Should cancel when user hits Ctrl+C at harness menu");
}

#[test]
fn test_select_fallback_agent_cancelled_on_ctrl_c_at_session_menu() {
    // "1" selects AGY, then \x03\n cancels at session menu
    let mut input = Cursor::new(b"1\n\x03\n");
    let mut output = Vec::new();

    let target = select_fallback_agent_target(&mut input, &mut output);
    assert!(target.is_none(), "Should cancel when user hits Ctrl+C at session selection menu");
}

#[test]
fn test_select_fallback_agent_cancelled_on_ctrl_c_at_custom_session_id() {
    // "1" (AGY), "999" (custom), then \x03\n at session ID prompt
    let mut input = Cursor::new(b"1\n999\n\x03\n");
    let mut output = Vec::new();

    let target = select_fallback_agent_target(&mut input, &mut output);
    assert!(target.is_none(), "Should cancel when user hits Ctrl+C at custom session ID prompt");
}

#[test]
fn test_select_fallback_agent_cancelled_on_ctrl_c_at_folder_path() {
    // "1" (AGY), "999" (custom), "sess-1\n", then \x03\n at folder path prompt
    let mut input = Cursor::new(b"1\n999\nsess-1\n\x03\n");
    let mut output = Vec::new();

    let target = select_fallback_agent_target(&mut input, &mut output);
    assert!(target.is_none(), "Should cancel when user hits Ctrl+C at folder path prompt");
}

#[test]
fn test_is_cancel_input_matches_signals_and_words() {
    use harness_queue::cli::is_cancel_input;

    assert!(is_cancel_input("\x03"));
    assert!(is_cancel_input("\x03\n"));
    assert!(is_cancel_input("  \x03  "));
    assert!(is_cancel_input("\x04"));
    assert!(is_cancel_input("q"));
    assert!(is_cancel_input("Q"));
    assert!(is_cancel_input("exit"));
    assert!(is_cancel_input("EXIT"));
    assert!(is_cancel_input("cancel"));

    assert!(!is_cancel_input(""));
    assert!(!is_cancel_input("1"));
    assert!(!is_cancel_input("2"));
    assert!(!is_cancel_input("new-session"));
    assert!(!is_cancel_input("/path/to/folder"));
}
