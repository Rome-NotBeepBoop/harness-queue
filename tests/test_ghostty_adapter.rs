use harness_queue::terminal::ghostty::{
    build_ghostty_send_script, parse_ghostty_terminals_output, GhosttyTerminalEntry,
};
use harness_queue::terminal::TerminalError;

#[test]
fn test_parse_ghostty_terminals_output() {
    let script_output = "term_aaa\t/Users/romey/Developer/harness-queue\tTerminal 1\nterm_bbb\t/Users/romey/Developer/Hestia.git\tCodex\n";
    let entries = parse_ghostty_terminals_output(script_output);
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries[0],
        GhosttyTerminalEntry {
            id: "term_aaa".to_string(),
            working_directory: "/Users/romey/Developer/harness-queue".to_string(),
            title: "Terminal 1".to_string(),
        }
    );
    assert_eq!(
        entries[1],
        GhosttyTerminalEntry {
            id: "term_bbb".to_string(),
            working_directory: "/Users/romey/Developer/Hestia.git".to_string(),
            title: "Codex".to_string(),
        }
    );
}

#[test]
fn test_build_ghostty_send_script_escapes_properly() {
    let script = build_ghostty_send_script("term_123", "Run test with \"quotes\" & newline\n");
    assert!(script.contains("tell application \"Ghostty\""));
    assert!(script.contains("input text"));
    assert!(script.contains("send key \"enter\""));
    assert!(script.contains("term_123"));
}

#[test]
fn test_permission_denied_detection() {
    let err_str = "execution error: Ghostty got an error: Not authorized to send Apple events to Ghostty. (-1743)";
    let parsed = harness_queue::terminal::ghostty::classify_osascript_error(err_str);
    match parsed {
        TerminalError::PermissionDenied(msg) => {
            assert!(msg.contains("macOS Automation permission"));
        }
        other => panic!("Expected PermissionDenied, got {:?}", other),
    }
}
