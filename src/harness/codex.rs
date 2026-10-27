use super::{
    AgentReadiness, HarnessAdapter, HarnessCapabilities, HarnessError, QuotaStatus, SupportLevel,
};
use crate::models::{AgentTarget, HarnessKind};
use crate::terminal::{AgentStatus, DeliveryAck, TerminalAdapter};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

pub fn parse_rate_limits_payload(json_str: &str) -> Result<QuotaStatus, HarnessError> {
    let v: Value = serde_json::from_str(json_str)
        .map_err(|e| HarnessError::QuotaCheckFailed(e.to_string()))?;

    let root = v.get("result").unwrap_or(&v);
    let ordinary_opt = root.get("ordinaryUsageAllowed").and_then(|a| a.as_bool());
    let rate_limits_opt = root.get("rateLimits");

    if ordinary_opt.is_none() && rate_limits_opt.is_none() {
        return Ok(QuotaStatus::Unknown);
    }

    let ordinary_allowed = ordinary_opt.unwrap_or(true);
    let rate_limits = rate_limits_opt.unwrap_or(root);
    let reached_type = rate_limits.get("rateLimitReachedType").and_then(|t| t.as_str());
    let spend_control = rate_limits.get("spendControlReached").and_then(|s| s.as_bool()).unwrap_or(false);

    let primary_used = rate_limits.pointer("/primary/usedPercent").and_then(|u| u.as_f64()).unwrap_or(0.0);
    let resets_at = rate_limits.pointer("/primary/resetsAt").and_then(|r| r.as_u64());

    if !ordinary_allowed || reached_type.is_some() || spend_control || primary_used >= 100.0 {
        let reason = if let Some(rt) = reached_type {
            format!("Rate limit reached: {}", rt)
        } else if spend_control {
            "Spend control limit reached".to_string()
        } else if primary_used >= 100.0 {
            "Usage limit 100% reached".to_string()
        } else {
            "Usage quota currently exhausted".to_string()
        };

        return Ok(QuotaStatus::RateLimited { resets_at, reason });
    }

    Ok(QuotaStatus::Available)
}

pub struct CodexHarness {
    pub codex_bin: String,
    last_check: std::sync::Mutex<Option<(std::time::Instant, QuotaStatus)>>,
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

    pub fn fetch_rate_limits(&self) -> Result<QuotaStatus, HarnessError> {
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
        let mut status = QuotaStatus::Available;
        let mut found = false;

        for line_str in reader.lines().take(10).flatten() {
            if let Ok(val) = serde_json::from_str::<Value>(&line_str) {
                if val.get("id").and_then(|id| id.as_i64()) == Some(2) {
                    status = parse_rate_limits_payload(&line_str)?;
                    found = true;
                    break;
                }
            }
        }

        let _ = child.kill();
        let _ = child.wait();

        if found {
            Ok(status)
        } else {
            Ok(QuotaStatus::Unknown)
        }
    }
}

impl HarnessAdapter for CodexHarness {
    fn kind(&self) -> HarnessKind {
        HarnessKind::Codex
    }

    fn check_quota(&self, _session_id: &str) -> Result<QuotaStatus, HarnessError> {
        let mut lock = self.last_check.lock().unwrap();
        if let Some((instant, ref status)) = *lock {
            let ttl = match status {
                QuotaStatus::Available => std::time::Duration::from_secs(5),
                _ => std::time::Duration::from_secs(2),
            };
            if instant.elapsed() < ttl {
                return Ok(status.clone());
            }
        }

        let fresh = self.fetch_rate_limits()?;
        *lock = Some((std::time::Instant::now(), fresh.clone()));
        Ok(fresh)
    }

    fn is_ready(&self, target: &AgentTarget, terminal: &dyn TerminalAdapter) -> Result<AgentReadiness, HarnessError> {
        match terminal.get_agent_status(&target.pane_id) {
            Ok(AgentStatus::Ready) => Ok(AgentReadiness::Ready),
            Ok(AgentStatus::Busy) => Ok(AgentReadiness::Busy),
            Ok(AgentStatus::BlockedApproval) => {
                Ok(AgentReadiness::BlockedOnApproval("User approval prompt active".to_string()))
            }
            Ok(AgentStatus::RateLimited) => {
                // If the structured JSON-RPC API confirms quota is Available, do not let
                // historical scrollback text in the terminal pane block dispatch!
                if let Ok(QuotaStatus::Available) = self.check_quota(&target.session_id) {
                    Ok(AgentReadiness::Ready)
                } else {
                    Ok(AgentReadiness::Interrupted("Usage limit reached".to_string()))
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
}
