use harness_queue::harness::agy::AgyHarness;
use harness_queue::models::{AgentTarget, HarnessKind, Mode};
use harness_queue::supervisor::Supervisor;
use harness_queue::terminal::mock::MockTerminalAdapter;
use harness_queue::tui::app::TuiApp;
use std::sync::Arc;

#[test]
fn test_tui_app_initialization_and_usage() {
    let mock_terminal = Arc::new(MockTerminalAdapter::new());
    let agy_harness = Arc::new(AgyHarness);
    let mut supervisor = Supervisor::new(mock_terminal, agy_harness, Mode::AutoSend);
    supervisor.target = Some(AgentTarget {
        terminal_id: "term_tui_1".to_string(),
        pane_id: "pane_tui_1".to_string(),
        pid: 200,
        harness: HarnessKind::Agy,
        session_id: "sess_agy_tui".to_string(),
        cwd: "/Users/romey/Developer/harness-queue".to_string(),
    });

    let app = TuiApp::new(&mut supervisor);
    assert!(app.running);
    assert!(!app.logs.is_empty());

    let usage = app.supervisor.get_target_usage_summary();
    assert!(usage.is_some());
    assert!(usage.unwrap().contains("Antigravity"));
}

#[test]
fn test_compute_header_lines_wide_screen_is_two_liner() {
    use harness_queue::tui::app::compute_header_lines;
    use ratatui::style::Style;
    use ratatui::text::Span;

    let target = AgentTarget {
        terminal_id: "term_tui_1".to_string(),
        pane_id: "pane_tui_1".to_string(),
        pid: 200,
        harness: HarnessKind::Agy,
        session_id: "term_bbda9ebd".to_string(),
        cwd: "/Users/romey/Developer/harness-queue".to_string(),
    };

    let badge = Span::styled(" ✓ READY ", Style::default());
    let badge_len = 9;

    // Big screen (width 160)
    let wide_lines = compute_header_lines(
        "Orca IDE",
        "Auto-send",
        badge.clone(),
        badge_len,
        Some(&target),
        Some("Antigravity: Active · Quota available"),
        160,
    );
    assert_eq!(
        wide_lines.len(),
        2,
        "When terminal is big/wide, Session Overview must be exactly a 2-liner!"
    );

    // Narrow screen (split pane width 50)
    let narrow_lines = compute_header_lines(
        "Orca IDE",
        "Auto-send",
        badge,
        badge_len,
        Some(&target),
        Some("Antigravity: Active · Quota available"),
        50,
    );
    assert!(
        narrow_lines.len() >= 4,
        "When terminal is narrow, info must autofall below onto new lines! Found: {}",
        narrow_lines.len()
    );
}

#[test]
fn test_wrap_text_splits_cleanly_and_handles_long_words() {
    use harness_queue::tui::app::wrap_text;

    // Normal wrapping
    let text = "This is a prompt that will be wrapped across several lines";
    let wrapped = wrap_text(text, 20);
    for line in &wrapped {
        assert!(
            line.chars().count() <= 20,
            "Line exceeded 20 chars: '{}'",
            line
        );
    }
    assert_eq!(
        wrapped.join(" "),
        "This is a prompt that will be wrapped across several lines"
    );

    // Very long word exceeding max_width
    let long_word = "a".repeat(35);
    let wrapped_long = wrap_text(&long_word, 10);
    assert_eq!(wrapped_long.len(), 4);
    assert_eq!(wrapped_long[0].len(), 10);
    assert_eq!(wrapped_long[1].len(), 10);
    assert_eq!(wrapped_long[2].len(), 10);
    assert_eq!(wrapped_long[3].len(), 5);
}

#[test]
fn test_wrap_log_line_indents_timestamp() {
    use harness_queue::tui::app::wrap_log_line;

    let log = "14:59:21 ● Watching Google Antigravity session term_0937adc6";
    // Wrap to 35 chars
    let lines = wrap_log_line(log, 35);
    assert!(lines.len() >= 2);

    // First line should have timestamp
    let line0_str: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(
        line0_str.starts_with("14:59:21 "),
        "First line must start with timestamp: {}",
        line0_str
    );

    // Second line should indent with 9 spaces
    let line1_str: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(
        line1_str.starts_with("         "),
        "Continuation line must indent under timestamp: '{}'",
        line1_str
    );
}

#[test]
fn test_compute_queue_item_lines_wraps_without_truncation() {
    use harness_queue::models::QueueItem;
    use harness_queue::tui::app::compute_queue_item_lines;

    let mut item = QueueItem::new(
        1,
        "reply with \"hello\" this is a test if its working, do not think, do not reason, just reply with \"hello\"".to_string(),
    );
    item.attachments_summary = Some("test.png".to_string());

    // Inner width 40 (like in a narrow terminal or split pane)
    let lines = compute_queue_item_lines(&item, false, 40);
    assert!(
        lines.len() >= 3,
        "Long prompt must wrap across multiple lines rather than truncate at 40 chars! Got {} lines",
        lines.len()
    );

    // Verify all prompt words appear in rendered lines
    let all_text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
        .collect();
    assert!(
        all_text.contains("reply with \"hello\""),
        "Missing prompt start"
    );
    assert!(
        all_text.contains("do not reason"),
        "Missing middle of prompt"
    );
    assert!(
        all_text.contains("just"),
        "Missing prompt end (was it truncated?)"
    );
    assert!(all_text.contains("test.png"), "Missing attachment");
}

#[test]
fn test_layout_never_hides_middle_on_small_panes() {
    use ratatui::layout::{Constraint, Direction, Layout, Rect};

    // Small height terminal (height: 16, width: 45) - like mobile terminal in user screenshot
    let size = Rect::new(0, 0, 45, 16);
    let is_narrow = size.width < 80;
    let is_short = size.height < 24;
    assert!(is_narrow);
    assert!(is_short);

    let margin = if is_short || size.width < 75 { 0 } else { 1 };
    assert_eq!(margin, 0, "Margin should be 0 on small/narrow screens");

    let footer_height = if size.height < 16 { 0 } else { 1 };
    let input_height = 3;
    let min_middle_height = if size.height < 20 { 4 } else { 6 };
    let reserved = input_height + footer_height + min_middle_height + (margin * 2);
    let max_header = size.height.saturating_sub(reserved).max(3);
    let header_height = (5u16 + 2).min(max_header);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(margin)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Min(min_middle_height),
            Constraint::Length(input_height),
            Constraint::Length(footer_height),
        ])
        .split(size);

    assert!(
        chunks[1].height >= 4,
        "Middle chunk must be guaranteed at least 4 lines high! Got {}",
        chunks[1].height
    );

    // Now test vertical stacking of middle chunks on narrow screen
    let middle_rect = chunks[1];
    let middle_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(middle_rect);

    assert!(middle_chunks[0].height >= 2, "Prompt Queue must be visible");
    assert!(
        middle_chunks[1].height >= 2,
        "Real-time Event Log must be visible"
    );
    assert_eq!(
        middle_chunks[0].width, 45,
        "Prompt Queue gets full 45 cols width"
    );
    assert_eq!(
        middle_chunks[1].width, 45,
        "Real-time Event Log gets full 45 cols width"
    );
}
