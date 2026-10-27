use super::{AgentStatus, DeliveryAck, NeighborDetection, TerminalAdapter, TerminalError};
use crate::models::AgentTarget;
use crate::terminal::herdr::HerdrAdapter;
use crate::terminal::orca::is_same_or_subfolder;
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

pub fn find_ghostty_neighbor(
    caller_id: &str,
    caller_cwd: &std::path::Path,
    terminals: &[GhosttyTerminalEntry],
) -> NeighborDetection {
    // Strictly filter to:
    // 1. Not the caller itself
    // 2. In the EXACT SAME folder (working_directory matches caller_cwd)
    let candidates: Vec<&GhosttyTerminalEntry> = terminals
        .iter()
        .filter(|t| {
            if t.id == caller_id {
                return false;
            }
            if t.working_directory.is_empty() {
                return false;
            }
            is_same_or_subfolder(std::path::Path::new(&t.working_directory), caller_cwd)
        })
        .collect();

    if candidates.is_empty() {
        return NeighborDetection::NoneFound(format!(
            "No adjacent agent found in the current Ghostty split pane and folder ({}). Open a split pane next to your agent to attach.",
            caller_cwd.display()
        ));
    }

    if candidates.len() == 1 {
        let entry = candidates[0];
        let harness = HerdrAdapter::infer_harness(&entry.title);
        return NeighborDetection::Exact(AgentTarget {
            terminal_id: entry.id.clone(),
            pane_id: entry.id.clone(),
            pid: 0,
            harness,
            session_id: entry.id.clone(),
            cwd: entry.working_directory.clone(),
        });
    }

    let targets = candidates
        .into_iter()
        .map(|e| AgentTarget {
            terminal_id: e.id.clone(),
            pane_id: e.id.clone(),
            pid: 0,
            harness: HerdrAdapter::infer_harness(&e.title),
            session_id: e.id.clone(),
            cwd: e.working_directory.clone(),
        })
        .collect();

    NeighborDetection::Ambiguous(targets)
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
        if let Ok(prog) = std::env::var("TERM_PROGRAM") {
            if prog == "ghostty" {
                return true;
            }
        }
        if std::env::var("GHOSTTY_RESOURCES_DIR").is_ok() {
            return true;
        }
        false
    }

    fn detect_current_pane(&self) -> Result<String, TerminalError> {
        let script = r#"tell application "Ghostty"
    if (count of windows) > 0 then
        set w to front window
        set t to selected tab of w
        set f to focused terminal of t
        return (id of f as string)
    end if
    error "No active Ghostty window"
end tell"#;

        self.run_osascript(script)
    }

    fn find_adjacent_agent(&self, current_pane: &str) -> Result<NeighborDetection, TerminalError> {
        let terminals = self.list_terminals_in_front_tab()?;
        let caller_cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        Ok(find_ghostty_neighbor(current_pane, &caller_cwd, &terminals))
    }

    fn get_agent_status(&self, _pane_id: &str) -> Result<AgentStatus, TerminalError> {
        Ok(AgentStatus::Ready)
    }

    fn send_prompt(&self, target: &AgentTarget, prompt: &str) -> Result<DeliveryAck, TerminalError> {
        let script = build_ghostty_send_script(&target.terminal_id, prompt);
        let out = self.run_osascript(&script)?;
        if out.contains("ok") {
            Ok(DeliveryAck {
                acknowledged: true,
                details: format!("Delivered to Ghostty terminal {}", target.terminal_id),
            })
        } else {
            Err(TerminalError::CommandFailed("Ghostty did not acknowledge delivery".to_string()))
        }
    }

    fn verify_target_alive(&self, target: &AgentTarget) -> Result<bool, TerminalError> {
        let script = format!(
            r#"tell application "Ghostty"
    repeat with term in terminals
        if id of term is "{}" then
            return "true"
        end if
    end repeat
    return "false"
end tell"#,
            target.terminal_id
        );
        let res = self.run_osascript(&script)?;
        Ok(res.contains("true"))
    }
}
