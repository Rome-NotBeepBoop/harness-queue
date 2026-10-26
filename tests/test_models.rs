use harness_queue::models::{
    AgentTarget, HarnessKind, ItemStatus, Mode, QueueItem, DEFAULT_CONTINUATION_PROMPT,
};

#[test]
fn test_default_continuation_prompt_exact_text() {
    assert_eq!(
        DEFAULT_CONTINUATION_PROMPT,
        "Continue the unfinished work. Inspect the session history and repository state before making changes. Do not repeat completed tasks."
    );
}

#[test]
fn test_queue_item_lifecycle() {
    let mut item = QueueItem::new(1, "Fix tests".to_string());
    assert_eq!(item.id, 1);
    assert_eq!(item.prompt, "Fix tests");
    assert_eq!(item.status, ItemStatus::Queued);
    assert!(item.submitted_at.is_none());

    item.mark_submitting(100);
    assert_eq!(item.status, ItemStatus::Submitting);
    assert_eq!(item.submitted_at, Some(100));

    item.mark_acknowledged();
    assert_eq!(item.status, ItemStatus::Acknowledged);

    item.mark_uncertain();
    assert_eq!(item.status, ItemStatus::Uncertain);
}

#[test]
fn test_agent_target_and_harness_kinds() {
    let target = AgentTarget {
        terminal_id: "term_123".to_string(),
        pane_id: "w1:p3".to_string(),
        pid: 4567,
        harness: HarnessKind::Codex,
        session_id: "7f2a-abcd".to_string(),
        cwd: "/Users/romey/Developer/harness-queue".to_string(),
    };
    assert_eq!(target.pane_id, "w1:p3");
    assert_eq!(target.harness, HarnessKind::Codex);
    assert_eq!(target.short_session_id(), "7f2a");
}

#[test]
fn test_mode_display() {
    assert_eq!(Mode::AutoResume.as_str(), "Auto-resume");
    assert_eq!(Mode::AutoSend.as_str(), "Auto-send");
}
