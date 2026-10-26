use super::{AgentStatus, DeliveryAck, NeighborDetection, TerminalAdapter, TerminalError};
use crate::models::{AgentTarget, HarnessKind};
use serde_json::Value;
use std::process::Command;

pub fn parse_neighbor_result(json_str: &str) -> Result<Option<String>, TerminalError> {
    let v: Value = serde_json::from_str(json_str)
        .map_err(|e| TerminalError::JsonParseError(e.to_string()))?;
    if let Some(neighbor) = v.pointer("/result/neighbor/neighbor_pane_id") {
        if let Some(s) = neighbor.as_str() {
            return Ok(Some(s.to_string()));
        }
    }
    Ok(None)
}

pub struct HerdrAdapter {
    herdr_bin: String,
}

impl Default for HerdrAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl HerdrAdapter {
    pub fn new() -> Self {
        Self {
            herdr_bin: "herdr".to_string(),
        }
    }

    fn run_herdr(&self, args: &[&str]) -> Result<String, TerminalError> {
        let output = Command::new(&self.herdr_bin)
            .args(args)
            .output()
            .map_err(|e| TerminalError::CommandFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TerminalError::CommandFailed(stderr.to_string()));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn detect_agent_in_pane(&self, pane_id: &str) -> Result<Option<AgentTarget>, TerminalError> {
        // Read process info
        let proc_json = self.run_herdr(&["pane", "process-info", "--pane", pane_id])?;
        let v: Value = serde_json::from_str(&proc_json)
            .map_err(|e| TerminalError::JsonParseError(e.to_string()))?;

        let shell_pid = v.pointer("/result/process_info/shell_pid")
            .and_then(|p| p.as_u64())
            .unwrap_or(0) as u32;

        let foreground = v.pointer("/result/process_info/foreground_processes")
            .and_then(|fp| fp.as_array());

        let mut target_pid = shell_pid;
        let mut cmdline = String::new();
        let mut cwd = String::new();

        if let Some(procs) = foreground {
            if let Some(first) = procs.first() {
                if let Some(p) = first.get("pid").and_then(|p| p.as_u64()) {
                    target_pid = p as u32;
                }
                if let Some(cmd) = first.get("cmdline").and_then(|c| c.as_str()) {
                    cmdline = cmd.to_string();
                }
                if let Some(c) = first.get("cwd").and_then(|c| c.as_str()) {
                    cwd = c.to_string();
                }
            }
        }

        // Infer harness kind from cmdline or process name
        let harness = Self::infer_harness(&cmdline);

        // Read pane info for terminal id
        let pane_json = self.run_herdr(&["pane", "get", pane_id])?;
        let pv: Value = serde_json::from_str(&pane_json)
            .map_err(|e| TerminalError::JsonParseError(e.to_string()))?;

        let terminal_id = pv.pointer("/result/pane/terminal_id")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        if cwd.is_empty() {
            if let Some(c) = pv.pointer("/result/pane/cwd").and_then(|c| c.as_str()) {
                cwd = c.to_string();
            }
        }

        // Discover session id (e.g. from herdr agent or codex)
        let session_id = format!("{:x}", target_pid);

        Ok(Some(AgentTarget {
            terminal_id,
            pane_id: pane_id.to_string(),
            pid: target_pid,
            harness,
            session_id,
            cwd,
        }))
    }

    pub fn infer_harness(cmdline: &str) -> HarnessKind {
        let lower = cmdline.to_lowercase();
        if lower.contains("codex") {
            HarnessKind::Codex
        } else if lower.contains("claude") {
            HarnessKind::ClaudeCode
        } else if lower.contains("opencode") {
            HarnessKind::OpenCode
        } else if lower.contains("omp") {
            HarnessKind::Omp
        } else if lower.contains("pi") {
            HarnessKind::Pi
        } else if lower.contains("agy") || lower.contains("antigravity") {
            HarnessKind::Agy
        } else {
            HarnessKind::Unknown(cmdline.to_string())
        }
    }
}

impl TerminalAdapter for HerdrAdapter {
    fn terminal_name(&self) -> &'static str {
        "Herdr"
    }

    fn is_available(&self) -> bool {
        std::env::var("HERDR_ENV").map(|v| v == "1").unwrap_or(false)
            || Command::new(&self.herdr_bin).arg("status").output().map(|o| o.status.success()).unwrap_or(false)
    }

    fn detect_current_pane(&self) -> Result<String, TerminalError> {
        if let Ok(pane) = std::env::var("HERDR_PANE_ID") {
            if !pane.is_empty() {
                return Ok(pane);
            }
        }

        let out = self.run_herdr(&["pane", "current"])?;
        let v: Value = serde_json::from_str(&out)
            .map_err(|e| TerminalError::JsonParseError(e.to_string()))?;
        if let Some(id) = v.pointer("/result/pane/pane_id").and_then(|p| p.as_str()) {
            Ok(id.to_string())
        } else {
            Err(TerminalError::NotDetected)
        }
    }

    fn find_adjacent_agent(&self, current_pane: &str) -> Result<NeighborDetection, TerminalError> {
        // Directions to check in order: right, left, down, up
        let directions = ["right", "left", "down", "up"];
        let mut found_neighbors = Vec::new();

        for dir in directions {
            if let Ok(out) = self.run_herdr(&["pane", "neighbor", "--pane", current_pane, "--direction", dir]) {
                if let Ok(Some(neighbor_id)) = parse_neighbor_result(&out) {
                    if !found_neighbors.contains(&neighbor_id) {
                        found_neighbors.push(neighbor_id);
                    }
                }
            }
        }

        if found_neighbors.is_empty() {
            return Ok(NeighborDetection::NoneFound(
                "No adjacent pane found in current Herdr tab".to_string(),
            ));
        }

        let mut targets = Vec::new();
        for n_id in found_neighbors {
            if let Ok(Some(target)) = self.detect_agent_in_pane(&n_id) {
                targets.push(target);
            }
        }

        if targets.len() == 1 {
            Ok(NeighborDetection::Exact(targets.remove(0)))
        } else if targets.len() > 1 {
            Ok(NeighborDetection::Ambiguous(targets))
        } else {
            Ok(NeighborDetection::NoneFound(
                "Adjacent pane does not contain a recognized agent".to_string(),
            ))
        }
    }

    fn get_agent_status(&self, pane_id: &str) -> Result<AgentStatus, TerminalError> {
        match self.run_herdr(&["agent", "get", pane_id]) {
            Ok(out) => {
                let v: Value = serde_json::from_str(&out)
                    .map_err(|e| TerminalError::JsonParseError(e.to_string()))?;
                let status_str = v.pointer("/result/agent/status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown");
                match status_str {
                    "idle" | "done" => Ok(AgentStatus::Ready),
                    "working" => Ok(AgentStatus::Busy),
                    "blocked" => Ok(AgentStatus::BlockedApproval),
                    _ => Ok(AgentStatus::Unknown),
                }
            }
            Err(_) => {
                // Agent not registered in herdr agent tracking; check process info
                let proc_out = self.run_herdr(&["pane", "process-info", "--pane", pane_id])?;
                let v: Value = serde_json::from_str(&proc_out)
                    .map_err(|e| TerminalError::JsonParseError(e.to_string()))?;
                let foreground = v.pointer("/result/process_info/foreground_processes")
                    .and_then(|fp| fp.as_array());
                if let Some(fp) = foreground {
                    if !fp.is_empty() {
                        Ok(AgentStatus::Ready)
                    } else {
                        Ok(AgentStatus::Unknown)
                    }
                } else {
                    Ok(AgentStatus::Unknown)
                }
            }
        }
    }

    fn send_prompt(&self, target: &AgentTarget, prompt: &str) -> Result<DeliveryAck, TerminalError> {
        // Try herdr agent prompt first
        let res = self.run_herdr(&["agent", "prompt", &target.pane_id, prompt]);
        if let Ok(out) = res {
            return Ok(DeliveryAck {
                acknowledged: true,
                details: format!("Agent prompt sent: {}", out.trim()),
            });
        }

        // Fallback to pane send-text + Enter
        let text_with_nl = format!("{}\n", prompt);
        self.run_herdr(&["pane", "send-text", &target.pane_id, &text_with_nl])?;

        Ok(DeliveryAck {
            acknowledged: true,
            details: "Sent text to pane directly".to_string(),
        })
    }

    fn verify_target_alive(&self, target: &AgentTarget) -> Result<bool, TerminalError> {
        let proc_json = self.run_herdr(&["pane", "process-info", "--pane", &target.pane_id])?;
        let v: Value = serde_json::from_str(&proc_json)
            .map_err(|e| TerminalError::JsonParseError(e.to_string()))?;

        let shell_pid = v.pointer("/result/process_info/shell_pid")
            .and_then(|p| p.as_u64())
            .unwrap_or(0) as u32;

        let foreground = v.pointer("/result/process_info/foreground_processes")
            .and_then(|fp| fp.as_array());

        let mut current_pid = shell_pid;
        if let Some(procs) = foreground {
            if let Some(first) = procs.first() {
                if let Some(p) = first.get("pid").and_then(|p| p.as_u64()) {
                    current_pid = p as u32;
                }
            }
        }

        Ok(current_pid == target.pid)
    }
}
