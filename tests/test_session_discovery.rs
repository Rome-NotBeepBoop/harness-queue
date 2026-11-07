use harness_queue::models::HarnessKind;
use harness_queue::session_discovery::{
    discover_agy_sessions, discover_codex_sessions, discover_sessions_for_harness,
    format_session_label, format_timestamp, parse_iso_or_sqlite_time, DiscoveredSession,
};
use std::io::Write;

#[test]
fn test_format_session_label_exact_style() {
    let session = DiscoveredSession {
        session_id: "conv-123".to_string(),
        title: "adding features in hq".to_string(),
        workspace: "harness-queue".to_string(),
        workspace_path: "/Users/romey/Developer/harness-queue".to_string(),
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
        workspace_path: "/Users/romey/Developer/harness-queue".to_string(),
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

#[test]
fn test_format_timestamp_day_month_year() {
    let raw = "2026-11-07 11:30:00+00:00";
    let ts = parse_iso_or_sqlite_time(raw).unwrap();
    let formatted = format_timestamp(ts);
    // Day should be first: 07/11/2026
    assert!(
        formatted.starts_with("07/11/2026"),
        "Formatted timestamp must be DD/MM/YYYY, got: {}",
        formatted
    );
}

#[test]
fn test_discover_agy_sessions_with_preview_and_subagent_filter() {
    let tmp_dir = std::env::temp_dir().join(format!("hq_test_agy_rich_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp_dir);
    let db_path = tmp_dir.join("conversation_summaries.db");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "CREATE TABLE conversation_summaries (
            conversation_id TEXT PRIMARY KEY,
            title TEXT,
            preview TEXT,
            workspace_uris TEXT,
            last_modified_time TEXT,
            nesting_depth INTEGER
        )",
        [],
    ).unwrap();

    // Top-level session with title
    conn.execute(
        "INSERT INTO conversation_summaries VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            "c-top-1",
            "Cross-Platform Windows Terminal Agent Support",
            "Cross-Platform Windows Terminal Agent Support",
            "[\"file:///Users/romey/Developer/harness-queue\"]",
            "2026-11-07 12:40:41+00:00",
            0
        ],
    ).unwrap();

    // Subagent session with nesting_depth = 1 (should be excluded)
    conn.execute(
        "INSERT INTO conversation_summaries VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            "c-subagent-1",
            "",
            "",
            "[\"file:///Users/romey/Developer/harness-queue\"]",
            "2026-11-07 13:00:00+00:00",
            1
        ],
    ).unwrap();

    // Top-level session with empty title but non-empty preview
    conn.execute(
        "INSERT INTO conversation_summaries VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            "c-top-2",
            "",
            "Explain how harness-queue works\nSecond line",
            "[\"file:///Users/romey/Developer/harness-queue\"]",
            "2026-11-06 10:00:00+00:00",
            0
        ],
    ).unwrap();

    let sessions = discover_agy_sessions(Some(&db_path)).expect("must discover agy sessions");
    // Subagent should be filtered out
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].session_id, "c-top-1");
    assert_eq!(sessions[0].title, "Cross-Platform Windows Terminal Agent Support");
    assert!(sessions[0].formatted_time.starts_with("07/11/2026"));

    assert_eq!(sessions[1].session_id, "c-top-2");
    assert_eq!(sessions[1].title, "Explain how harness-queue works");

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_discover_agy_sessions_from_sqlite() {
    let tmp_dir = std::env::temp_dir().join(format!("hq_test_agy_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp_dir);
    let db_path = tmp_dir.join("conversation_summaries.db");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "CREATE TABLE conversation_summaries (
            conversation_id TEXT PRIMARY KEY,
            title TEXT,
            workspace_uris TEXT,
            last_modified_time TEXT
        )",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO conversation_summaries VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            "c-uuid-1",
            "adding features in hq",
            "[\"file:///Users/romey/Developer/harness-queue\"]",
            "2026-11-07 23:30:00+00:00"
        ],
    ).unwrap();

    let sessions = discover_agy_sessions(Some(&db_path)).expect("must discover agy sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, "c-uuid-1");
    assert_eq!(sessions[0].title, "adding features in hq");
    assert_eq!(sessions[0].workspace, "harness-queue");
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_discover_codex_sessions_from_jsonl() {
    let tmp_dir = std::env::temp_dir().join(format!("hq_test_codex_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp_dir);
    let jsonl_path = tmp_dir.join("session_index.jsonl");

    let mut file = std::fs::File::create(&jsonl_path).unwrap();
    writeln!(file, r#"{{"id":"codex-1","thread_name":"debug issue","updated_at":"2026-11-07T11:30:00Z"}}"#).unwrap();

    let sessions = discover_codex_sessions(Some(&jsonl_path)).expect("must discover codex sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, "codex-1");
    assert_eq!(sessions[0].title, "debug issue");
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_missing_database_returns_empty_vector_not_error() {
    let missing_path = std::path::Path::new("/nonexistent/conversation_summaries.db");
    let sessions = discover_agy_sessions(Some(missing_path)).unwrap();
    assert!(sessions.is_empty());
}

#[test]
fn test_discover_sessions_for_harness() {
    let sessions = discover_sessions_for_harness(&HarnessKind::Agy);
    // Should not panic even if system has or has not active sessions
    let _ = sessions;
}

#[test]
fn test_corrupt_database_returns_empty_vector_not_panic() {
    let tmp_dir = std::env::temp_dir().join(format!("hq_test_corrupt_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp_dir);
    let db_path = tmp_dir.join("conversation_summaries.db");
    // Write garbage bytes
    std::fs::write(&db_path, b"not a valid sqlite file").unwrap();

    let res = discover_agy_sessions(Some(&db_path));
    // Either returns empty vector or clean error, must NOT panic
    assert!(res.is_err() || res.unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_discover_real_agy_sessions() {
    let sessions = discover_agy_sessions(None).unwrap();
    if !sessions.is_empty() {
        assert!(!sessions[0].session_id.is_empty());
        assert!(!sessions[0].formatted_time.is_empty());
    }
}


