use super::{AgentStatus, DeliveryAck, NeighborDetection, TerminalAdapter, TerminalError};
use crate::models::{AgentTarget, HarnessKind};
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

pub struct FallbackAdapter {
    pub target: AgentTarget,
    pub clipboard_enabled: bool,
    pub active_child: Arc<Mutex<Option<Child>>>,
}

impl FallbackAdapter {
    pub fn new(target: AgentTarget, clipboard_enabled: bool) -> Self {
        Self {
            target,
            clipboard_enabled,
            active_child: Arc::new(Mutex::new(None)),
        }
    }

    fn copy_to_clipboard(&self, text: &str) {
        #[cfg(target_os = "macos")]
        {
            if let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                let _ = child.wait();
            }
        }

        #[cfg(target_os = "windows")]
        {
            if let Ok(mut child) = Command::new("clip").stdin(Stdio::piped()).spawn() {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                let _ = child.wait();
            }
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            // Linux fallback (xclip / wl-copy)
            if let Ok(mut child) = Command::new("wl-copy").stdin(Stdio::piped()).spawn() {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                let _ = child.wait();
            } else if let Ok(mut child) = Command::new("xclip").args(["-selection", "clipboard"]).stdin(Stdio::piped()).spawn() {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                let _ = child.wait();
            }
        }
    }
}

impl TerminalAdapter for FallbackAdapter {
    fn terminal_name(&self) -> &'static str {
        if std::env::var("WT_SESSION").is_ok() || cfg!(target_os = "windows") {
            "Windows Terminal (Fallback)"
        } else {
            "Terminal (Fallback)"
        }
    }

    fn is_available(&self) -> bool {
        true
    }

    fn detect_current_pane(&self) -> Result<String, TerminalError> {
        Ok("standalone_pane".to_string())
    }

    fn find_adjacent_agent(&self, _current_pane: &str) -> Result<NeighborDetection, TerminalError> {
        Ok(NeighborDetection::Exact(self.target.clone()))
    }

    fn get_agent_status(&self, _pane_id: &str) -> Result<AgentStatus, TerminalError> {
        let mut lock = self.active_child.lock().unwrap();
        if let Some(child) = lock.as_mut() {
            match child.try_wait() {
                Ok(Some(_status)) => {
                    // Process finished its turn
                    *lock = None;
                    Ok(AgentStatus::Ready)
                }
                Ok(None) => {
                    // Turn is actively running in background
                    Ok(AgentStatus::Busy)
                }
                Err(_) => {
                    *lock = None;
                    Ok(AgentStatus::Ready)
                }
            }
        } else {
            Ok(AgentStatus::Ready)
        }
    }

    fn send_prompt(
        &self,
        target: &AgentTarget,
        prompt: &str,
    ) -> Result<DeliveryAck, TerminalError> {
        if self.clipboard_enabled {
            self.copy_to_clipboard(prompt);
        }

        if std::env::var("MOCK_SPAWN").map(|v| v == "1").unwrap_or(false) {
            return Ok(DeliveryAck {
                acknowledged: true,
                details: format!(
                    "Dispatched to {} session {}",
                    target.harness.display_name(),
                    target.short_session_id()
                ),
            });
        }

        let mut child_handle: Option<Child> = None;

        match target.harness {
            HarnessKind::Agy => {
                let agy_bin = std::env::var("AGY_BIN").unwrap_or_else(|_| "agy".to_string());
                let mut cmd = Command::new(&agy_bin);
                if agy_bin.ends_with("sleep") {
                    cmd.arg(prompt);
                } else {
                    cmd.args(["--conversation", &target.session_id, "-p", prompt]);
                }
                cmd.stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                if !target.cwd.is_empty() && std::path::Path::new(&target.cwd).exists() {
                    cmd.current_dir(&target.cwd);
                }
                match cmd.spawn() {
                    Ok(c) => child_handle = Some(c),
                    Err(e) => {
                        return Err(TerminalError::CommandFailed(format!(
                            "Failed to spawn {}: {}",
                            agy_bin, e
                        )));
                    }
                }
            }
            HarnessKind::Codex => {
                let codex_bin = std::env::var("CODEX_BIN").unwrap_or_else(|_| "codex".to_string());
                let mut cmd = Command::new(&codex_bin);
                if codex_bin.ends_with("sleep") {
                    cmd.arg(prompt);
                } else {
                    cmd.args(["queue", "--thread", &target.session_id, "--message", prompt]);
                }
                cmd.stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                if !target.cwd.is_empty() && std::path::Path::new(&target.cwd).exists() {
                    cmd.current_dir(&target.cwd);
                }
                match cmd.spawn() {
                    Ok(c) => child_handle = Some(c),
                    Err(e) => {
                        return Err(TerminalError::CommandFailed(format!(
                            "Failed to spawn {}: {}",
                            codex_bin, e
                        )));
                    }
                }
            }
            HarnessKind::ClaudeCode => {
                let claude_bin = std::env::var("CLAUDE_BIN").unwrap_or_else(|_| "claude".to_string());
                let mut cmd = Command::new(&claude_bin);
                if claude_bin.ends_with("sleep") {
                    cmd.arg(prompt);
                } else {
                    cmd.args(["-p", prompt]);
                }
                cmd.stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                if !target.cwd.is_empty() && std::path::Path::new(&target.cwd).exists() {
                    cmd.current_dir(&target.cwd);
                }
                match cmd.spawn() {
                    Ok(c) => child_handle = Some(c),
                    Err(e) => {
                        return Err(TerminalError::CommandFailed(format!(
                            "Failed to spawn {}: {}",
                            claude_bin, e
                        )));
                    }
                }
            }
            _ => {
                // If in testing environment and MOCK_SPAWN is set, simulate delivery
                if std::env::var("MOCK_SPAWN").map(|v| v == "1").unwrap_or(false) {
                    // Simulated delivery without spawning external process
                } else {
                    return Err(TerminalError::CommandFailed(format!(
                        "Fallback dispatch not supported for harness {}",
                        target.harness.display_name()
                    )));
                }
            }
        }

        if let Some(c) = child_handle {
            let mut lock = self.active_child.lock().unwrap();
            *lock = Some(c);
        }

        Ok(DeliveryAck {
            acknowledged: true,
            details: format!(
                "Dispatched to {} session {}",
                target.harness.display_name(),
                target.short_session_id()
            ),
        })
    }

    fn verify_target_alive(&self, _target: &AgentTarget) -> Result<bool, TerminalError> {
        Ok(true)
    }

    fn get_agent_preview(&self, _pane_id: &str) -> Option<String> {
        None
    }
}
