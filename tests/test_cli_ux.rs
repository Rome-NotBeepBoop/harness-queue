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
fn test_format_boxed_header_and_guide() {
    use harness_queue::cli::{format_boxed_header, format_not_detected_guide};

    let target = AgentTarget {
        terminal_id: "term_7b2d3b68-605c-49dc-92de-e7eb283fe5b1".to_string(),
        pane_id: "term_7b2d3b68-605c-49dc-92de-e7eb283fe5b1".to_string(),
        pid: 0,
        harness: HarnessKind::Codex,
        session_id: "term_7b2d3b68-605c-49dc-92de-e7eb283fe5b1".to_string(),
        cwd: "/Users/romey/Developer/Hestia".to_string(),
    };

    let header = format_boxed_header("Orca IDE", "term_d5d8770b-718b-4c1f", &target, Some("Antigravity: Active · Quota available"));
    assert!(header.contains("Harness Queue (hq) v0.1"));
    assert!(header.contains("Host: Orca IDE"));
    assert!(header.contains("Location: term_d5d8770b"));
    assert!(header.contains("Target: OpenAI Codex (term_7b2d3b68)"));
    assert!(header.contains("Worktree: /Users/romey/Developer/Hestia"));
    assert!(header.contains("Usage: Antigravity: Active · Quota available"));
    assert!(header.starts_with('┌'));
    assert!(header.ends_with('┘'));

    let guide = format_not_detected_guide();
    assert!(guide.contains("Terminal Not Detected"));
    assert!(guide.contains("Orca IDE"));
    assert!(guide.contains("Herdr"));
}

#[test]
fn test_read_initial_input_cases() {
    use harness_queue::cli::{read_initial_input, InitialChoice};
    use std::io::Cursor;

    // Empty input (press Enter) -> AutoResume
    let mut empty = Cursor::new(b"\n");
    let choice = read_initial_input(&mut empty);
    assert_eq!(choice, InitialChoice::AutoResume);

    // "1" -> AutoResume
    let mut one = Cursor::new(b"1\n");
    let choice = read_initial_input(&mut one);
    assert_eq!(choice, InitialChoice::AutoResume);

    // "2" -> AutoSend (no prompt yet)
    let mut two = Cursor::new(b"2\n");
    let choice = read_initial_input(&mut two);
    assert_eq!(choice, InitialChoice::AutoSend(None));

    // Custom prompt typed directly -> AutoSend with that prompt!
    let mut custom = Cursor::new(b"Fix the receiving UI and run tests\n");
    let choice = read_initial_input(&mut custom);
    assert_eq!(
        choice,
        InitialChoice::AutoSend(Some("Fix the receiving UI and run tests".to_string()))
    );

    // EOF / Ctrl+D (0 bytes) -> Cancelled!
    let mut eof = Cursor::new(b"");
    let choice = read_initial_input(&mut eof);
    assert_eq!(choice, InitialChoice::Cancelled);
}

#[test]
fn test_select_ambiguous_target() {
    use harness_queue::cli::select_ambiguous_target;
    use std::io::Cursor;

    let targets = vec![
        AgentTarget {
            terminal_id: "term_1".to_string(),
            pane_id: "term_1".to_string(),
            pid: 0,
            harness: HarnessKind::Codex,
            session_id: "s1".to_string(),
            cwd: "".to_string(),
        },
        AgentTarget {
            terminal_id: "term_2".to_string(),
            pane_id: "term_2".to_string(),
            pid: 0,
            harness: HarnessKind::ClaudeCode,
            session_id: "s2".to_string(),
            cwd: "".to_string(),
        },
    ];

    let mut input = Cursor::new(b"2\n");
    let mut out = Vec::new();
    let selected = select_ambiguous_target(&mut input, &mut out, &targets);
    assert_eq!(selected.unwrap().terminal_id, "term_2");
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

    let input_bytes = b"line 1\\\nline 2\n";
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
