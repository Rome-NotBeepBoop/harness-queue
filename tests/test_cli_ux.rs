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

#[test]
fn test_read_prompt_eof_handling() {
    use harness_queue::cli::{read_prompt_event, InputEvent};
    use std::io::Cursor;

    let mut empty_input = Cursor::new(b"");
    let mut out = Vec::new();
    let event = read_prompt_event(&mut empty_input, &mut out, "› Prompt: ");
    assert!(matches!(event, InputEvent::Eof));
}

#[test]
fn test_read_prompt_multiline_paste() {
    use harness_queue::cli::{read_prompt_event, InputEvent};
    use std::io::Cursor;

    let input_bytes = b"line 1\nline 2\n\n";
    let mut input = Cursor::new(input_bytes);
    let mut out = Vec::new();
    let event = read_prompt_event(&mut input, &mut out, "› Prompt: ");
    match event {
        InputEvent::Text(txt) => {
            assert!(txt.contains("line 1"));
            assert!(txt.contains("line 2"));
        }
        _ => panic!("Expected multiline Text event"),
    }
}
