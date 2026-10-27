use harness_queue::models::HarnessKind;
use harness_queue::terminal::ghostty::{
    build_ghostty_send_script, find_ghostty_neighbor, parse_ghostty_terminals_output,
    GhosttyTerminalEntry,
};
use harness_queue::terminal::NeighborDetection;
use std::path::Path;

#[test]
fn test_parse_ghostty_terminals_output() {
    let raw = "term_1\t/Users/romey/Developer/harness-queue\tOpenAI Codex\nterm_2\t/tmp\tbash\n";
    let entries = parse_ghostty_terminals_output(raw);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].id, "term_1");
    assert_eq!(entries[0].working_directory, "/Users/romey/Developer/harness-queue");
    assert_eq!(entries[0].title, "OpenAI Codex");
}

#[test]
fn test_build_ghostty_send_script() {
    let script = build_ghostty_send_script("term_123", "echo \"hello world\"");
    assert!(script.contains("id of term is \"term_123\""));
    assert!(script.contains("input text \"echo \\\"hello world\\\"\""));
    assert!(script.contains("send key \"enter\""));
}

#[test]
fn test_ghostty_neighbor_only_detects_same_folder() {
    let entries = vec![
        GhosttyTerminalEntry {
            id: "term_caller".to_string(),
            working_directory: "/Users/romey/Developer/harness-queue".to_string(),
            title: "hq".to_string(),
        },
        GhosttyTerminalEntry {
            id: "term_agent_same_dir".to_string(),
            working_directory: "/Users/romey/Developer/harness-queue".to_string(),
            title: "OpenAI Codex".to_string(),
        },
        GhosttyTerminalEntry {
            id: "term_agent_diff_dir".to_string(),
            working_directory: "/Users/romey/Developer/Hestia.git".to_string(),
            title: "OpenAI Codex".to_string(),
        },
    ];

    let cwd = Path::new("/Users/romey/Developer/harness-queue");
    let res = find_ghostty_neighbor("term_caller", cwd, &entries);

    match res {
        NeighborDetection::Exact(target) => {
            assert_eq!(target.terminal_id, "term_agent_same_dir");
            assert_eq!(target.harness, HarnessKind::Codex);
        }
        _ => panic!("Expected Exact neighbor in same folder"),
    }
}

#[test]
fn test_ghostty_neighbor_rejects_different_folder() {
    let entries = vec![
        GhosttyTerminalEntry {
            id: "term_caller".to_string(),
            working_directory: "/Users/romey/Developer/harness-queue".to_string(),
            title: "hq".to_string(),
        },
        GhosttyTerminalEntry {
            id: "term_agent_diff_dir".to_string(),
            working_directory: "/Users/romey/Developer/Hestia.git".to_string(),
            title: "OpenAI Codex".to_string(),
        },
    ];

    let cwd = Path::new("/Users/romey/Developer/harness-queue");
    let res = find_ghostty_neighbor("term_caller", cwd, &entries);

    match res {
        NeighborDetection::NoneFound(msg) => {
            assert!(msg.contains("No adjacent agent found in the current Ghostty split pane and folder"));
        }
        _ => panic!("Expected NoneFound because agent is in a different folder"),
    }
}
