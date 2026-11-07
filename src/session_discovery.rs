use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredSession {
    pub session_id: String,
    pub title: String,
    pub workspace: String,
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
pub fn format_timestamp(ts_secs: u64) -> String {
    let (year, month, day, hour, min) = epoch_secs_to_datetime(ts_secs);
    let ampm = if hour < 12 { "am" } else { "pm" };
    let hour12 = match hour % 12 {
        0 => 12,
        h => h,
    };
    format!("{:02}/{:02}/{:04} {:02}:{:02}{}", month, day, year, hour12, min, ampm)
}

/// Convert civil epoch seconds to UTC (year, month, day, hour, min)
fn epoch_secs_to_datetime(ts: u64) -> (u32, u32, u32, u32, u32) {
    let sec_in_day = (ts % 86400) as u32;
    let hour = sec_in_day / 3600;
    let min = (sec_in_day % 3600) / 60;

    let mut days = (ts / 86400) as i64;
    // Shift epoch to 0000-03-01
    days += 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = (days - era * 146097) as u64; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // month in year [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // day of month [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // month [1, 12]
    let year = if m <= 2 { y + 1 } else { y };

    (year as u32, m as u32, d as u32, hour, min)
}

/// Parse ISO-8601 or SQLite datetime string into Unix timestamp (seconds)
pub fn parse_iso_or_sqlite_time(raw: &str) -> Option<u64> {
    let clean = raw.trim();
    if clean.is_empty() {
        return None;
    }

    // Expected format: YYYY-MM-DD[ T]HH:MM:SS
    // Examples: "2026-11-07 22:30:00+00:00", "2026-11-07T11:30:00Z"
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
