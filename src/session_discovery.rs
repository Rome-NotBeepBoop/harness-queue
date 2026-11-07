use crate::models::HarnessKind;
use rusqlite::OpenFlags;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredSession {
    pub session_id: String,
    pub title: String,
    pub workspace: String,
    pub workspace_path: String,
    pub updated_at: Option<u64>,
    pub formatted_time: String,
}

pub fn format_session_label(session: &DiscoveredSession) -> String {
    let title = if session.title.trim().is_empty() {
        "untitled"
    } else {
        session.title.trim()
    };
    let time_str = if session.formatted_time.trim().is_empty() {
        "unknown"
    } else {
        session.formatted_time.trim()
    };
    let workspace = if session.workspace.trim().is_empty() {
        "default"
    } else {
        session.workspace.trim()
    };

    format!("{} '{}' {}", workspace, title, time_str)
}

/// Convert seconds since UNIX epoch to MM/DD/YYYY hh:mma (e.g. 07/11/2026 11:30pm)
#[cfg(unix)]
fn get_local_datetime(ts_secs: u64) -> (u32, u32, u32, u32, u32) {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let t = ts_secs as libc::time_t;
    unsafe {
        libc::localtime_r(&t, &mut tm);
    }
    (
        (tm.tm_year + 1900) as u32,
        (tm.tm_mon + 1) as u32,
        tm.tm_mday as u32,
        tm.tm_hour as u32,
        tm.tm_min as u32,
    )
}

#[cfg(not(unix))]
fn get_local_datetime(ts_secs: u64) -> (u32, u32, u32, u32, u32) {
    epoch_secs_to_datetime(ts_secs)
}

pub fn format_timestamp(ts_secs: u64) -> String {
    let (year, month, day, hour, min) = get_local_datetime(ts_secs);
    let ampm = if hour < 12 { "am" } else { "pm" };
    let hour12 = match hour % 12 {
        0 => 12,
        h => h,
    };
    format!("{:02}/{:02}/{:04} {:02}:{:02}{}", day, month, year, hour12, min, ampm)
}

/// Convert civil epoch seconds to UTC (year, month, day, hour, min)
pub fn epoch_secs_to_datetime(ts: u64) -> (u32, u32, u32, u32, u32) {
    let sec_in_day = (ts % 86400) as u32;
    let hour = sec_in_day / 3600;
    let min = (sec_in_day % 3600) / 60;

    let mut days = (ts / 86400) as i64;
    days += 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = (days - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    (year as u32, m as u32, d as u32, hour, min)
}

/// Parse ISO-8601 or SQLite datetime string into Unix timestamp (seconds)
pub fn parse_iso_or_sqlite_time(raw: &str) -> Option<u64> {
    let clean = raw.trim();
    if clean.is_empty() {
        return None;
    }

    let parts: Vec<&str> = clean.split(|c| c == 'T' || c == ' ').collect();
    if parts.len() < 2 {
        return None;
    }

    let date_parts: Vec<&str> = parts[0].split('-').collect();
    if date_parts.len() != 3 {
        return None;
    }

    let year: u32 = date_parts[0].parse().ok()?;
    let month: u32 = date_parts[1].parse().ok()?;
    let day: u32 = date_parts[2].parse().ok()?;

    let time_clean = parts[1].split(|c| c == '+' || c == 'Z' || c == '.').next()?;
    let time_parts: Vec<&str> = time_clean.split(':').collect();
    if time_parts.len() < 2 {
        return None;
    }

    let hour: u32 = time_parts[0].parse().ok()?;
    let min: u32 = time_parts[1].parse().ok()?;
    let sec: u32 = if time_parts.len() >= 3 {
        time_parts[2].parse().ok().unwrap_or(0)
    } else {
        0
    };

    datetime_to_epoch_secs(year, month, day, hour, min, sec)
}

fn datetime_to_epoch_secs(year: u32, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> Option<u64> {
    if month == 0 || month > 12 || day == 0 || day > 31 || hour >= 24 || min >= 60 || sec >= 60 {
        return None;
    }

    let y = if month <= 2 { year as i64 - 1 } else { year as i64 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let m = if month <= 2 { month + 9 } else { month - 3 };
    let doy = (153 * m as u64 + 2) / 5 + day as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe as i64 - 719468;

    if days < 0 {
        return None;
    }

    let secs = (days as u64) * 86400 + (hour as u64) * 3600 + (min as u64) * 60 + (sec as u64);
    Some(secs)
}

pub fn resolve_home_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return PathBuf::from(home);
        }
    }
    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        if !user_profile.trim().is_empty() {
            return PathBuf::from(user_profile);
        }
    }
    PathBuf::from(".")
}

fn extract_workspace_info(uris_json: &str) -> (String, String) {
    let default_cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());
    let default_name = std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| "workspace".to_string());

    if let Ok(val) = serde_json::from_str::<Value>(uris_json) {
        let uri_opt = if let Some(arr) = val.as_array() {
            arr.first().and_then(|v| v.as_str())
        } else {
            val.as_str()
        };

        if let Some(first) = uri_opt {
            let clean = first.trim_start_matches("file://");
            let p = Path::new(clean);
            let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
            let abs_path = clean.to_string();
            if !fname.is_empty() {
                return (fname, abs_path);
            }
        }
    }

    (default_name, default_cwd)
}

/// Discover sessions for Google Antigravity (AGY) from conversation_summaries.db
pub fn discover_agy_sessions(custom_db_path: Option<&Path>) -> Result<Vec<DiscoveredSession>, String> {
    let candidate_paths = if let Some(p) = custom_db_path {
        vec![p.to_path_buf()]
    } else {
        let home = resolve_home_dir();
        let mut paths = Vec::new();
        if let Ok(env_dir) = std::env::var("GEMINI_DIR") {
            paths.push(PathBuf::from(env_dir).join("conversation_summaries.db"));
        }
        if let Ok(env_dir) = std::env::var("ANTIGRAVITY_DATA_DIR") {
            paths.push(PathBuf::from(env_dir).join("conversation_summaries.db"));
        }
        // Active CLI directory first
        paths.push(home.join(".gemini").join("antigravity-cli").join("conversation_summaries.db"));
        // Standalone / GUI directory second
        paths.push(home.join(".gemini").join("antigravity").join("conversation_summaries.db"));
        paths
    };

    let mut session_map: std::collections::HashMap<String, DiscoveredSession> =
        std::collections::HashMap::new();

    for db_path in &candidate_paths {
        if !db_path.exists() {
            continue;
        }

        let conn = match rusqlite::Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Try rich query with preview and nesting_depth filter (to exclude internal subagents)
        let query_res = conn.prepare(
            "SELECT conversation_id, title, preview, workspace_uris, last_modified_time
             FROM conversation_summaries
             WHERE nesting_depth = 0 OR nesting_depth IS NULL
             ORDER BY last_modified_time DESC
             LIMIT 30",
        );

        let rows: Vec<(String, String, String, String)> = match query_res {
            Ok(mut stmt) => {
                let mapped = stmt.query_map([], |row| {
                    let id: String = row.get(0)?;
                    let title: String = row.get(1).unwrap_or_default();
                    let preview: String = row.get(2).unwrap_or_default();
                    let uris: String = row.get(3).unwrap_or_default();
                    let raw_time: String = row.get(4).unwrap_or_default();
                    let effective_title = if !title.trim().is_empty() {
                        title
                    } else if !preview.trim().is_empty() {
                        preview.lines().next().unwrap_or("").chars().take(40).collect()
                    } else {
                        "".to_string()
                    };
                    Ok((id, effective_title, uris, raw_time))
                });
                mapped
                    .map(|m| m.into_iter().flatten().collect())
                    .unwrap_or_default()
            }
            Err(_) => {
                // Fallback for minimal schemas without preview / nesting_depth
                let fallback = conn.prepare(
                    "SELECT conversation_id, title, workspace_uris, last_modified_time
                     FROM conversation_summaries
                     ORDER BY last_modified_time DESC
                     LIMIT 30",
                );
                match fallback {
                    Ok(mut stmt) => {
                        let mapped = stmt.query_map([], |row| {
                            let id: String = row.get(0)?;
                            let title: String = row.get(1).unwrap_or_default();
                            let uris: String = row.get(2).unwrap_or_default();
                            let raw_time: String = row.get(3).unwrap_or_default();
                            Ok((id, title, uris, raw_time))
                        });
                        mapped
                            .map(|m| m.into_iter().flatten().collect())
                            .unwrap_or_default()
                    }
                    Err(_) => Vec::new(),
                }
            }
        };

        for (id, title, uris, raw_time) in rows {
            let ts = parse_iso_or_sqlite_time(&raw_time);
            let formatted_time = ts.map(format_timestamp).unwrap_or_else(|| "unknown".to_string());
            let (workspace, workspace_path) = extract_workspace_info(&uris);

            let session = DiscoveredSession {
                session_id: id.clone(),
                title,
                workspace,
                workspace_path,
                updated_at: ts,
                formatted_time,
            };

            if let Some(existing) = session_map.get_mut(&id) {
                if session.updated_at > existing.updated_at {
                    *existing = session;
                }
            } else {
                session_map.insert(id, session);
            }
        }
    }

    let mut sessions: Vec<DiscoveredSession> = session_map.into_values().collect();
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    sessions.truncate(20);

    Ok(sessions)
}

/// Discover sessions for OpenAI Codex from session_index.jsonl
pub fn discover_codex_sessions(custom_jsonl_path: Option<&Path>) -> Result<Vec<DiscoveredSession>, String> {
    let jsonl_path = match custom_jsonl_path {
        Some(p) => p.to_path_buf(),
        None => resolve_home_dir()
            .join(".codex")
            .join("session_index.jsonl"),
    };

    if !jsonl_path.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(&jsonl_path).map_err(|e| format!("Failed to open Codex session_index.jsonl: {}", e))?;
    let reader = BufReader::new(file);

    let default_cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());
    let default_workspace = std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| "workspace".to_string());

    let mut sessions = Vec::new();
    for line_res in reader.lines() {
        if let Ok(line) = line_res {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
                let id = val.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if id.is_empty() {
                    continue;
                }
                let title = val
                    .get("thread_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let updated_raw = val
                    .get("updated_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let ts = parse_iso_or_sqlite_time(updated_raw);
                let formatted_time = ts.map(format_timestamp).unwrap_or_else(|| "unknown".to_string());

                sessions.push(DiscoveredSession {
                    session_id: id,
                    title,
                    workspace: default_workspace.clone(),
                    workspace_path: default_cwd.clone(),
                    updated_at: ts,
                    formatted_time,
                });
            }
        }
    }

    // Sort newest first if updated_at is known
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    sessions.truncate(20);

    Ok(sessions)
}

/// Discover recent sessions for any given harness kind
pub fn discover_sessions_for_harness(kind: &HarnessKind) -> Vec<DiscoveredSession> {
    match kind {
        HarnessKind::Agy => discover_agy_sessions(None).unwrap_or_default(),
        HarnessKind::Codex => discover_codex_sessions(None).unwrap_or_default(),
        _ => Vec::new(),
    }
}
