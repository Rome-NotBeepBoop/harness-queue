use harness_queue::cli::select_ambiguous_target;
use harness_queue::models::{AgentTarget, HarnessKind};
use std::io::Cursor;

#[test]
fn test_select_ambiguous_target_with_multiple_agy_agents() {
    let targets = vec![
        AgentTarget {
            terminal_id: "term_bbda9ebd".to_string(),
            pane_id: "term_bbda9ebd".to_string(),
            pid: 100,
            harness: HarnessKind::Agy,
            session_id: "term_bbda9ebd".to_string(),
            cwd: "/Users/romey/Developer/harness-queue".to_string(),
        },
        AgentTarget {
            terminal_id: "term_743d09f8".to_string(),
            pane_id: "term_743d09f8".to_string(),
            pid: 101,
            harness: HarnessKind::Agy,
            session_id: "term_743d09f8".to_string(),
            cwd: "/Users/romey/Developer/harness-queue".to_string(),
        },
    ];

    // Case 1: user presses Enter (default to 1)
    let mut input = Cursor::new(b"\n");
    let mut output = Vec::new();
    let selected = select_ambiguous_target(&mut input, &mut output, &targets);
    assert_eq!(selected, Some(targets[0].clone()));

    let output_str = String::from_utf8(output).unwrap();
    assert!(output_str.contains("Multiple agents detected in this terminal tab (2 found)"));
    assert!(output_str.contains("1) Google Antigravity (term_bbda9ebd)"));
    assert!(output_str.contains("2) Google Antigravity (term_743d09f8)"));

    // Case 2: user selects 2
    let mut input = Cursor::new(b"2\n");
    let mut output = Vec::new();
    let selected = select_ambiguous_target(&mut input, &mut output, &targets);
    assert_eq!(selected, Some(targets[1].clone()));

    // Case 3: user cancels / enters q
    let mut input = Cursor::new(b"q\n");
    let mut output = Vec::new();
    let selected = select_ambiguous_target(&mut input, &mut output, &targets);
    assert_eq!(selected, None);
}
