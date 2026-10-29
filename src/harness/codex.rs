use super::{
    AgentReadiness, HarnessAdapter, HarnessCapabilities, HarnessError, QuotaStatus, SupportLevel,
};
use crate::models::{AgentTarget, HarnessKind};
use crate::terminal::{AgentStatus, DeliveryAck, TerminalAdapter};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct CodexUsageDetails {
    pub used_percent: f64,
    pub resets_at: Option<u64>,
    pub window_mins: Option<u64>,
    pub status: QuotaStatus,
}

pub fn parse_rate_limits_details(json_str: &str) -> Result<CodexUsageDetails, HarnessError> {
    let v: Value = serde_json::from_str(json_str)
        .map_err(|e| HarnessError::QuotaCheckFailed(e.to_string()))?;

    let root = v.get("result").unwrap_or(&v);
    let ordinary_opt = root.get("ordinaryUsageAllowed").and_then(|a| a.as_bool());
    let rate_limits_opt = root.get("rateLimits");

    if ordinary_opt.is_none() && rate_limits_opt.is_none() {
        return Ok(CodexUsageDetails {
            used_percent: 0.0,
            resets_at: None,
            window_mins: None,
            status: QuotaStatus::Unknown,
        });
    }

    let ordinary_allowed = ordinary_opt.unwrap_or(true);
    let rate_limits = rate_limits_opt.unwrap_or(root);
    let reached_type = rate_limits
        .get("rateLimitReachedType")
        .and_then(|t| t.as_str());
    let spend_control = rate_limits
        .get("spendControlReached")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);

    let primary_used = rate_limits
        .pointer("/primary/usedPercent")
        .and_then(|u| u.as_f64())
        .unwrap_or(0.0);
    let resets_at = rate_limits
        .pointer("/primary/resetsAt")
        .and_then(|r| r.as_u64());
    let window_mins = rate_limits
        .pointer("/primary/windowDurationMins")
        .and_then(|w| w.as_u64());

    let status = if !ordinary_allowed || reached_type.is_some() || spend_control || primary_used >= 100.0 {
        let reason = if let Some(rt) = reached_type {
            format!("Rate limit reached: {}", rt)
        } else if spend_control {
            "Spend control limit reached".to_string()
        } else if primary_used >= 100.0 {
            "Usage limit 100% reached".to_string()
        } else {
            "Usage quota currently exhausted".to_string()
        };

        QuotaStatus::RateLimited { resets_at, reason }
    } else {
        QuotaStatus::Available
    };

    Ok(CodexUsageDetails {
        used_percent: primary_used,
        resets_at,
        window_mins,
        status,
    })
}

pub fn parse_rate_limits_payload(json_str: &str) -> Result<QuotaStatus, HarnessError> {
    parse_rate_limits_details(json_str).map(|d| d.status)
}

pub struct CodexHarness {
    pub codex_bin: String,
    last_check: std::sync::Mutex<Option<(std::time::Instant, CodexUsageDetails)>>,
}

impl Default for CodexHarness {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexHarness {
    pub fn new() -> Self {
        let bin = std::env::var("CODEX_BIN").unwrap_or_else(|_| {
            if std::path::Path::new("/opt/homebrew/bin/codex").exists() {
                "/opt/homebrew/bin/codex".to_string()
            } else if std::path::Path::new("/usr/local/bin/codex").exists() {
                "/usr/local/bin/codex".to_string()
            } else {
                "codex".to_string()
            }
        });
        Self {
            codex_bin: bin,
            last_check: std::sync::Mutex::new(None),
        }
    }

    pub fn fetch_rate_limits_details(&self) -> Result<CodexUsageDetails, HarnessError> {
        let mut child = Command::new(&self.codex_bin)
            .args(["app-server", "--listen", "stdio://"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| HarnessError::QuotaCheckFailed(e.to_string()))?;

        let mut stdin = child.stdin.take().ok_or_else(|| {
            HarnessError::QuotaCheckFailed("Failed to open app-server stdin".to_string())
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            HarnessError::QuotaCheckFailed("Failed to open app-server stdout".to_string())
        })?;

        let init_req = r#"{"id":1,"method":"initialize","params":{"clientInfo":{"name":"hq","version":"0.1.0"}}}"#;
        let initialized_notif = r#"{"method":"initialized"}"#;
        let quota_req = r#"{"id":2,"method":"account/rateLimits/read"}"#;

        let _ = writeln!(stdin, "{}", init_req);
        let _ = writeln!(stdin, "{}", initialized_notif);
        let _ = writeln!(stdin, "{}", quota_req);
        let _ = stdin.flush();

        let reader = BufReader::new(stdout);
        let mut details = CodexUsageDetails {
            used_percent: 0.0,
            resets_at: None,
            window_mins: None,
            status: QuotaStatus::Available,
        };
        let mut found = false;

        for line_str in reader.lines().take(10).flatten() {
            if let Ok(val) = serde_json::from_str::<Value>(&line_str) {
                if val.get("id").and_then(|id| id.as_i64()) == Some(2) {
                    details = parse_rate_limits_details(&line_str)?;
                    found = true;
                    break;
                }
            }
        }

        let _ = child.kill();
        let _ = child.wait();

        if found {
            Ok(details)
        } else {
            Ok(CodexUsageDetails {
                used_percent: 0.0,
                resets_at: None,
                window_mins: None,
                status: QuotaStatus::Unknown,
            })
        }
    }

    pub fn fetch_rate_limits(&self) -> Result<QuotaStatus, HarnessError> {
        self.fetch_rate_limits_details().map(|d| d.status)
    }

    pub fn get_cached_or_fresh_details(&self) -> Option<CodexUsageDetails> {
        let mut lock = self.last_check.lock().unwrap();
        if let Some((instant, ref details)) = *lock {
            if instant.elapsed() < std::time::Duration::from_secs(5) {
                return Some(details.clone());
            }
        }

        if let Ok(fresh) = self.fetch_rate_limits_details() {
            *lock = Some((std::time::Instant::now(), fresh.clone()));
            Some(fresh)
        } else {
            lock.as_ref().map(|(_, d)| d.clone())
        }
    }
}

impl HarnessAdapter for CodexHarness {
    fn kind(&self) -> HarnessKind {
        HarnessKind::Codex
    }

    fn check_quota(&self, _session_id: &str) -> Result<QuotaStatus, HarnessError> {
        let mut lock = self.last_check.lock().unwrap();
        if let Some((instant, ref details)) = *lock {
            let ttl = match details.status {
                QuotaStatus::Available => std::time::Duration::from_secs(5),
                _ => std::time::Duration::from_secs(2),
            };
            if instant.elapsed() < ttl {
                return Ok(details.status.clone());
            }
        }

        let fresh = self.fetch_rate_limits_details()?;
        let status = fresh.status.clone();
        *lock = Some((std::time::Instant::now(), fresh));
        Ok(status)
    }

    fn is_ready(
        &self,
        target: &AgentTarget,
        terminal: &dyn TerminalAdapter,
    ) -> Result<AgentReadiness, HarnessError> {
        match terminal.get_agent_status(&target.pane_id) {
            Ok(AgentStatus::Ready) => Ok(AgentReadiness::Ready),
            Ok(AgentStatus::Busy) => Ok(AgentReadiness::Busy),
            Ok(AgentStatus::BlockedApproval) => Ok(AgentReadiness::BlockedOnApproval(
                "User approval prompt active".to_string(),
            )),
            Ok(AgentStatus::RateLimited) => {
                // If the structured JSON-RPC API confirms quota is Available, do not let
                // historical scrollback text in the terminal pane block dispatch!
                if let Ok(QuotaStatus::Available) = self.check_quota(&target.session_id) {
                    Ok(AgentReadiness::Ready)
                } else {
                    Ok(AgentReadiness::Interrupted(
                        "Usage limit reached".to_string(),
                    ))
                }
            }
            _ => Ok(AgentReadiness::Ready),
        }
    }

    fn submit_prompt(
        &self,
        target: &AgentTarget,
        prompt: &str,
        terminal: &dyn TerminalAdapter,
    ) -> Result<DeliveryAck, HarnessError> {
        // First try terminal level delivery
        match terminal.send_prompt(target, prompt) {
            Ok(ack) => Ok(ack),
            Err(e) => {
                // If terminal adapter fails, attempt codex queue fallback
                let output = Command::new(&self.codex_bin)
                    .args(["queue", "--thread", &target.session_id, "--message", prompt])
                    .output();

                if let Ok(out) = output {
                    if out.status.success() {
                        return Ok(DeliveryAck {
                            acknowledged: true,
                            details: "Delivered via codex queue CLI".to_string(),
                        });
                    }
                }

                Err(HarnessError::ExecutionFailed(e.to_string()))
            }
        }
    }

    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities {
            can_track_session: true,
            can_check_quota: true,
            can_auto_resume: true,
            level: SupportLevel::Full,
        }
    }

    fn get_usage_summary(
        &self,
        target: &AgentTarget,
        terminal: &dyn TerminalAdapter,
    ) -> Option<String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        if let Some(details) = self.get_cached_or_fresh_details() {
            let window_label = match details.window_mins {
                Some(300) => "5h window",
                Some(mins) if mins >= 60 => "5h window",
                _ => "5h window",
            };

            match details.status {
                QuotaStatus::RateLimited { resets_at, .. } => {
                    let reset_str = if let Some(ts) = resets_at {
                        if ts > now {
                            let diff = ts - now;
                            format!(" · resets in {}h {}m", diff / 3600, (diff % 3600) / 60)
                        } else {
                            " · reset due".to_string()
                        }
                    } else {
                        "".to_string()
                    };
                    return Some(format!(
                        "Codex Quota: 100% used ({}{})",
                        window_label, reset_str
                    ));
                }
                QuotaStatus::Available => {
                    let reset_str = if let Some(ts) = details.resets_at {
                        if ts > now {
                            let diff = ts - now;
                            format!(" · resets in {}h {}m", diff / 3600, (diff % 3600) / 60)
                        } else {
                            "".to_string()
                        }
                    } else {
                        "".to_string()
                    };
                    return Some(format!(
                        "Codex Quota: {:.1}% used ({}{})",
                        details.used_percent, window_label, reset_str
                    ));
                }
                QuotaStatus::Unknown => {}
            }
        }

        match terminal.get_agent_status(&target.pane_id) {
            Ok(AgentStatus::RateLimited) => {
                Some("Codex Quota: Rate limited · Waiting for recovery".to_string())
            }
            Ok(AgentStatus::Ready) => Some("Codex Quota: Active · Ready".to_string()),
            Ok(AgentStatus::Busy) => Some("Codex Quota: Busy · Active turn".to_string()),
            _ => Some("Codex Quota: Connected · Available".to_string()),
        }
    }
}
