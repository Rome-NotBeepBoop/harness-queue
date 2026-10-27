use harness_queue::models::HarnessKind;
use harness_queue::terminal::orca::{parse_orca_terminals_json, OrcaTerminalEntry};
use harness_queue::terminal::NeighborDetection;

#[test]
fn test_parse_orca_terminal_list_json() {
    let json_data = r#"{
        "ok": true,
        "result": {
            "terminals": [
                {
                    "handle": "term_caller",
                    "tabId": "tab_123",
                    "leafId": "leaf_1",
                    "worktreeId": "wt_1",
                    "worktreePath": "/Users/romey/Developer/harness-queue",
                    "title": "hq terminal",
                    "connected": true,
                    "writable": true,
                    "preview": "❯ hq",
                    "agentIdentity": null
                },
                {
                    "handle": "term_agent",
                    "tabId": "tab_123",
                    "leafId": "leaf_2",
                    "worktreeId": "wt_1",
                    "worktreePath": "/Users/romey/Developer/harness-queue",
                    "title": "Codex session",
                    "connected": true,
                    "writable": true,
                    "preview": "Codex ready\n❯",
                    "agentIdentity": "codex"
                }
            ]
        }
    }"#;

    let terminals = parse_orca_terminals_json(json_data).expect("parsed orca terminals");
    assert_eq!(terminals.len(), 2);
    assert_eq!(terminals[0].handle, "term_caller");
    assert_eq!(terminals[1].handle, "term_agent");
    assert_eq!(terminals[1].agent_identity.as_deref(), Some("codex"));
}

#[test]
fn test_orca_neighbor_selection_prefers_same_tab() {
    use harness_queue::terminal::orca::find_orca_neighbor;
    use std::path::Path;

    let entries = vec![
        OrcaTerminalEntry {
            handle: "term_caller".to_string(),
            tab_id: "tab_split".to_string(),
            worktree_id: "wt_1".to_string(),
            worktree_path: "/tmp/project".to_string(),
            title: "hq".to_string(),
            connected: true,
            writable: true,
            orphaned: false,
            preview: "".to_string(),
            agent_identity: None,
        },
        OrcaTerminalEntry {
            handle: "term_same_tab_agent".to_string(),
            tab_id: "tab_split".to_string(),
            worktree_id: "wt_1".to_string(),
            worktree_path: "/tmp/project".to_string(),
            title: "Claude Code".to_string(),
            connected: true,
            writable: true,
            orphaned: false,
            preview: "Claude Code v2.1\n>".to_string(),
            agent_identity: Some("claude".to_string()),
        },
        OrcaTerminalEntry {
            handle: "term_other_tab_agent".to_string(),
            tab_id: "tab_other".to_string(),
            worktree_id: "wt_1".to_string(),
            worktree_path: "/tmp/project".to_string(),
            title: "Codex".to_string(),
            connected: true,
            writable: true,
            orphaned: false,
            preview: "".to_string(),
            agent_identity: Some("codex".to_string()),
        },
    ];

    let cwd = Path::new("/tmp/project");
    let res = find_orca_neighbor("term_caller", Some("tab_split"), cwd, &entries);
    match res {
        NeighborDetection::Exact(target) => {
            assert_eq!(target.terminal_id, "term_same_tab_agent");
            assert_eq!(target.harness, HarnessKind::ClaudeCode);
        }
        _ => panic!("Expected Exact neighbor in same tab"),
    }
}

#[test]
fn test_orca_never_detects_agents_outside_current_tab() {
    use harness_queue::terminal::orca::find_orca_neighbor;
    use std::path::Path;

    // Caller is alone in "tab_caller". An agent exists in "tab_other" in the same folder.
    let entries = vec![
        OrcaTerminalEntry {
            handle: "term_caller".to_string(),
            tab_id: "tab_caller".to_string(),
            worktree_id: "wt_1".to_string(),
            worktree_path: "/tmp/project".to_string(),
            title: "hq".to_string(),
            connected: true,
            writable: true,
            orphaned: false,
            preview: "".to_string(),
            agent_identity: None,
        },
        OrcaTerminalEntry {
            handle: "term_other_tab_agent".to_string(),
            tab_id: "tab_other".to_string(),
            worktree_id: "wt_1".to_string(),
            worktree_path: "/tmp/project".to_string(),
            title: "Codex".to_string(),
            connected: true,
            writable: true,
            orphaned: false,
            preview: "Codex ready\n❯".to_string(),
            agent_identity: Some("codex".to_string()),
        },
    ];

    let cwd = Path::new("/tmp/project");
    let res = find_orca_neighbor("term_caller", Some("tab_caller"), cwd, &entries);
    match res {
        NeighborDetection::NoneFound(msg) => {
            assert!(msg.contains("No adjacent agent found in the current split pane and folder"));
        }
        _ => panic!("Expected NoneFound because agent is in a different tab"),
    }
}

#[test]
fn test_orca_never_detects_agents_outside_current_folder() {
    use harness_queue::terminal::orca::find_orca_neighbor;
    use std::path::Path;

    // Both are in "tab_split", but the agent is in a completely different folder ("/Users/other/Hestia")
    let entries = vec![
        OrcaTerminalEntry {
            handle: "term_caller".to_string(),
            tab_id: "tab_split".to_string(),
            worktree_id: "wt_1".to_string(),
            worktree_path: "/Users/romey/Developer/harness-queue".to_string(),
            title: "hq".to_string(),
            connected: true,
            writable: true,
            orphaned: false,
            preview: "".to_string(),
            agent_identity: None,
        },
        OrcaTerminalEntry {
            handle: "term_hestia_agent".to_string(),
            tab_id: "tab_split".to_string(),
            worktree_id: "wt_hestia".to_string(),
            worktree_path: "/Users/romey/Developer/Hestia.git".to_string(),
            title: "Codex".to_string(),
            connected: true,
            writable: true,
            orphaned: false,
            preview: "Codex ready\n❯".to_string(),
            agent_identity: Some("codex".to_string()),
        },
    ];

    let cwd = Path::new("/Users/romey/Developer/harness-queue");
    let res = find_orca_neighbor("term_caller", Some("tab_split"), cwd, &entries);
    match res {
        NeighborDetection::NoneFound(msg) => {
            assert!(msg.contains("No adjacent agent found in the current split pane and folder"));
        }
        _ => panic!("Expected NoneFound because agent is in a different folder"),
    }
}
