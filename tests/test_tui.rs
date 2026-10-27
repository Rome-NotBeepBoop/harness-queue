use harness_queue::harness::mock::MockHarness;
use harness_queue::models::{AgentTarget, HarnessKind, ItemStatus, Mode, State};
use harness_queue::supervisor::Supervisor;
use harness_queue::terminal::mock::MockTerminalAdapter;
use harness_queue::tui::app::TuiApp;
use std::sync::Arc;

fn create_test_supervisor() -> Supervisor {
    let mock_terminal = Arc::new(MockTerminalAdapter::new());
    let mock_harness = Arc::new(MockHarness::new(HarnessKind::Codex));
    let mut supervisor = Supervisor::new(mock_terminal, mock_harness, Mode::AutoSend);
    supervisor.target = Some(AgentTarget {
        terminal_id: "term_test".to_string(),
        pane_id: "term_test".to_string(),
        pid: 1234,
        harness: HarnessKind::Codex,
        session_id: "sess_123".to_string(),
        cwd: "/tmp/project".to_string(),
    });
    let _ = supervisor.state_machine.transition(State::SelectingMode);
    let _ = supervisor.state_machine.transition(State::Watching);
    supervisor
}

#[test]
fn test_tui_app_initialization() {
    let mut supervisor = create_test_supervisor();
    let app = TuiApp::new(&mut supervisor);

    assert!(app.running);
    assert_eq!(app.input, "");
    assert_eq!(app.cursor_position, 0);
    assert!(!app.logs.is_empty());
    assert!(app
        .logs
        .iter()
        .any(|l| l.contains("Supervisor initialized")));
}

#[test]
fn test_tui_app_typing_and_submitting_prompt() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    let mut supervisor = create_test_supervisor();
    let mut app = TuiApp::new(&mut supervisor);

    let key_a = KeyEvent {
        code: KeyCode::Char('h'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    app.handle_key(key_a);

    let key_b = KeyEvent {
        code: KeyCode::Char('i'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    app.handle_key(key_b);

    assert_eq!(app.input, "hi");
    assert_eq!(app.cursor_position, 2);

    // Press enter
    let key_enter = KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    app.handle_key(key_enter);

    assert_eq!(app.input, "");
    assert_eq!(app.cursor_position, 0);
    assert_eq!(app.supervisor.queue.len(), 1);
    assert_eq!(app.supervisor.queue.get(1).unwrap().prompt, "hi");
    assert_eq!(
        app.supervisor.queue.get(1).unwrap().status,
        ItemStatus::Queued
    );
    assert!(app.logs.iter().any(|l| l.contains("✓ Prompt #1 queued")));
}

#[test]
fn test_tui_app_submitting_with_image() {
    let temp_dir = std::env::temp_dir();
    let img_path = temp_dir.join("tui_test_image.png");
    std::fs::write(&img_path, b"dummy png").expect("write file");

    let mut supervisor = create_test_supervisor();
    let mut app = TuiApp::new(&mut supervisor);

    app.input = format!("Check @{} for errors", img_path.display());
    app.cursor_position = app.input.len();
    app.submit_input();

    assert_eq!(app.supervisor.queue.len(), 1);
    let item = app.supervisor.queue.get(1).unwrap();
    assert!(item.attachments_summary.is_some());
    assert!(item
        .attachments_summary
        .as_ref()
        .unwrap()
        .contains("tui_test_image.png"));
    assert!(app.logs.iter().any(|l| l.contains("tui_test_image.png")));

    let _ = std::fs::remove_file(img_path);
}

#[test]
fn test_tui_app_exit_on_esc() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    let mut supervisor = create_test_supervisor();
    let mut app = TuiApp::new(&mut supervisor);

    let key_esc = KeyEvent {
        code: KeyCode::Esc,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    app.handle_key(key_esc);
    assert!(!app.running);
}
