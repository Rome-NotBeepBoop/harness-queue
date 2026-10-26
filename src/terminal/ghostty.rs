use super::{AgentStatus, DeliveryAck, NeighborDetection, TerminalAdapter, TerminalError};
use crate::models::AgentTarget;
use crate::terminal::herdr::HerdrAdapter;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhosttyTerminalEntry {
    pub id: String,
    pub working_directory: String,
    pub title: String,
}

pub fn parse_ghostty_terminals_output(output: &str) -> Vec<GhosttyTerminalEntry> {
    let mut entries = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            entries.push(GhosttyTerminalEntry {
                id: parts[0].trim().to_string(),
                working_directory: parts[1].trim().to_string(),
                title: parts[2].trim().to_string(),
            });
        }
    }
    entries
}

pub fn build_ghostty_send_script(terminal_id: &str, prompt: &str) -> String {
    let escaped_prompt = prompt
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "");

    format!(
        r#"tell application "Ghostty"
    repeat with term in terminals
        if id of term is "{}" then
            input text "{}" to term
            send key "enter" to term
            return "ok"
        end if
    end repeat
    error "Terminal not found: {}"
end tell"#,
        terminal_id, escaped_prompt, terminal_id
    )
}

pub fn classify_osascript_error(error_msg: &str) -> TerminalError {
    if error_msg.contains("-1743") || error_msg.contains("Not authorized to send Apple events") {
        TerminalError::PermissionDenied(
            "Missing macOS Automation permission for Ghostty. Enable Terminal/Ghostty under System Settings -> Privacy & Security -> Automation."
                .to_string(),
        )
    } else {
        TerminalError::CommandFailed(error_msg.to_string())
    }
}

pub struct GhosttyAdapter;

impl Default for GhosttyAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GhosttyAdapter {
    pub fn new() -> Self {
        Self
    }

    fn run_osascript(&self, script: &str) -> Result<String, TerminalError> {
        let output = Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|e| TerminalError::CommandFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(classify_osascript_error(&stderr));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn list_terminals_in_front_tab(&self) -> Result<Vec<GhosttyTerminalEntry>, TerminalError> {
        let script = r#"tell application "Ghostty"
    if (count of windows) > 0 then
        set w to front window
        set t to selected tab of w
        set terms to terminals of t
        set res to ""
        repeat with term in terms
            set res to res & (id of term) & tab & (working directory of term) & tab & (name of term) & linefeed
        end repeat
        return res
    else
        return ""
    end if
end tell"#;

        let out = self.run_osascript(script)?;
        Ok(parse_ghostty_terminals_output(&out))
    }
}

impl TerminalAdapter for GhosttyAdapter {
    fn terminal_name(&self) -> &'static str {
        "Ghostty"
    }

    fn is_available(&self) -> bool {
        if std::env::var("GHOSTTY_RESOURCES_DIR").is_ok() {
            return true;
        }

        // Check if Ghostty is running and responds to AppleScript
        Command::new("osascript")
            .args(["-e", "tell application \"Ghostty\" to get version"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn detect_current_pane(&self) -> Result<String, TerminalError> {
        // Current working directory can identify the caller terminal
        let current_dir = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let terminals = self.list_terminals_in_front_tab()?;
        for term in &terminals {
            if term.working_directory == current_dir {
                return Ok(term.id.clone());
            }
        }

        if let Some(first) = terminals.first() {
            Ok(first.id.clone())
        } else {
            Err(TerminalError::NotDetected)
        }
    }

    fn find_adjacent_agent(&self, current_pane: &str) -> Result<NeighborDetection, TerminalError> {
        let terminals = self.list_terminals_in_front_tab()?;
        let neighbors: Vec<&GhosttyTerminalEntry> =
            terminals.iter().filter(|t| t.id != current_pane).collect();

        if neighbors.is_empty() {
            return Ok(NeighborDetection::NoneFound(
                "No adjacent terminal found in active Ghostty tab".to_string(),
            ));
        }

        if neighbors.len() == 1 {
            let n = neighbors[0];
            let harness = HerdrAdapter::infer_harness(&n.title);
            let target = AgentTarget {
                terminal_id: n.id.clone(),
                pane_id: n.id.clone(),
                pid: 0,
                harness,
                session_id: n.id.clone(),
                cwd: n.working_directory.clone(),
            };
            Ok(NeighborDetection::Exact(target))
        } else {
            let targets: Vec<AgentTarget> = neighbors
                .iter()
                .map(|n| AgentTarget {
                    terminal_id: n.id.clone(),
                    pane_id: n.id.clone(),
                    pid: 0,
                    harness: HerdrAdapter::infer_harness(&n.title),
                    session_id: n.id.clone(),
                    cwd: n.working_directory.clone(),
                })
                .collect();
            Ok(NeighborDetection::Ambiguous(targets))
        }
    }

    fn get_agent_status(&self, _pane_id: &str) -> Result<AgentStatus, TerminalError> {
        // Ghostty AppleScript exposes surface properties, assume ready if terminal exists
        Ok(AgentStatus::Ready)
    }

    fn send_prompt(&self, target: &AgentTarget, prompt: &str) -> Result<DeliveryAck, TerminalError> {
        let script = build_ghostty_send_script(&target.terminal_id, prompt);
        self.run_osascript(&script)?;
        Ok(DeliveryAck {
            acknowledged: true,
            details: "Delivered to Ghostty terminal via AppleScript".to_string(),
        })
    }

    fn verify_target_alive(&self, target: &AgentTarget) -> Result<bool, TerminalError> {
        let script = format!(
            r#"tell application "Ghostty"
    repeat with term in terminals
        if id of term is "{}" then
            return "alive"
        end if
    end repeat
    return "dead"
end tell"#,
            target.terminal_id
        );

        let out = self.run_osascript(&script)?;
        Ok(out.trim() == "alive")
    }
}
