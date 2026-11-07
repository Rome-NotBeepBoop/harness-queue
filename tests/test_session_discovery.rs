use harness_queue::session_discovery::{
    format_session_label, format_timestamp, parse_iso_or_sqlite_time, DiscoveredSession,
};

#[test]
fn test_format_session_label_exact_style() {
    let session = DiscoveredSession {
        session_id: "conv-123".to_string(),
        title: "adding features in hq".to_string(),
        workspace: "harness-queue".to_string(),
        updated_at: Some(1794051000),
        formatted_time: "07/11/2026 11:30pm".to_string(),
    };
    let label = format_session_label(&session);
    assert_eq!(label, "harness-queue 'adding features in hq' 07/11/2026 11:30pm");
}

#[test]
fn test_format_session_label_empty_title_fallback() {
    let session = DiscoveredSession {
        session_id: "conv-456".to_string(),
        title: "".to_string(),
        workspace: "harness-queue".to_string(),
        updated_at: None,
        formatted_time: "unknown".to_string(),
    };
    let label = format_session_label(&session);
    assert_eq!(label, "harness-queue 'untitled' unknown");
}

#[test]
fn test_parse_sqlite_timestamp() {
    let raw = "2026-11-07 22:30:00+00:00";
    let ts = parse_iso_or_sqlite_time(raw);
    assert!(ts.is_some(), "Should parse sqlite timestamp format");
}

#[test]
fn test_format_timestamp_structure() {
    // 1794051000 secs since epoch
    let formatted = format_timestamp(1794051000);
    assert!(!formatted.is_empty());
    // Should have either am or pm
    assert!(formatted.ends_with("am") || formatted.ends_with("pm"));
}
